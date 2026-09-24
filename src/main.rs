//! `abrightd` CLI: run the daemon, replay a CSV trace, or calibrate.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

use abrightd::als::iio::IioSysfs;
use abrightd::als::replay::Replay;
use abrightd::als::AlsSource;
use abrightd::clock::SystemClock;
use abrightd::config::Config;
use abrightd::controller::AutomaticBrightnessController;
use abrightd::daemon::{self, Daemon};
use abrightd::mapping::{infer_auto_brightness_adjustment, BrightnessMappingStrategy};
use abrightd::output::sysfs::SysfsBacklight;
use abrightd::output::BacklightSink;
use abrightd::ramp::Ramp;
use abrightd::state::PersistedState;
use abrightd::status::SharedState;

#[derive(Parser, Debug)]
#[command(name = "abrightd", about = "AOSP automatic brightness for GNU/Linux")]
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
}

#[derive(Subcommand, Debug)]
enum CalibrateAction {
    /// Print the effective lux -> brightness curve and live readings.
    Show,
    /// Set the global auto-brightness adjustment.
    Adjust(AdjustArgs),
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

    let config = match &cli.config {
        Some(path) => Config::load(path).with_context(|| format!("loading {}", path.display()))?,
        None => Config::default(),
    };
    let config_source = match &cli.config {
        Some(path) => path.display().to_string(),
        None => "built-in defaults".into(),
    };

    if let Some(Command::Calibrate { action }) = &cli.command {
        return match action {
            CalibrateAction::Show => run_calibrate_show(&config, &config_source).await,
            CalibrateAction::Adjust(args) => run_calibrate_adjust(&config, args).await,
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

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("abrightd={level},warn")));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Build the mapping strategy from config and apply the persisted adjustment.
fn mapper_with_state(
    config: &Config,
    state: &PersistedState,
) -> anyhow::Result<Box<dyn BrightnessMappingStrategy>> {
    let mut mapper = config.mapper()?;
    let adjustment = state.adjustment_clamped();
    if adjustment != 0.0 {
        mapper.set_auto_brightness_adjustment(adjustment);
    }
    Ok(Box::new(mapper))
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

    let mut mapper = config.mapper()?;
    let base: Vec<f32> = SHOW_LUX
        .iter()
        .map(|lux| mapper.get_brightness(*lux))
        .collect();
    mapper.set_auto_brightness_adjustment(adjustment);
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
// Replay / daemon / TUI
// ---------------------------------------------------------------------------

/// Synchronous, deterministic replay harness.
fn run_replay(path: &Path, config: &Config, dump: bool) -> anyhow::Result<()> {
    let replay = Replay::from_path(path).with_context(|| format!("reading {}", path.display()))?;
    let samples = replay.into_vec();

    let state = PersistedState::load();
    let mapper = mapper_with_state(config, &state)?;
    let mut controller = AutomaticBrightnessController::new(config.controller_config()?, mapper);
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
    let mapper = mapper_with_state(&config, &state)?;
    if state.adjustment_clamped() != 0.0 {
        info!(
            "loaded persisted adjustment {:+}",
            state.adjustment_clamped()
        );
    }
    let controller = AutomaticBrightnessController::new(config.controller_config()?, mapper);
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
            #[cfg(not(feature = "dbus"))]
            "logind" => anyhow::bail!(
                "logind support was not compiled in; rebuild with --features dbus or use [output] kind = \"sysfs\""
            ),
            other => anyhow::bail!("unknown [output] kind: {other}"),
        }
    };

    info!("starting abrightd");
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
