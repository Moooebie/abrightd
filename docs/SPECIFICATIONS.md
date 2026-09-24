# Specifications: Android Auto-Brightness for GNU/Linux

**Project:** `autobrightnessd` (working name `abrightd`)
**Status:** Implemented — see [`ARCHITECTURE.md`](ARCHITECTURE.md) for the
maintainer-facing implementation notes.
**Deliverable:** A standalone Rust userspace daemon implementing the full AOSP
auto-brightness pipeline (weighted smoothing, slow/fast ambient lux, ambient +
screen hysteresis, spline mapping, and short-term user learning), sourcing lux
from an IIO ambient light sensor and driving backlight through logind D-Bus with
a sysfs fallback.

**Target hardware:** laptop with an IIO ambient light sensor (ALS).

---

## 1. What we are porting

Android's auto-brightness is *not* a single algorithm. It is a small userspace
stack living in `frameworks/base/services/core/java/com/android/server/display/`.

| AOSP component | Responsibility |
|---|---|
| `AutomaticBrightnessController` | Orchestrator: sensor events, timing, debounce, hysteresis, state machine, user-adjustment sampling |
| `AmbientLightRingBuffer` | Timestamped lux ring buffer |
| `calculateAmbientLux` / `calculateWeight` | Weighted, time-horizon smoothing of lux |
| `HysteresisLevels` | Brightening/darkening thresholds for both ambient lux and screen brightness |
| `BrightnessMappingStrategy` | lux → brightness curve; `PhysicalMappingStrategy` (lux→nits→backlight) and `SimpleMappingStrategy` (lux→backlight) |
| `Spline` (Fritsch–Carlson monotone cubic) | Interpolation of the control points |
| `ShortTermModel` + `addUserDataPoint` | Per-user "learning" from manual brightness overrides |
| `BrightnessSynchronizer` / `BrightnessUtils` | nits↔backlight and linear↔gamma conversion |

### 1.1 Constants to preserve

| Constant | Value | Purpose |
|---|---|---|
| `AMBIENT_LIGHT_PREDICTION_TIME_MILLIS` | 100 | Prediction horizon end-offset for weighting |
| `AMBIENT_LIGHT_HORIZON_LONG` | ~10 000 ms | Slow ambient estimate window |
| `AMBIENT_LIGHT_HORIZON_SHORT` | ~2 000 ms | Fast ambient estimate window |
| `BRIGHTNESS_ADJUSTMENT_SAMPLE_DEBOUNCE_MILLIS` | 10 000 | Debounce before storing a manual brightness sample |
| `LUX_GRAD_SMOOTHING` | 0.25 | Curve-smoothing lux offset |
| `MAX_GRAD` | 1.0 | Curve-smoothing exponent |
| `SHORT_TERM_MODEL_THRESHOLD_RATIO` | 0.6 | ±band around user anchor that keeps the short-term model |
| `MIN_PERMISSABLE_INCREASE` | 0.004 | Guarantees a monotone curve can always increase |
| Normal sensor rate | ~200 ms | Steady-state sample period |
| Initial sensor rate | ~1000 ms | Fast initial sample period after enabling |
| Brightening debounce | ~2000 ms | Dwell required before brightening |
| Darkening debounce | ~4000 ms | Dwell required before darkening |
| Short-term model timeout | ~1800 s | Expiry of learned user correction |

### 1.2 Modes intentionally out of scope

Android-specific concerns that this port does **not** include (seams left for
future work): doze/idle/bedtime brightness modes, foreground-app context
correction, HBM / brightness-range controller, and display white balance.

---

## 2. The algorithm (implementation spec)

### 2.1 Input smoothing

For each sensor event at time `t` with lux `L`:

1. Push `(t, L)` into a ring buffer, pruning entries older than `horizon_long`.
2. Compute `ambient_lux(now, horizon)` over samples in `[now - horizon, now]`,
   treating the "now" edge as `now + 100 ms` (the prediction offset):
   - `weight = weight_integral(end_delta) - weight_integral(start_delta)`
   - `weight_integral(x) = x * (x * 0.5 + intercept)`, `intercept = horizon_long`
   - clamp the oldest in-window sample's start to `horizon_start_time`
   - return `sum(lux * weight) / sum(weight)`
3. Maintain both a **slow** (long horizon) and a **fast** (short horizon)
   estimate.

### 2.2 Threshold state machine

1. `set_ambient_lux(lux)` computes:
   - ambient brightening threshold = `max(lux * (1 + c_b), lux + min_brightening)`
   - ambient darkening threshold = `max(min(lux * (1 - c_d), lux - min_darkening), 0)`
   - where `c_b`/`c_d` come from `HysteresisLevels` lookup tables.
2. On each update, the ambient value transitions only when:
   - brightening: `slow >= brightening_threshold && fast >= brightening_threshold`
     **and** the brightening dwell has elapsed; or
   - darkening: `slow <= darkening_threshold && fast <= darkening_threshold`
     **and** the darkening dwell has elapsed.
3. On transition, set the accepted lux to the **fast** ambient lux, update the
   thresholds, and recompute brightness.
4. Schedule the next update at `max(next_transition, now + normal_sensor_rate)`.

### 2.3 lux → brightness

1. `Spline::create_spline(lux_levels, brightness_levels)` — monotone cubic
   (Fritsch–Carlson) when `y` is monotone, otherwise linear. Clamp `x` to the
   spline domain; pass `NaN` through.
2. Apply the auto-brightness adjustment as a gamma: `brightness^i` where
   `i = max_gamma^(-adjustment)` and `adjustment ∈ [-1, 1]`.
3. If a user data point exists, insert it as a control point and smooth the
   surrounding curve:
   - `permissible_ratio(lux, prev) = ((lux + 0.25) / (prev + 0.25))^1.0`
   - a curve step may increase by at least `MIN_PERMISSABLE_INCREASE = 0.004`
4. Apply screen-brightness hysteresis: accept the new normalized value only if
   it is outside `[screen_darkening_threshold, screen_brightening_threshold]`,
   unless the change was manually initiated.
5. Clamp to the configured `[min, max]` brightness range.

### 2.4 User learning (`add_user_data_point`)

Infer the adjustment from `desired = current^gamma`:

```
gamma      = log(desired) / log(current)      # for current, desired in (0.1, 0.9)
adjustment = -log(gamma) / log(max_gamma)
```

Edge cases (ported verbatim):
- `current <= 0.1 || current >= 0.9` → `adjustment = desired - current`
- `desired == 0` → `adjustment = -1`
- `desired == 1` → `adjustment = +1`

Clamp to `[-1, 1]`, then store the user point, recompute the spline, and smooth
the curve around the new control point.

Reset rules (short-term model):
- when ambient lux leaves the `±SHORT_TERM_MODEL_THRESHOLD_RATIO (0.6)` band
  around the anchor lux; or
- after `short_term_model_timeout` following the transition out of an
  interactive display policy.

### 2.5 Output conversion

Android converts normalized brightness → nits → device backlight. On Linux:

- Default (`SimpleMappingStrategy`): the spline's Y axis *is* the normalized
  backlight fraction; scale to the device range.
- Optional (`PhysicalMappingStrategy`, only with a device profile providing
  `nits[]` and `brightness[]`): replicate the nits↔brightness splines.
- Apply a `linear → perceptual` conversion only when the panel's sysfs is known
  to be linear (per-profile flag).

---

## 3. Android → Linux concept mapping

| Android | Linux equivalent |
|---|---|
| `SensorManager` `TYPE_LIGHT` | IIO sysfs `in_illuminance0_input` (primary); iio-sensor-proxy D-Bus (later) |
| `SensorEventListener` rate | Timer-driven sysfs polling at controller-selected rate |
| `DisplayPowerController` brightness | `org.freedesktop.login1.Session.SetBrightness` (preferred); `/sys/class/backlight/*/brightness` fallback |
| `Settings.System.SCREEN_BRIGHTNESS` | Own TOML config + D-Bus property |
| `config_autoBrightness*` resources | Built-in defaults + per-device TOML profile |
| `BrightnessConfiguration` | Runtime config object |
| `TaskStackListener` (foreground app) | Out of scope |
| `DisplayWhiteBalanceController` | Out of scope |

---

## 4. Architecture

```
                 ┌──────────────────────────────────────────────┐
                 │                abrightd (tokio)              │
 IIO sysfs ─────▶│  AlsSource ──▶ AutomaticBrightnessController │
 (poll timer)    │                    │                         │
                 │                    ├─ RingBuffer (slow/fast)  │
 D-Bus ─────────▶│  (control/status)  ├─ HysteresisLevels       │
                 │                    ├─ BrightnessMappingStrategy
                 │                    │    └─ Spline             │
                 │                    └─ ShortTermModel (learning)
                 │                    │                         │
                 │                    ▼                         │
                 │              Ramp / limiter                  │
                 │                    │                         │
                 │              BacklightSink ──▶ logind / sysfs│
                 └──────────────────────────────────────────────┘
```

Two boundaries keep the core testable: `AlsSource` (input) and
`BacklightSink` (output) are traits, and the controller library is pure, with an
injected `Clock` and no I/O.

### 4.1 Repository layout

```
autobrightnessd/
  Cargo.toml
  README.md
  docs/
    SPECIFICATIONS.md  # this document
    ARCHITECTURE.md    # maintainer-facing implementation notes
    rolling/           # temporary plans (git-excluded)
  examples/
    abrightd.toml      # annotated profile
    lenovo.toml        # reference machine profile
  systemd/abrightd.service
  udev/90-abrightd-backlight.rules
  src/
    main.rs            # CLI + daemon lifecycle
    lib.rs
    clock.rs           # Clock trait (sensor-scale + monotonic)
    controller.rs      # AutomaticBrightnessController port
    ring_buffer.rs     # AmbientLightRingBuffer + weighting
    hysteresis.rs      # HysteresisLevels
    spline.rs          # monotone cubic + linear spline
    mapping.rs         # BrightnessMappingStrategy (Simple + Physical)
    short_term.rs      # ShortTermModel / user data points
    ramp.rs            # output rate limiting
    config.rs          # TOML profiles
    state.rs           # persisted calibration state
    status.rs          # shared state + command queue
    dbus.rs            # org.abrightd control/status service
    tui.rs             # live indicator
    logging / etc.
    als/
      mod.rs           # AlsSource trait
      iio.rs           # sysfs discovery + polling
      replay.rs        # CSV replay for tests
    output/
      mod.rs           # BacklightSink trait
      logind.rs        # D-Bus
      sysfs.rs         # /sys/class/backlight
  tests/
    golden.rs          # vectors ported from AOSP Java tests
    replay.rs          # synthetic traces
    properties.rs      # property tests
```

### 4.2 Crates & stack

| Concern | Choice |
|---|---|
| Async runtime | `tokio` (timers, signals, multi-source select) |
| D-Bus | `zbus` — logind `SetBrightness`, own service, later iio-sensor-proxy |
| Config | `serde` + `toml` |
| CLI | `clap` (derive) |
| Errors | `thiserror` (lib) + `anyhow` (bin) |
| Logging | `tracing` + `tracing-subscriber`; log lux, slow/fast lux, thresholds, chosen value at `debug` |
| Time | `rustix`/`libc`: `CLOCK_BOOTTIME` for sensor scale, `CLOCK_MONOTONIC` for handler timers (mirrors AOSP's two clocks) |
| Testing | `proptest` + ported golden vectors |
| Safety | No `unsafe` except a small attested sysfs shim |

---

## 5. Module specifications

### 5.1 `spline.rs`

```rust
pub trait Spline {
    fn interpolate(&self, x: f32) -> f32;
}
pub fn create_spline(x: &[f32], y: &[f32]) -> Box<dyn Spline>;
pub struct LinearSpline { /* ... */ }
pub struct MonotoneCubicSpline { /* ... */ }
```

Reproduce AOSP boundary handling exactly: `x <= x[0]` returns `y[0]`,
`x >= x[n-1]` returns `y[n-1]`, `NaN` returns `NaN`.

### 5.2 `ring_buffer.rs`

```rust
pub struct AmbientLightRingBuffer { /* VecDeque<(u64, f32)> */ }
impl AmbientLightRingBuffer {
    pub fn push(&mut self, time_ms: u64, lux: f32);
    pub fn prune(&mut self, before_ms: u64);
    pub fn calculate_ambient_lux(&self, now_ms: u64, horizon_ms: u64) -> Option<f32>;
}
fn weight_integral(x: f32, intercept: f32) -> f32 { x * (x * 0.5 + intercept) }
```

### 5.3 `hysteresis.rs`

```rust
pub struct HysteresisLevels { /* tables + minima */ }
impl HysteresisLevels {
    pub fn brightening_threshold(&self, value: f32) -> f32;
    pub fn darkening_threshold(&self, value: f32) -> f32;
}
```

Ship AOSP defaults; allow TOML override. Keep ambient (lux units) and screen
(normalized [0,1]) variants.

### 5.4 `mapping.rs`

```rust
pub trait BrightnessMappingStrategy {
    fn brightness(&self, lux: f32) -> f32;              // [0,1]
    fn add_user_data_point(&mut self, lux: f32, brightness: f32);
    fn clear_user_data_points(&mut self);
    fn has_user_data_points(&self) -> bool;
    fn auto_brightness_adjustment(&self) -> f32;
    fn set_auto_brightness_adjustment(&mut self, a: f32) -> bool;
    fn convert_to_nits(&self, brightness: f32) -> Option<f32>;
    fn brightness_from_nits(&self, nits: f32) -> Option<f32>;
}
pub struct SimpleMappingStrategy { /* lux, brightness, max_gamma, spline, user point */ }
pub struct PhysicalMappingStrategy { /* lux->nits, nits<->brightness splines */ }
```

### 5.5 `short_term.rs`

```rust
pub struct ShortTermModel { /* anchor lux, user brightness, enabled, timeout */ }
impl ShortTermModel {
    pub fn set_user_brightness(&mut self, lux: f32, brightness: f32);
    pub fn maybe_reset(&mut self, ambient_lux: f32);
    pub fn reset(&mut self);
}
```

### 5.6 `controller.rs`

Injected `Clock` trait so tests can drive virtual time. Fields mirror Android:
`ambient_lux`, `slow_ambient_lux`, `fast_ambient_lux`, `ambient_lux_valid`,
`ambient_{brightening,darkening}_threshold`, `screen_{brightening,darkening}_threshold`,
`screen_auto_brightness`, `raw_screen_auto_brightness`, `recent_light_samples`,
`pre_threshold_*`, `brightness_adjustment_sample_pending`.

Methods:
`configure`, `set_light_sensor_enabled`, `handle_light_sensor_event`,
`apply_light_sensor_measurement`, `update_ambient_lux`,
`update_auto_brightness(send_update, is_manually_set)`, `set_ambient_lux`,
`prepare_brightness_adjustment_sample`, `collect_brightness_adjustment_sample`.

**Addition over AOSP:** synthesize periodic samples when the ALS only emits on
change, so the slow/fast horizons behave correctly on such sensors.

### 5.7 Input: `als/`

```rust
#[async_trait::async_trait]
pub trait AlsSource {
    async fn next(&mut self) -> anyhow::Result<Sample>; // { time_ms, lux }
}
```

- `IioSysfs`: discover `/sys/bus/iio/devices/iio:device*` exposing
  `in_illuminance0_input`; read `in_illuminance_scale`/`_offset` if present;
  poll at controller-selected rate; synthesize periodic samples on unchanged
  reads.
- `Replay`: CSV `timestamp_ms,lux` for tests and Phase-0 golden traces.
- Later: `ExternalSocket`, `IioProxy`.

Blocking sysfs reads must not run on the async reactor — use `spawn_blocking` or
a dedicated thread.

### 5.8 Output: `output/`

```rust
#[async_trait::async_trait]
pub trait BacklightSink {
    async fn set(&self, fraction: f32) -> anyhow::Result<()>;
    async fn max_raw(&self) -> anyhow::Result<u32>;
}
```

- `Logind`: `org.freedesktop.login1.Session.SetBrightness("backlight", name, raw)`
  — unprivileged; preferred.
- `Sysfs`: `/sys/class/backlight/*/brightness`; udev rule grants access.
- `Ramp`: separate brighten/darken per-step caps and rate limits, because
  Android's ramp lives in `DisplayPowerController` and is not part of the
  algorithm being ported.

### 5.9 `config.rs`, `daemon.rs`, `dbus.rs`

Example profile:

```toml
[als]
kind = "iio"                 # or "replay"
device = "iio:device0"

[output]
kind = "logind"              # or "sysfs"
device = "intel_backlight"
gamma = "linear"             # "perceptual" if sysfs is already gamma-corrected

[curve]
max_gamma = 3.0
lux = [0, 10, 50, 100, 500, 1000, 5000, 10000, 100000]
bri = [0.03, 0.06, 0.12, 0.2, 0.35, 0.5, 0.7, 0.85, 1.0]

[timing]
horizon_long_ms = 10000
horizon_short_ms = 2000
sensor_rate_ms = 200
initial_sensor_rate_ms = 1000
warmup_ms = 0
brightening_debounce_ms = 2000
darkening_debounce_ms = 4000

[learning]
enabled = true
short_term_timeout_ms = 1800000
```

Curve seeds from AOSP's `config_autoBrightnessLuxLevels` /
`config_autoBrightnessLevels` defaults, then calibrated on hardware.

Own D-Bus service `org.abrightd`:
`Enable`, `SetProfile`, `AddUserPoint(lux, brightness)`, `ClearUserPoints`,
`Status` (lux, slow/fast lux, thresholds, chosen value).

`systemd --user` unit with hardening; readiness via D-Bus.

---

## 6. Data flow (one iteration)

1. `AlsSource` yields `(t_boottime, lux)`.
2. Controller pushes → prunes → computes short-horizon `ambient_lux` for the
   first valid sample (after warm-up) → `update_auto_brightness`.
3. Each update: compute slow/fast estimates → check transitions → on transition
   `set_ambient_lux(fast)`.
4. Mapping: `spline(lux)` → gamma/user adjustment → clamp → screen hysteresis →
   chosen value.
5. `Ramp` → `BacklightSink`. Full chain written to `Status`/logs.

---

## 7. Hardware bring-up

### 7.1 Sensor

- `ls /sys/bus/iio/devices`; confirm `in_illuminance0_input`.
- Check `in_illuminance_scale` / `in_illuminance_offset`.
- Observe update frequency; detect report-on-change-only behavior.
- Note saturation range (many ALS max out at a few thousand lux).

### 7.2 Backlight

- `ls /sys/class/backlight`; confirm `max_brightness`.
- Test logind `SetBrightness` within a session.
- Determine the gamma characteristic (by measurement or careful observation).

### 7.3 Calibration

- Sweep distinct light levels with a lux meter.
- Record `(lux, fraction)` pairs across at least: dark room, office, outdoors.
- Fit control points; check for oscillation and overshoot at each level.

---

## 8. Testing strategy

- **Golden vectors** — port AOSP `AutomaticBrightnessControllerTest` and
  `BrightnessMappingStrategyTest` expectations into `tests/golden/` against the
  pure library. This is the strongest correctness guarantee.
- **Replay harness** — `abrightd --replay trace.csv --dump-brightness`;
  deterministic, CI-friendly, no hardware.
- **Property tests** (`proptest`):
  - spline monotonicity and continuity;
  - `darkening_threshold(v) ≤ v ≤ brightening_threshold(v)`;
  - constant-lux input never oscillates;
  - `add_user_data_point` is reproduced exactly on the next query.
- **Synthetic traces** — step changes, ramps, flicker, sensor-on-change-only,
  duplicate/out-of-order timestamps, `NaN`/invalid lux.
- **Hardware-in-loop** — run against the real ALS, log the full state chain,
  compare chosen brightness to a lux meter; 24 h soak for leaks and CPU.

---

## 9. Milestones

| Phase | Content | Accept when | Est. |
|---|---|---|---|
| 0 | Repo scaffold, `Clock` trait, replay harness | `--replay` prints a trace | 0.5 d |
| 1 | `spline`, `hysteresis`, `ring_buffer`, `SimpleMappingStrategy`, `ShortTermModel` | golden + property tests green | 3–4 d |
| 2 | `AutomaticBrightnessController` state machine | golden tests green; replay matches AOSP traces | 3–4 d |
| 3 | IIO `AlsSource` | reads real lux, rate-switches, handles change-only sensors | 2–3 d |
| 4 | logind + sysfs `BacklightSink` + ramp | screen tracks a step change without oscillation | 2–3 d |
| 5 | config, D-Bus, systemd unit, conflict handling | `systemctl --user` start/stop, status over D-Bus | 3–4 d |
| 6 | calibration tool + default profiles | works out-of-box after one calibration run | 2 d |
| 7 | docs, packaging, soak test | release-ready | 2 d |

Roughly **3–4 focused weeks**; Phases 1–2 carry the technical risk.

---

## 10. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Most Linux panels expose no nits table | Default to `SimpleMappingStrategy`; `PhysicalMappingStrategy` only with a supplying profile |
| Uncalibrated / raw-scale sensors | Read `scale`/`offset`; per-device lux multiplier in config |
| Backlight contention (GNOME/KDE auto-brightness, `wluma`, `clight`, `ddcutil`) | Detect active controllers; require explicit `--takeover`; document conflict rules |
| Android ramps outside this algorithm | Implement our own ramp with conservative, configurable rates |
| Sensors that report only on change | Periodic sample synthesis in the IIO source |
| Blocking sysfs reads | `spawn_blocking` / dedicated thread |
| Licensing | AOSP display code is Apache-2.0; retain headers/notices on ported files |

---

## 11. Definition of done

- Installs and runs as a `systemd --user` service.
- Reads a real IIO ALS and drives backlight via logind.
- Shows no oscillation under constant or slowly changing light.
- Learns from manual overrides and expires them via the short-term model.
- Covered by ported golden vectors plus property and replay tests.
- Documents calibration, desktop conflicts, and the exact AOSP classes ported.
