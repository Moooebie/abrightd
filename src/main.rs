//! `abrightd` CLI: run the daemon, replay a CSV trace, or calibrate.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicI32;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

use abrightd::als::iio::IioSysfs;
use abrightd::als::replay::Replay;
use abrightd::als::AlsSource;
use abrightd::clock::SystemClock;
use abrightd::config::{
    Config, CurveConfig, HysteresisSection, LearningConfig, RampSection, TimingConfig,
};
use abrightd::controller::AutomaticBrightnessController;
use abrightd::daemon::{self, Daemon};
#[cfg(feature = "dbus")]
use abrightd::desktop;
use abrightd::desktop::Desktop;
use abrightd::desktop::Event;
use abrightd::mapping::{
    infer_auto_brightness_adjustment, BrightnessMappingStrategy, SimpleMappingStrategy,
};
use abrightd::output::sysfs::SysfsBacklight;
use abrightd::output::BacklightSink;
use abrightd::ramp::Ramp;
use abrightd::state::PersistedState;
use abrightd::status::SharedState;

/// Version shown by `--version`; `dev` off a release tag (see `build.rs`).
const VERSION: &str = env!("ABRIGHTD_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "abrightd",
    about = "AOSP automatic brightness for GNU/Linux",
    version = VERSION
)]
struct Cli {
    /// Path to a TOML profile.  Built-in defaults are used if omitted.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Replay a `timestamp_ms,lux` CSV instead of reading the sensor.
    #[arg(long)]
    replay: Option<PathBuf>,

    /// With `--replay`, print one line per step.
    #[arg(long)]
    dump_brightness: bool,

    /// Do not touch the backlight (useful for testing the sensor path).
    #[arg(long)]
    dry_run: bool,

    /// Show a live brightness indicator (requires the `tui` feature).
    #[arg(long)]
    tui: bool,

    /// TUI refresh interval in milliseconds.
    #[arg(long, default_value_t = 250)]
    interval_ms: u64,

    /// Log level (`error`, `warn`, `info`, `debug`, `trace`).
    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Inspect and adjust the ambient-light brightness curve.
    Calibrate {
        #[command(subcommand)]
        action: CalibrateAction,
    },
    /// Inspect desktop-environment integration (KDE/GNOME).
    Integrate {
        #[command(subcommand)]
        action: IntegrateAction,
    },
    /// Manage the configuration profile file.
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}

#[derive(Subcommand, Debug)]
enum IntegrateAction {
    /// Report the detected desktop, brightness ownership and conflicts.
    Detect,
}

#[derive(Subcommand, Debug)]
enum ProfileAction {
    /// Reset the profile's calibration sections to the built-in defaults.
    Reset(ResetArgs),
}

#[derive(Args, Debug)]
struct ResetArgs {
    /// Back up the existing file before resetting.
    #[arg(long)]
    backup: bool,
    /// Do not prompt for confirmation.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand, Debug)]
enum CalibrateAction {
    /// Print the effective lux -> brightness curve and live readings.
    Show,
    /// Set the global auto-brightness adjustment.
    Adjust(AdjustArgs),
    /// Reset the learned calibration (adjustment and user points).
    Reset(ResetArgs),
}

#[derive(Args, Debug)]
struct AdjustArgs {
    /// Explicit adjustment in [-1, 1]. Positive raises the whole curve.
    #[arg(long, conflicts_with = "point")]
    value: Option<f32>,

    /// Infer the adjustment from a preferred brightness at a given lux.
    #[arg(long, num_args = 2, value_names = ["LUX", "BRIGHTNESS"], conflicts_with = "value")]
    point: Option<Vec<f32>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);

    // Resolve the profile: explicit --config, else the user's default profile.
    let config_path = cli.config.clone().or_else(default_config_path);
    let config = match &config_path {
        Some(path) => Config::load(path).with_context(|| format!("loading {}", path.display()))?,
        None => Config::default(),
    };
    let config_source = match &config_path {
        Some(path) => path.display().to_string(),
        None => "built-in defaults".into(),
    };

    if let Some(Command::Calibrate { action }) = &cli.command {
        return match action {
            CalibrateAction::Show => run_calibrate_show(&config, &config_source).await,
            CalibrateAction::Adjust(args) => run_calibrate_adjust(&config, args).await,
            CalibrateAction::Reset(args) => run_calibrate_reset(args).await,
        };
    }

    if let Some(Command::Integrate { action }) = &cli.command {
        return match action {
            IntegrateAction::Detect => run_integrate_detect(&config).await,
        };
    }

    if let Some(Command::Profile { action }) = &cli.command {
        return match action {
            ProfileAction::Reset(args) => run_profile_reset(&config_path, args),
        };
    }

    if let Some(path) = &cli.replay {
        return run_replay(path, &config, cli.dump_brightness);
    }

    if cli.tui {
        return run_tui(cli.interval_ms).await;
    }

    run_daemon(config, cli.dry_run).await
}

/// The user's default profile path (`~/.config/abrightd/config.toml`),
/// whether or not it exists.
fn user_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("abrightd/config.toml")
}

/// The user's default profile path, if the file exists.  Used when `--config`
/// is not given so CLI commands reflect the running setup instead of built-in
/// defaults.
fn default_config_path() -> Option<PathBuf> {
    let path = user_config_path();
    path.exists().then_some(path)
}

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("abrightd={level},warn")));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Build the mapping strategy from config, with no calibration applied.
fn build_mapper(config: &Config) -> anyhow::Result<SimpleMappingStrategy> {
    config.mapper()
}

/// The persisted user point as `(lux, brightness)`, if any.
fn state_point(state: &PersistedState) -> Option<(f32, f32)> {
    state.single_user_point().map(|p| (p.lux, p.brightness))
}

// ---------------------------------------------------------------------------
// Calibration
// ---------------------------------------------------------------------------

/// Sweep points used by `calibrate show`.
const SHOW_LUX: [f32; 12] = [
    0.0, 1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 1000.0, 3000.0, 10_000.0, 30_000.0, 100_000.0,
];

async fn run_calibrate_show(config: &Config, config_source: &str) -> anyhow::Result<()> {
    let state = PersistedState::load();
    let adjustment = state.adjustment_clamped();
    let point = state_point(&state);

    let mut mapper = build_mapper(config)?;
    let base: Vec<f32> = SHOW_LUX
        .iter()
        .map(|lux| mapper.get_brightness(*lux))
        .collect();
    mapper.set_auto_brightness_adjustment(adjustment);
    if let Some((lux, brightness)) = point {
        mapper.restore_user_point(lux, brightness);
    }
    let adjusted: Vec<f32> = SHOW_LUX
        .iter()
        .map(|lux| mapper.get_brightness(*lux))
        .collect();

    println!("abrightd calibration");
    println!("  config      {config_source}");
    println!("  state       {}", abrightd::state::state_path().display());
    println!(
        "  adjustment  {adjustment:+.3}   (max_gamma {:.1})",
        config.curve.max_gamma
    );
    match sensor_description(config) {
        Some(description) => println!("  sensor      {description}"),
        None => println!("  sensor      [als] kind={}", config.als.kind),
    }
    match point {
        Some((lux, brightness)) => {
            println!("  user point  lux {lux:.2}, brightness {brightness:.4}")
        }
        None => println!("  user point  none"),
    }

    println!();
    println!("  {:>10}  {:>10}  {:>10}", "lux", "base", "adjusted");
    for (i, lux) in SHOW_LUX.iter().enumerate() {
        println!("  {lux:>10.1}  {:>10.4}  {:>10.4}", base[i], adjusted[i]);
    }

    #[cfg(feature = "dbus")]
    if let Ok(status) = abrightd::dbus::fetch_status().await {
        let get = |key: &str| status.get(key).cloned().unwrap_or_else(|| "—".into());
        println!();
        println!(
            "  live        lux {}   slow {}   fast {}   output {}   adjustment {}",
            get("lux"),
            get("slow_lux"),
            get("fast_lux"),
            get("output_brightness"),
            get("adjustment"),
        );
    }
    Ok(())
}

async fn run_calibrate_adjust(config: &Config, args: &AdjustArgs) -> anyhow::Result<()> {
    let new_adjustment = if let Some(value) = args.value {
        value.clamp(-1.0, 1.0)
    } else if let Some(point) = &args.point {
        let (lux, desired) = (point[0], point[1]);
        // Infer against the *unadjusted* base curve, as AOSP does.
        let mapper = config.mapper()?;
        let current = mapper.get_brightness(lux);
        let inferred = infer_auto_brightness_adjustment(config.curve.max_gamma, desired, current);
        println!(
            "  base brightness at {lux:.1} lx = {current:.4}; desired {desired:.4} -> adjustment {inferred:+.3}"
        );
        inferred
    } else {
        anyhow::bail!("specify --value <a> or --point <lux> <brightness>");
    };

    // Prefer applying live through the running daemon (which also persists).
    #[cfg(feature = "dbus")]
    {
        match abrightd::dbus::set_adjustment(new_adjustment).await {
            Ok(()) => {
                println!("  adjustment set to {new_adjustment:+.3} via org.abrightd (persisted)");
                return Ok(());
            }
            Err(err) => {
                println!("  daemon not reachable ({err}); saving to disk instead");
            }
        }
    }

    let mut state = PersistedState::load();
    state.schema_version = PersistedState::SCHEMA_VERSION;
    state.adjustment = new_adjustment;
    state.save()?;
    println!(
        "  adjustment saved to {} ({new_adjustment:+.3}); it applies on next start",
        abrightd::state::state_path().display()
    );
    Ok(())
}

/// Ask for confirmation unless `--yes` was given or stdin is not a terminal.
fn confirm(prompt: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        return Ok(true);
    }
    eprint!("{prompt} [y/N] ");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

async fn run_calibrate_reset(args: &ResetArgs) -> anyhow::Result<()> {
    if !confirm(
        "Reset calibration (adjustment and user points) to uncalibrated defaults?",
        args.yes,
    )? {
        println!("aborted");
        return Ok(());
    }

    if args.backup {
        let path = abrightd::state::state_path();
        if path.exists() {
            let backup = path.with_extension("toml.bak");
            std::fs::copy(&path, &backup)?;
            println!("  backed up {} -> {}", path.display(), backup.display());
        }
    }

    // Prefer the running daemon (which also persists).
    #[cfg(feature = "dbus")]
    {
        match abrightd::dbus::reset_calibration().await {
            Ok(()) => {
                println!("  calibration reset via org.abrightd (persisted)");
                return Ok(());
            }
            Err(err) => println!("  daemon not reachable ({err}); clearing state file"),
        }
    }

    PersistedState::cleared().save()?;
    println!("  cleared {}", abrightd::state::state_path().display());
    Ok(())
}

fn run_profile_reset(config_path: &Option<PathBuf>, args: &ResetArgs) -> anyhow::Result<()> {
    let target = config_path.clone().unwrap_or_else(user_config_path);
    if !confirm(
        &format!(
            "Reset profile {} calibration sections to built-in defaults?",
            target.display()
        ),
        args.yes,
    )? {
        println!("aborted");
        return Ok(());
    }

    if args.backup && target.exists() {
        let backup = target.with_extension("toml.bak");
        std::fs::copy(&target, &backup)?;
        println!("  backed up {} -> {}", target.display(), backup.display());
    }

    // Keep the device/DE wiring; reset the calibration-relevant sections.
    let mut config = if target.exists() {
        Config::load(&target)?
    } else {
        Config::default()
    };
    config.curve = CurveConfig::default();
    config.hysteresis = HysteresisSection::default();
    config.timing = TimingConfig::default();
    config.ramp = RampSection::default();
    config.learning = LearningConfig::default();

    let text = format!(
        "# abrightd profile (calibration reset to built-in defaults)\n\
         # [als], [output] and [integration] are preserved from the previous profile.\n\n{}",
        toml::to_string_pretty(&config)?
    );
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, text)?;
    println!("  wrote {}", target.display());
    println!("  restart to apply: systemctl --user restart abrightd");
    Ok(())
}

fn sensor_description(config: &Config) -> Option<String> {
    if config.als.kind != "iio" {
        return None;
    }
    let clock: Arc<dyn abrightd::clock::Clock> = Arc::new(SystemClock::new());
    IioSysfs::discover(config.als.device.as_deref(), config.als.poll_rate_ms, clock)
        .ok()
        .map(|source| {
            format!(
                "{}  (lux_multiplier {})",
                source.description(),
                config.als.lux_multiplier
            )
        })
}

// ---------------------------------------------------------------------------
// Desktop integration
// ---------------------------------------------------------------------------

async fn run_integrate_detect(config: &Config) -> anyhow::Result<()> {
    let desktop = Desktop::detect();
    println!("abrightd desktop integration");
    println!("  desktop        {}", desktop.name());
    println!("  output kind    {} (configured)", config.output.kind);
    println!("  watch user     {}", config.integration.watch_user_changes);
    println!(
        "  pause          locked={} suspend={}",
        config.integration.pause_when_locked, config.integration.pause_on_suspend
    );

    #[cfg(feature = "dbus")]
    {
        if desktop == Desktop::Kde {
            let diag = desktop::kde::diagnose().await;
            match (diag.brightness, diag.max) {
                (Some(b), Some(m)) if m > 0 => println!(
                    "  powerdevil     {b} / {m} ({:.1}%)",
                    b as f32 / m as f32 * 100.0
                ),
                _ => println!("  powerdevil     not reachable"),
            }
            if let Some((raw, max)) = diag.sysfs {
                if max > 0 {
                    println!(
                        "  sysfs panel    {raw} / {max} ({:.1}%)",
                        raw as f32 / max as f32 * 100.0
                    );
                }
            }
            match diag.ambient_auto_brightness {
                Some(true) => println!(
                    "  DE ALS auto    ENABLED — conflicts with abrightd; disable it (or use --takeover)"
                ),
                Some(false) => println!("  DE ALS auto    disabled"),
                None => println!(
                    "  DE ALS auto    not supported in this Plasma build (no conflict)"
                ),
            }
            if diag.is_stale() {
                println!(
                    "  note           PowerDevil's value disagrees with the panel; use [output] kind = \"kde\" to keep it in sync"
                );
            }
        }
    }

    #[cfg(not(feature = "dbus"))]
    println!("  (built without --features dbus: diagnostics limited)");

    Ok(())
}

// ---------------------------------------------------------------------------
// Replay / daemon / TUI
// ---------------------------------------------------------------------------

/// Synchronous, deterministic replay harness.
fn run_replay(path: &Path, config: &Config, dump: bool) -> anyhow::Result<()> {
    let replay = Replay::from_path(path).with_context(|| format!("reading {}", path.display()))?;
    let samples = replay.into_vec();

    let state = PersistedState::load();
    let mut controller = AutomaticBrightnessController::new(
        config.controller_config()?,
        Box::new(build_mapper(config)?),
    );
    controller.restore_calibration(state.adjustment_clamped(), state_point(&state));
    let mut ramp = Ramp::new(config.ramp_config());

    let points = daemon::replay(&mut controller, &mut ramp, &samples);

    if dump {
        println!("# time_ms\tlux\tcontroller\toutput");
        for p in &points {
            println!(
                "{}\t{:.3}\t{:.4}\t{:.4}",
                p.time_ms, p.lux, p.controller_brightness, p.output
            );
        }
    } else {
        let applied: Vec<f32> = points
            .iter()
            .filter(|p| !p.output.is_nan())
            .map(|p| p.output)
            .collect();
        let first = applied.first().copied().unwrap_or(f32::NAN);
        let last = applied.last().copied().unwrap_or(f32::NAN);
        println!(
            "replayed {} points; brightness {:.4} -> {:.4}",
            points.len(),
            first,
            last
        );
    }
    Ok(())
}

#[cfg(feature = "tui")]
async fn run_tui(interval_ms: u64) -> anyhow::Result<()> {
    abrightd::tui::run(interval_ms).await
}

#[cfg(not(feature = "tui"))]
async fn run_tui(_interval_ms: u64) -> anyhow::Result<()> {
    anyhow::bail!("TUI support was not compiled in; rebuild with --features tui")
}

async fn run_daemon(config: Config, dry_run: bool) -> anyhow::Result<()> {
    let clock: Arc<dyn abrightd::clock::Clock> = Arc::new(SystemClock::new());

    let state = PersistedState::load();
    let mut controller = AutomaticBrightnessController::new(
        config.controller_config()?,
        Box::new(build_mapper(&config)?),
    );
    controller.restore_calibration(state.adjustment_clamped(), state_point(&state));
    if state.adjustment_clamped() != 0.0 || state.single_user_point().is_some() {
        info!(
            "loaded persisted calibration: adjustment {:+}, point {:?}",
            state.adjustment_clamped(),
            state_point(&state)
        );
    }
    let ramp = Ramp::new(config.ramp_config());

    let mut als: Box<dyn AlsSource> = match config.als.kind.as_str() {
        "iio" => Box::new(
            IioSysfs::discover(
                config.als.device.as_deref(),
                config.als.poll_rate_ms,
                clock.clone(),
            )?
            .with_lux_multiplier(config.als.lux_multiplier),
        ),
        "replay" => {
            let path = config.als.replay_path.as_deref().ok_or_else(|| {
                anyhow::anyhow!("[als] replay_path is required for kind = \"replay\"")
            })?;
            Box::new(Replay::from_path(std::path::Path::new(path))?)
        }
        other => anyhow::bail!("unknown [als] kind: {other}"),
    };

    let last_commanded = Arc::new(AtomicI32::new(-1));
    let _ = &last_commanded;

    let sink: Box<dyn BacklightSink> = if dry_run {
        Box::new(NullSink)
    } else {
        match config.output.kind.as_str() {
            "sysfs" => Box::new(SysfsBacklight::discover(config.output.device.as_deref())?),
            #[cfg(feature = "dbus")]
            "logind" => Box::new(
                abrightd::output::logind::LogindBacklight::connect(config.output.device.as_deref())
                    .await?,
            ),
            #[cfg(feature = "dbus")]
            "kde" => Box::new(desktop::kde::KdeSink::connect(last_commanded.clone()).await?),
            #[cfg(not(feature = "dbus"))]
            "logind" | "kde" => anyhow::bail!(
                "{0} support was not compiled in; rebuild with --features dbus or use [output] kind = \"sysfs\"",
                config.output.kind
            ),
            other => anyhow::bail!("unknown [output] kind: {other}"),
        }
    };

    // Desktop/session event channel: user brightness changes, lock, suspend.
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<Event>(64);
    #[cfg(feature = "dbus")]
    {
        let desktop = Desktop::detect();
        if config.integration.pause_when_locked || config.integration.pause_on_suspend {
            desktop::spawn_session_monitor(event_tx.clone());
        }
        if config.integration.watch_user_changes && desktop == Desktop::Kde {
            desktop::kde::spawn_brightness_monitor(event_tx.clone(), last_commanded.clone());
        }
    }
    drop(event_tx);

    info!("starting abrightd {VERSION}");
    let shared = Arc::new(Mutex::new(SharedState::default()));

    // Keep the D-Bus connection alive for the lifetime of the daemon; dropping
    // it would release the `org.abrightd` name.
    #[cfg(feature = "dbus")]
    let _dbus_conn = match abrightd::dbus::serve(shared.clone()).await {
        Ok(conn) => {
            info!("org.abrightd D-Bus service ready");
            Some(conn)
        }
        Err(err) => {
            tracing::warn!("could not start D-Bus service: {err:#}");
            None
        }
    };

    let mut daemon = Daemon::new(
        controller,
        ramp,
        config.ramp.min_interval_ms,
        sink,
        clock,
        Some(shared),
        Some(event_rx),
    );
    daemon.run(als.as_mut()).await
}

/// A sink used by `--dry-run`.
struct NullSink;

#[async_trait::async_trait]
impl BacklightSink for NullSink {
    async fn set(&self, fraction: f32) -> anyhow::Result<()> {
        info!("dry-run backlight -> {fraction:.4}");
        Ok(())
    }

    async fn max_raw(&self) -> anyhow::Result<u32> {
        Ok(0)
    }

    fn description(&self) -> String {
        "null (dry-run)".into()
    }
}
