# abrightd — implementation notes

Maintainer/agent-facing companion to [`../README.md`](../README.md).  The README
explains *what* abrightd does and how to use it; this document explains *how it
is built*, where the sharp edges are, and the conventions to preserve when
changing it.

If you are an automated agent picking this repo up: read §1–§4 and §17 first,
then the module you are touching, then §16 (testing) before submitting.

---

## 1. Mental model

```
        ┌──────────────────────────── abrightd process ───────────────────────────┐
        │                                                                         │
 IIO ───┼─▶ AlsSource ──▶ AutomaticBrightnessController ──▶ Ramp ──▶ BacklightSink ┼──▶ logind/sysfs
 sysfs  │   (iio/replay)   │  ring buffer (slow/fast lux)                          │
        │                  │  ambient hysteresis                                   │
        │                  │  screen hysteresis                                    │
        │                  │  BrightnessMappingStrategy (Simple)                   │
        │                  └─ ShortTermModel (learning)                            │
        │                                                                         │
        │   SharedState (Arc<Mutex>) ◀──▶ org.abrightd D-Bus service               │
        │   TUI ── reads Status over D-Bus                                        │
        │   `calibrate` CLI ── writes state.toml and/or calls SetAdjustment        │
        └─────────────────────────────────────────────────────────────────────────┘
```

Key invariant: **the controller and everything it calls is pure and does no
I/O.**  It takes `now_ms` explicitly and returns a `ControllerOutput`.  All
boundaries are traits (`AlsSource`, `BacklightSink`, `Clock`).  Do not add
`std::fs`, `zbus`, `tokio` or `SystemTime` calls inside `controller.rs`,
`mapping.rs`, `hysteresis.rs`, `ring_buffer.rs`, `spline.rs`, `short_term.rs`.

Two independent state stores exist:

| Store | Owner | Contents |
|---|---|---|
| config TOML | user | static profile (curve, timing, hysteresis, sink) |
| `state.toml` | daemon + `calibrate` CLI | mutable calibration (`adjustment`; later: learned knots) |

---

## 2. Module map

| File | Responsibility | AOSP origin |
|---|---|---|
| `src/lib.rs` | module wiring, `brightness_to_raw`/`raw_to_brightness` | — |
| `src/spline.rs` | monotone cubic + linear interpolation | `android.util.Spline` |
| `src/hysteresis.rs` | `HysteresisLevels` | `HysteresisLevels` |
| `src/ring_buffer.rs` | `AmbientLightRingBuffer` + weighted lux integral | nested class in `AutomaticBrightnessController` |
| `src/mapping.rs` | `BrightnessMappingStrategy` + `SimpleMappingStrategy`, `infer_auto_brightness_adjustment` | `BrightnessMappingStrategy` |
| `src/short_term.rs` | `ShortTermModel`, `should_reset_short_term_model` | nested class in `AutomaticBrightnessController` |
| `src/controller.rs` | `AutomaticBrightnessController`, `ControllerConfig`, `ControllerOutput` | `AutomaticBrightnessController` |
| `src/clock.rs` | `Clock` trait, `SystemClock`, `TestClock` | `SystemClock` |
| `src/config.rs` | TOML profile schema + defaults | `config_autoBrightness*` resources |
| `src/state.rs` | persisted calibration state | — (our addition) |
| `src/status.rs` | `SharedState`, `Command` queue | — (our addition) |
| `src/dbus.rs` | `org.abrightd` service + client helpers | — (our addition) |
| `src/ramp.rs` | output rate limiting | `DisplayPowerController` ramp (replaced) |
| `src/als/*` | `AlsSource`, IIO sysfs, CSV replay | `SensorManager`/`SensorEventListener` |
| `src/output/*` | `BacklightSink`, sysfs, logind | `DisplayPowerController` → backlight |
| `src/daemon.rs` | event loop, replay harness | `Handler` loop |
| `src/tui.rs` | live indicator | — (our addition) |
| `src/main.rs` | CLI, `calibrate` subcommands | — |

---

## 3. AOSP fidelity: exact vs. deliberate deviation

**Ported verbatim** (see the Apache-2.0 headers in each file):

- Fritsch–Carlson tangent limiting and boundary handling (`x<=x[0]`, `x>=x[n-1]`,
  `NaN` passthrough).
- `AMBIENT_LIGHT_PREDICTION_TIME_MILLIS = 100`.
- `weight_integral(x) = x·(x·0.5 + intercept)`, `intercept = horizon_long`.
- Ring-buffer `prune` timestamp rewriting for report-on-change sensors.
- `LUX_GRAD_SMOOTHING = 0.25`, `MAX_GRAD = 1.0`,
  `MIN_PERMISSABLE_INCREASE = 0.004` (sic).
- `SHORT_TERM_MODEL_THRESHOLD_RATIO = 0.6`.
- `infer_auto_brightness_adjustment` edge cases: `current ≤ 0.1 || ≥ 0.9` →
  linear delta; `desired == 0` → `-1`; `desired == 1` → `+1`; else
  `-ln(gamma)/ln(max_gamma)`.
- `BRIGHTNESS_ADJUSTMENT_SAMPLE_DEBOUNCE_MILLIS = 10_000`.
- Slow/fast transition rule and debounce scheduling in `update_ambient_lux`.

**Deliberate deviations / additions** (do not "fix" these to match AOSP):

1. **`Box<dyn Spline>` instead of an abstract class.**  `create_spline` chooses
   monotone cubic when `y` is monotone, else linear, exactly as AOSP.
2. **`VecDeque` ring buffer.**  AOSP's doubling ring is behaviourally identical.
3. **No `Handler`.**  The controller exposes `next_wakeup_ms()` / `on_timer()`
   instead of posting messages.  See §4.
4. **Ramp is ours.**  AOSP's ramp lives in `DisplayPowerController` and is not
   part of the algorithm; `ramp.rs` is a conservative stand-in.
5. **Persistence + D-Bus + TUI + calibration** are additions (§11–§13).
6. **Out of scope:** doze/idle/bedtime modes, foreground-app correction, HBM,
   display white balance, `PhysicalMappingStrategy` (nits profiles).
7. **Hysteresis defaults.**  AOSP's fallback (no device config) is levels
   `[0]`, 10% brightening / 20% darkening, and **minimum `0f`** (not the
   `10`/`30` lux shown in `DisplayDeviceConfig` doc comments).  See §6 — this
   exact mistake once made low-light behaviour look broken.
8. **`refresh_after_user_change`.**  AOSP passes `isManuallySet` to bypass
   screen hysteresis for user-initiated changes; the D-Bus command path uses it.

Reference checkouts used: `android-14.0.0_r1` and `master` of
`platform/frameworks/base` (see §19).

---

## 4. The controller (the core)

`AutomaticBrightnessController` is a pure state machine.  Fields mirror AOSP:

- ambient: `ambient_lux`, `slow_ambient_lux`, `fast_ambient_lux`,
  `ambient_lux_valid`, `ambient_{brightening,darkening}_threshold`,
  `pre_threshold_lux`, `ring`.
- brightness: `screen_auto_brightness`, `raw_screen_auto_brightness`,
  `screen_{brightening,darkening}_threshold`, `pre_threshold_brightness`.
- lifecycle: `light_sensor_enabled`, `light_sensor_enable_time`,
  `current_light_sensor_rate`.
- observations: `recent_light_samples`, `last_observed_lux(_time)`.
- learning: `short_term`.
- user sampling: `brightness_adjustment_sample_*`.
- timers: `ambient_update_at`, `brightness_adjustment_sample_at`,
  `invalidate_short_term_at`.

### Time

Everything takes `now_ms: i64`.  `clock::Clock` exposes `uptime_ms`
(`CLOCK_MONOTONIC`) and `elapsed_realtime_ms` (`CLOCK_BOOTTIME`).  **The daemon
runs the whole pipeline on `CLOCK_BOOTTIME`** so ring-buffer horizons and
debounce timers stay consistent across suspend.  Tests use `TestClock`.

### Scheduling (replacing `Handler`)

- `next_wakeup_ms()` = `min(ambient_update_at, brightness_adjustment_sample_at,
  invalidate_short_term_at)`.
- `on_timer(now)` fires whichever deadlines are `<= now`, then returns an output
  with the new `next_wakeup_ms`.
- `ControllerOutput { brightness: Option<f32>, next_wakeup_ms: Option<i64> }`.
  `brightness` is `Some` exactly when AOSP would call `mCallbacks.updateBrightness()`.

### Feeding samples

```
handle_light_sensor_event(now, lux):
  if !light_sensor_enabled -> no-op
  ambient_update_at = None                      # removeMessages(MSG_UPDATE_AMBIENT_LUX)
  if ring.is_empty() -> current_rate = normal   # first sample switches rate
  apply_light_sensor_measurement(now, lux)      # recent++, prune(long), push, last_observed
  update_ambient_lux(now)
```

### `update_ambient_lux(now)`

1. If `!ambient_lux_valid`: wait for `warmup + enable_time`; then
   `set_ambient_lux(calculate_ambient_lux(now, horizon_short))`, mark valid,
   `update_auto_brightness(true, false)`.
2. Compute `next_brighten`/`next_darken` dwell deadlines from the ring
   (newest→oldest, break on first sample not past the threshold, add the
   configured debounce).
3. `slow = calc(now, horizon_long)`, `fast = calc(now, horizon_short)`.
4. Transition iff **both** slow and fast cross the *same* threshold **and** the
   dwell has elapsed:
   `(slow>=bright && fast>=bright && next_brighten<=now) || (slow<=dark && fast<=dark && next_darken<=now)`.
   On transition: `pre_threshold_lux = ambient_lux`, `set_ambient_lux(fast)`,
   `update_auto_brightness(true,false)`, recompute dwell deadlines.
5. Schedule at `min(next_brighten,next_darken)`; if that is `<= now`, use
   `now + normal_light_sensor_rate`.

### `update_auto_brightness(send_update, is_manually_set)`

`value = mapper.get_brightness(ambient_lux)` → `raw_screen_auto_brightness`;
`new = clamp(value)`.  Skip if the new value is inside
`(screen_darkening_threshold, screen_brightening_threshold)` **and** not
manual and the current value is still within range.  Otherwise record
`pre_threshold_brightness`, set `screen_auto_brightness`, recompute screen
thresholds, and if `send_update` queue/return the value.  `clamp` uses the
output `[min,max]`.

### Invariants

- Thresholds always bracket `ambient_lux` / `screen_auto_brightness` for
  positive values (property-tested).
- `ambient_lux` is never negative (`set_ambient_lux` clamps).
- A constant lux never produces repeated updates (property + replay tested).
- `now` is monotonic non-decreasing for a given clock.

### Unused-but-ready hooks

`prepare_brightness_adjustment_sample` / `collect_brightness_adjustment_sample`
and `short_term.invalidate()` + `schedule_short_term_invalidation` are ported
but not yet driven by real user events.  Wiring them (detect external backlight
writes, sample after 10 s) is Tier 4 — see §18.

---

## 5. Ring buffer & weighting

- AOSP `prune(horizon)` drops samples strictly older than `horizon`, but keeps
  the youngest out-of-horizon sample and rewrites its timestamp to `horizon`.
  This is what makes report-on-change sensors usable; do not "simplify" it.
- `calculate_ambient_lux(now, horizon)`:
  1. `horizon_start = now - horizon`; find the first sample just outside it.
  2. Walk newest→oldest accumulating weight
     `weight_integral(end) - weight_integral(start)` with
     `weight_integral(x) = x·(x·0.5 + horizon_long)`.
  3. The oldest in-window sample is clamped to `horizon_start`.
  4. Returns `Σ(lux·weight)/Σ(weight)`, or `None` when empty/all-zero.
  The "now" edge starts at `now + 100 ms` (prediction), guaranteeing non-zero
  weight for the newest sample.

The prediction offset is why a completely static sensor still yields a finite
estimate.

---

## 6. Hysteresis (and the low-light gotcha)

`HysteresisLevels` stores percentage *fractions* (`0.10 == 10%`) and threshold
levels.  `reference_level(value)` returns 0 below `levels[0]`, else the
percentage of the highest level `<= value`.

```
brightening_threshold(v) = max(v·(1 + pct), v + min_brightening)
darkening_threshold(v)   = max(min(v·(1 - pct), v - min_darkening), 0)
```

**AOSP fallback defaults (what we ship):** `levels = [0.0]`, brightening
`0.10`, darkening `0.20`, `min_* = 0.0`.

> History: an earlier revision shipped `min_brightening = 10`, `min_darkening
> = 30` (read from `DisplayDeviceConfig` *doc comments*).  At low lux that gave
> a dead band of `[0, lux+10]`, so the accepted lux appeared frozen in dim
> light.  AOSP's `HysteresisLevels.createHysteresisLevels` uses `0f` when no
> device minimum is supplied.  Keep minima `0` unless a profile sets them
> deliberately.

TOML mapping (`config.rs`): each of `ambient_brightening`, `ambient_darkening`,
`screen_brightening`, `screen_darkening` fills `percentages`, `levels`, `min`.
Note `HysteresisPairConfig` uses `#[serde(default)]`, so a partial override
replaces the whole pair with per-field defaults — always specify `percentages`
and `levels` together.

---

## 7. Mapping (lux → brightness)

`SimpleMappingStrategy` holds the raw `lux`/`brightness` control points plus a
compiled spline, `max_gamma`, `auto_brightness_adjustment`, and a single user
point.

- `get_brightness(lux) = spline.interpolate(lux)` (unclamped; the controller
  clamps).
- `adjusted_curve()` applies `b' = b^gamma`, `gamma = max_gamma^(−adjustment)`,
  then inserts the user point (if any) and smooths around it with
  `permissible_ratio(curr, prev) = ((curr+0.25)/(prev+0.25))^1.0`, guaranteeing
  at least `0.004` increase per step upward.
- `add_user_data_point` infers the adjustment from the **unadjusted** base curve
  and stores the point; the spline passes through it exactly.
- `clear_user_data_points` resets adjustment to 0 and rebuilds.

Reference point: `infer_auto_brightness_adjustment` is the function the
`calibrate adjust --point` CLI uses.

**Extension seam:** multi-point calibration (Tier 3) and the learned correction
(Tier 4) should be added here as an additional correction applied after the
gamma/user step, before `create_spline`.  Keep it monotone and clamped.

---

## 8. Short-term model

`ShortTermModel { anchor, brightness, valid }`.  `NO_USER_LUX = NO_USER_BRIGHTNESS
= -1`.  `should_reset_short_term_model(lux, anchor, min_ratio, max_ratio)` returns
true when `lux ∉ (anchor·(1−min_ratio), anchor·(1+max_ratio)]` (default 0.6).

In the controller, `set_ambient_lux` calls the equivalent of AOSP's
`mShortTermModel.maybeReset`: if invalid but anchored and the lux left the band,
clear the mapper's user points and reset; otherwise re-validate.

Currently the STM is in-memory only and holds at most one point (AOSP
behaviour).  Persisting it and supporting many points is Tier 4.

---

## 9. I/O adapters

**`AlsSource`** (`async fn next() -> Result<Option<Sample>>`; `None` = EOF):

- `IioSysfs::discover(device, poll_rate_ms, clock)`: probes
  `in_illuminance0_input`, `in_illuminance_input`, `in_illuminance_raw`;
  reads `in_illuminance_scale`/`_offset`; `with_lux_multiplier(m)` multiplies.
  Blocking reads run in `spawn_blocking`.  A sample is emitted every poll even
  if the raw value is unchanged (report-on-change synthesis).
- `Replay`: CSV `timestamp_ms,lux`, tolerates `#` comments and a header.

**`BacklightSink`** (`set(fraction)`, `max_raw()`):

- `SysfsBacklight` writes `round(fraction·max)` to
  `/sys/class/backlight/*/brightness`.
- `LogindBacklight` (feature `dbus`) resolves the session via
  `Manager.GetSession("auto")` and calls `Session.SetBrightness("backlight",…,
  raw)`.  It derives name/max from sysfs.
- `NullSink` (`--dry-run`).

**`Ramp`**: snap on first update, otherwise move by at most `brighten_step` /
`darken_step` per tick, no more often than `min_interval_ms`.

---

## 10. Daemon loop

`Daemon::run(als)`:

1. `enable sensor` at `clock.elapsed_realtime_ms()`.
2. Loop: `apply_commands()`; compute the earliest of
   `controller.next_wakeup_ms()` and `ramp_deadline`; `select!` over
   `ctrl_c`, `als.next()`, and a `sleep` until that deadline.
3. On a sample: `controller.handle_light_sensor_event(t, lux)` then `advance`.
4. On a timer: `controller.on_timer(now)` then `advance`.
5. `advance(now, out)`: latch `target`; ramp toward it; write the sink when the
   ramped value changes by more than float-epsilon; set `ramp_deadline` while
   `ramp.current != target`; `update_shared()`.

`replay(controller, ramp, samples)` is the deterministic harness: it merges
samples with `next_wakeup_ms()` events (sample wins ties), stops after the last
sample plus a 120 s settle window, and returns `ReplayPoint`s.  Use it instead of
wall-clock sleeps whenever you need reproducibility.

---

## 11. D-Bus & shared state

- `SharedState` (in `status.rs`) is the snapshot the daemon publishes and the
  TUI reads; `Command` is the queue the service pushes and the daemon drains.
- `Status` (`a{ss}`) keys: `enabled`, `profile`, `lux`, `last_observed_lux`,
  `slow_lux`, `fast_lux`, `ambient_brightening_threshold`,
  `ambient_darkening_threshold`, `screen_brightening_threshold`,
  `screen_darkening_threshold`, `controller_brightness`, `output_brightness`,
  `adjustment`, `user_points` (`"lux:brightness;…"`).
- Service `org.abrightd` at `/org/abrightd` on the **session** bus.  Methods:
  `Enable(bool)`, `SetProfile(string)`, `AddUserPoint(d, d)`,
  `ClearUserPoints()`, `SetAdjustment(d)`, `Status()`.
- `apply_commands()` drains the queue, then **immediately** calls
  `refresh_after_user_change` for any command (AOSP's `isManuallySet`), and
  persists `state.toml` when `SetAdjustment` was applied.
- `serve()` returns the `Connection`; the caller **must hold it** for the
  daemon's lifetime or the bus name is released (this was a real bug).
- `fetch_status()` / `set_adjustment()` are the client helpers used by
  `calibrate`.

`lux` is the *accepted* (post-hysteresis) ambient value; `last_observed_lux` is
the most recent raw sample.  Keep both when diagnosing.

---

## 12. Calibration & persistence

`PersistedState { schema_version, adjustment }` in `state.toml`.

- `state_dir()` prefers `$STATE_DIRECTORY` (systemd `StateDirectory=`), then
  `$XDG_STATE_HOME/abrightd`, then `$HOME/.local/state/abrightd`.
- `save()` writes a temp file then `rename`s (atomic).
- `load()` returns defaults on missing/malformed input (forward-compatible).
- The daemon applies `adjustment` **before** constructing the controller, both
  in `run_daemon` and `run_replay`.
- `abrightd calibrate show` prints the base vs. adjusted curve, sensor info and
  live `Status`.  `calibrate adjust --value <a>` sets it directly;
  `--point <lux> <brightness>` infers against the base curve.  Both prefer the
  live D-Bus path (which persists) and fall back to writing `state.toml`.
- Bump `SCHEMA_VERSION` for incompatible changes; add fields with
  `#[serde(default)]`.

---

## 13. TUI

`ratatui` + `crossterm`, feature `tui` (implies `dbus`).  It polls `Status`
every `--interval-ms`, draws into a fixed buffer (clipped to the terminal, so it
can never wrap/scroll), and restores the terminal on `q`/`Esc`/`Ctrl-C`.

Layout: title, **sensor** (latest raw sample), **ambient** (accepted + slow/fast
+ trend), a fixed-width `brightness` bar with the percentage at its right edge,
`target`, `adjust`, `state`, help.  The bar is `█` fill over a fixed width;
values below 10 are shown with 3 decimals so small low-light changes are
visible.

---

## 14. Configuration reference

See `examples/abrightd.toml` for the annotated version.  Defaults:

| Section | Field | Default | Notes |
|---|---|---|---|
| `[als]` | `kind` | `"iio"` | `"iio"` or `"replay"` |
| | `device` | auto | name or absolute path |
| | `poll_rate_ms` | `200` | |
| | `lux_multiplier` | `1.0` | per-device calibration |
| `[output]` | `kind` | `"logind"` | `"logind"` (feature `dbus`) or `"sysfs"` |
| | `min`/`max` | `0.0`/`1.0` | clamps the curve |
| `[ramp]` | steps / interval | `0.05`/`0.05`/`100 ms` | |
| `[curve]` | `max_gamma` | `3.0` | |
| | `lux`/`bri` | 9-point seed | `bri` normalized [0,1] |
| `[timing]` | horizons | `10000`/`2000 ms` | |
| | rates | `200`/`1000 ms` | normal/initial |
| | debounce | `2000`/`4000 ms` | brighten/darken |
| | warmup | `0 ms` | |
| `[hysteresis.*]` | pct/levels/min | `0.10`/`0.20`, `[0]`, `0` | see §6 |
| `[learning]` | `short_term_timeout_ms` | `1_800_000` | |
| | `short_term_threshold_ratio` | `0.6` | (currently not read by controller) |

Units: all times ms, lux in lux, brightness normalized `[0,1]`, percentages
fractions.

---

## 15. Build, features, packaging

```sh
cargo build                      # daemon + sysfs
cargo build --features dbus      # + logind + org.abrightd
cargo build --features tui       # + indicator (implies dbus)
cargo test --all-features
cargo clippy --all-features --all-targets
```

- `main.rs` always needs `clap`; only `dbus`/`tui` are optional.  There is no
  `cli` feature (an earlier one was removed because `main.rs` used clap
  unconditionally).
- `systemd/abrightd.service` (`systemd --user`) and
  `udev/90-abrightd-backlight.rules` are shipped.  The unit sets
  `StateDirectory=abrightd`, which systemd may expose as a compatibility
  symlink to `~/.config/abrightd`; both CLI and daemon resolve the same path.
- Install for this machine: copy `target/release/abrightd` to
  `~/.local/bin/abrightd` **after stopping the service** (otherwise `ETXTBSY`).

---

## 16. Testing

| Kind | Location | Covers |
|---|---|---|
| unit | `#[cfg(test)]` in each module | spline, hysteresis, ring buffer, mapping, ramp, state, config, IIO/sysfs with temp dirs |
| golden | `tests/golden.rs` | hand-computed AOSP vectors (weighted integral, infer adjustment, control-point interpolation) |
| property | `tests/properties.rs` (`proptest`) | monotone/continuous spline, hysteresis bracketing, user point exactness, no oscillation |
| replay | `tests/replay.rs` + `tests/replay_trace.csv` | determinism, brighten/darken steps, no oscillation in constant light, output range |
| config | `tests/example_config.rs` | the shipped example parses and builds a controller |

Rules when changing the core:

- Never weaken a golden test to make a change pass; if behaviour must change,
  update the vector with a comment explaining the AOSP justification.
- Use `TestClock` and `daemon::replay` for anything time-dependent.
- Run `cargo test --all-features` and `cargo clippy --all-features
  --all-targets` (must be warning-free) before finishing.
- After changing the daemon, rebuild with `--features tui`, reinstall, and
  restart the service if you intend to run it here.

---

## 17. Debugging playbook

```sh
systemctl --user status abrightd
journalctl --user -u abrightd -f
~/.local/bin/abrightd calibrate show          # curve + live snapshot
busctl --user call org.abrightd /org/abrightd org.abrightd Status
```

- **"Reading doesn't change"** → compare `last_observed_lux` (raw) with `lux`
  (accepted) in `Status`/TUI.  Raw frozen = sensor/driver (this HID ALS reports
  `in_illuminance_hysteresis_relative = 0.01` and has a low-light floor).
  Raw moves but accepted lags = hysteresis + debounce (by design); check the
  `min`/`percentages` in `[hysteresis.*]` (§6).
- **"Wrong lux"** → `in_illuminance_raw · in_illuminance_scale + offset`, then
  `lux_multiplier`.  The kernel's value is authoritative as far as we know.
- **Backlight unchanged** → confirm the sink (`Status.output_brightness` vs
  `cat /sys/class/backlight/*/brightness`), check logind session resolution,
  and remember the ramp needs several ticks; `--dry-run` logs only.
- **D-Bus method missing** → the binary was built without `--features dbus`;
  rebuild.  `calibrate adjust` then silently falls back to `state.toml`.
- **Terminal garbage after TUI** → it should restore on exit; if killed with
  `SIGKILL` it cannot — run `reset`/`stty sane`.
- **`ETXTBSY` on install** → stop the service first.

---

## 18. Extension points / roadmap

- **Tier 1 — lux calibration.**  Fit `lux = a·raw + b` (or knots) against a
  reference meter; store under `[als.calibration]`/`state.toml`; apply in
  `IioSysfs`.
- **Tier 3 — guided multi-point calibration.**  `ratatui` wizard recording
  `(lux, brightness)` anchors; fit a monotone correction in `SimpleMappingStrategy`
  (there is room for a correction step in `adjusted_curve`).
- **Tier 4 — continuous learning.**  Wire `prepare/collect_brightness_adjustment_sample`
  to detected external backlight writes (PowerDevil/brightness keys), persist the
  short-term model, and add a monotone long-term correction (isotonic + forgetting).
  Extend `PersistedState` and add D-Bus commands; keep the `Command`/`SharedState`
  pattern.
- **`PhysicalMappingStrategy`.**  Optional nits profile: implement
  `convert_to_nits` and the nits↔backlight splines in `mapping.rs`.
- When extending `PersistedState`, add `#[serde(default)]` fields and bump
  `SCHEMA_VERSION` only for breaking changes.

---

## 19. AOSP sources

Fetched from `https://android.googlesource.com/platform/frameworks/base`:

- `services/core/java/com/android/server/display/AutomaticBrightnessController.java`
- `services/core/java/com/android/server/display/BrightnessMappingStrategy.java`
- `services/core/java/com/android/server/display/HysteresisLevels.java`
  (and `…/display/config/HysteresisLevels.java` for the defaults/minima logic)
- `services/core/java/com/android/server/display/DisplayDeviceConfig.java`
- `core/java/android/util/Spline.java`
- `core/res/res/values/config.xml`

Apache-2.0; retain the notices on ported files.

---

## 20. Conventions for agents

1. Keep the core pure; inject time; never do I/O in the controller or its
   dependencies.
2. Preserve AOSP semantics unless there is a documented reason; note deviations
   in §3 and in code comments with the original class name.
3. Take `now_ms` (or `Clock`) as a parameter for anything time-based; never call
   the system clock inside logic.
4. New behaviour needs a test — unit for math, golden for AOSP parity, replay
   for time-dependent logic, property for invariants.
5. `cargo fmt`, `cargo clippy --all-features --all-targets` (0 warnings),
   `cargo test --all-features` must all pass.
6. Update the README for user-visible changes and this document for structural
   ones.
