# abrightd

A standalone Rust userspace daemon implementing Android's **automatic
brightness** pipeline on GNU/Linux, sourced from an IIO ambient-light sensor and
driving the backlight through logind D-Bus (preferred) or sysfs (fallback).

This is a faithful port of the AOSP classes in
`frameworks/base/services/core/java/com/android/server/display/`:

| AOSP component | This crate |
|---|---|
| `AutomaticBrightnessController` | `src/controller.rs` |
| `AmbientLightRingBuffer` + weighted lux | `src/ring_buffer.rs` |
| `HysteresisLevels` | `src/hysteresis.rs` |
| `android.util.Spline` | `src/spline.rs` |
| `BrightnessMappingStrategy.SimpleMappingStrategy` | `src/mapping.rs` |
| `ShortTermModel` / user learning | `src/short_term.rs` |

See [`docs/SPECIFICATIONS.md`](docs/SPECIFICATIONS.md) for the full
specification, and [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for
maintainer-facing implementation notes (core invariants, module map, AOSP
fidelity, debugging playbook).

## Status

| Phase | Content | State |
|---|---|---|
| 0 | Scaffold, `Clock`, replay harness | ✅ |
| 1 | `spline`, `hysteresis`, `ring_buffer`, `SimpleMappingStrategy`, `ShortTermModel` | ✅ |
| 2 | `AutomaticBrightnessController` state machine | ✅ |
| 3 | IIO `AlsSource` | ✅ |
| 4 | logind + sysfs `BacklightSink` + ramp | ✅ (logind behind the `dbus` feature) |
| 5 | config, D-Bus, systemd unit | ✅ (D-Bus behind the `dbus` feature) |
| 5b | Calibration (Tier 0 `show`, Tier 2 global adjustment) | ✅ |
| 5c | KDE integration (PowerDevil sink, user-override learning, lock/suspend) | ✅ |
| 6 | Calibration tool + default profiles | partial (defaults shipped) |
| 7 | Docs / soak test | partial |

The pure core is I/O-free, takes time as an explicit argument, and is covered by
unit tests plus a deterministic replay harness (no hardware required).

## Build

```sh
cargo build                      # daemon + sysfs output
cargo build --features dbus      # + logind output and the org.abrightd service
cargo build --features tui       # + the live brightness indicator (implies dbus)
cargo test --all-features
cargo build --release --features tui
```

The dependency set matches `docs/SPECIFICATIONS.md`: `tokio`, `zbus`, `clap`, `serde`/`toml`,
`thiserror`/`anyhow`, `tracing`/`tracing-subscriber`, `async-trait` and
`proptest`.

## Run

As a user service (sysfs output):

```sh
abrightd --config ~/.config/abrightd/config.toml
```

With the D-Bus service built in, `[output] kind = "logind"` routes brightness
through logind, and `org.abrightd` exposes `Enable`, `SetProfile`,
`AddUserPoint`, `ClearUserPoints` and `Status`.

### Brightness indicator (TUI)

Build with `--features tui` and run the indicator in a terminal while the daemon
is running:

```sh
abrightd --tui                 # refresh every 250 ms
abrightd --tui --interval-ms 100
```

```
  abrightd  ·  AOSP auto-brightness
  ────────────────────────────────────────────────────

  ambient          14.63 lx   slow 15.7    fast 15.7     ● steady
  backlight   ████████░░░░░░░░░░░░░░░░░░░░    29.0%
                              target 90.0%  learned

  adjust       ▲ brightening
  state        enabled=true  profile=default

  q / Esc / Ctrl-C to quit
```

It reads the daemon's `Status` over D-Bus, so it shows the ambient lux (and the
slow/fast estimates), the commanded backlight with the ramp target marked, the
light trend and the current adjustment direction (`▲ brightening` /
`▼ darkening` / `● steady`).  It exits with `q`, `Esc` or `Ctrl-C`.

### Desktop integration (KDE)

On KDE Plasma, set the output to PowerDevil so the Plasma brightness UI and OSD
stay in sync, and abrightd can learn from the brightness keys:

```toml
[output]
kind = "kde"          # drives PowerDevil's BrightnessControl

[integration]
watch_user_changes = true   # treat brightness keys/slider as user intent
pause_when_locked = true    # stop adjusting while the session is locked
pause_on_suspend = true
```

```sh
abrightd integrate detect
```

```
abrightd desktop integration
  desktop        KDE Plasma
  output kind    kde (configured)
  watch user     true
  powerdevil     1425 / 10000 (14.2%)
  sysfs panel    2190 / 15360 (14.3%)
  DE ALS auto    not supported in this Plasma build (no conflict)
```

`integrate detect` reports the desktop, the PowerDevil brightness (and whether
it disagrees with the real panel — i.e. whether `kind = "kde"` is worth using),
and whether the desktop has its own ambient-light auto-brightness that would
conflict.

When a user presses the brightness keys or moves the Plasma slider, PowerDevil
emits `brightnessChanged`; abrightd treats that as a user override, records it
against the current lux (AOSP short-term learning) and adopts it instead of
overwriting it.  Auto-adjustments are written with `setBrightnessSilent` so they
do not spam the OSD.

> Note: `kind = "logind"` still works everywhere and needs fewer moving parts,
> but the Plasma brightness UI will show a stale value.  `kind = "kde"` is the
> right choice on Plasma.

#### Plasma applet (on/off switch)

A minimal Plasma 6 widget lives in `dist/plasma-applet-org.kde.abrightd`:

```sh
cd dist/plasma-applet-org.kde.abrightd
./install.sh          # builds the QML D-Bus bridge (sudo) and installs the applet
```

Then add **Auto Brightness** via *right-click panel → Add Widgets*.  The panel
icon toggles abrightd on/off; the popup shows a switch and the live lux /
brightness.  If the widget reports a missing QML module, restart Plasma once:

```sh
systemctl --user restart plasma-plasmashell.service
```

Remove it with `./uninstall.sh`.

Deterministic replay (no sensor or backlight needed):

```sh
abrightd --replay tests/replay_trace.csv --dump-brightness
# time_ms  lux    controller  output
```

Dry run against the real sensor without touching the backlight:

```sh
abrightd --dry-run --log-level debug
```

## Measured components

The pipeline is: `AlsSource` → `AutomaticBrightnessController` (ring buffer,
slow/fast lux, ambient + screen hysteresis, mapping + short-term learning) →
`Ramp` → `BacklightSink`.

Two traits keep the core testable: [`als::AlsSource`] and
[`output::BacklightSink`].  The controller never performs I/O.

## Configuration

See `examples/abrightd.toml`.  The curve seeds from AOSP's
`config_autoBrightnessLuxLevels` / brightness defaults and is meant to be
calibrated on the target hardware (dark room / office / outdoors).

## Calibration

The base curve is a generic seed, so it usually needs adjusting per panel.
`abrightd` exposes AOSP's global auto-brightness adjustment — a gamma applied
to the whole curve, `b' = b^(max_gamma^−adjustment)` — and persists it.

```sh
# Show the effective lux -> brightness curve and live readings
abrightd calibrate show

# Set the adjustment directly (positive raises the whole curve)
abrightd calibrate adjust --value 0.3

# Or infer it from a preferred brightness at a given lux
abrightd calibrate adjust --point 40 0.20
```

`--point <lux> <brightness>` infers the adjustment against the *unadjusted*
base curve using AOSP's `inferAutoBrightnessAdjustment`, so the curve then
passes through your chosen brightness at that lux.

The adjustment is stored in `$XDG_STATE_HOME/abrightd/state.toml`
(`~/.local/state/abrightd/state.toml` by default).  When the daemon is running
it is applied live through `org.abrightd` (and persisted by the daemon);
otherwise it is written to disk and takes effect on the next start.

This is the first calibration tier.  Planned follow-ups: sensor (lux)
calibration, a guided multi-point wizard, and continuous local learning from
your brightness changes.

## Hardware bring-up

```sh
ls /sys/bus/iio/devices                    # confirm an illuminance channel
for d in /sys/bus/iio/devices/*/; do
    grep -l . "$d"in_illuminance* 2>/dev/null && echo "  <- $d"
done
cat /sys/class/backlight/*/max_brightness  # confirm the backlight range
```

Notes from the reference machine (Lenovo, Intel HID sensor hub):

* The sensor is named `als` and exposes **`in_illuminance_raw`** with
  `in_illuminance_scale` / `in_illuminance_offset`, not `in_illuminance_input`.
  `IioSysfs` probes all three names and applies `lux = raw * scale + offset`,
  then the optional `[als] lux_multiplier` calibration factor.
* It only emits new values **on change**; the source synthesises a sample every
  poll interval so the slow/fast horizons and debounce timers behave correctly.
* `intel_backlight` is root-only; logind `Session.SetBrightness` works for the
  active session without root, so use `[output] kind = "logind"` (build with
  `--features dbus`).

Install the udev rule (`udev/90-abrightd-backlight.rules`) so the user session
can write `brightness` without root, and the systemd user unit
(`systemd/abrightd.service`).

## AOSP fidelity notes

* Boundary handling, edge cases (`current <= 0.1 || >= 0.9`, `desired == 0/1`),
  the Fritsch–Carlson tangent limiting, the `MIN_PERMISSABLE_INCREASE = 0.004`
  curve smoothing, and the `0.6` short-term-model band are ported verbatim.
* `AMBIENT_LIGHT_PREDICTION_TIME_MILLIS = 100` is preserved.
* AOSP's two clocks (`CLOCK_BOOTTIME` for sensor scale, `CLOCK_MONOTONIC` for
  timers) are both available through `clock::Clock`; the daemon runs on
  `CLOCK_BOOTTIME` so horizons and debounce timers stay consistent across
  suspend.
* **Addition over AOSP:** the IIO source emits a periodic sample every poll
  interval even when the sensor only reports on change, so the slow/fast
  horizons behave correctly on such sensors.

## License

Portions are derived from AOSP (`Apache-2.0`); headers/notices are retained.
