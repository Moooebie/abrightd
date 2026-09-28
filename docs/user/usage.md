# Usage

Once [installed](installation.md), `abrightd` normally runs as a systemd user
service.  Everything below can also be run by hand.

## Running

```sh
systemctl --user status abrightd      # is it running?
systemctl --user restart abrightd     # apply a profile change
journalctl --user -u abrightd -f      # follow the log
journalctl --user -u abrightd --log-level  # (set RUST_LOG for more detail)
```

Foreground, useful while experimenting:

```sh
abrightd --config ~/.config/abrightd/config.toml --log-level debug
abrightd --dry-run                    # read the sensor, never touch the backlight
abrightd --replay tests/replay_trace.csv --dump-brightness   # no hardware needed
```

The profile is resolved as: `--config <path>` → `~/.config/abrightd/config.toml`
→ built-in defaults.

## The indicator (TUI)

Built with `--features tui` (which `build.sh` uses), the TUI shows the live
pipeline in a terminal while the daemon runs:

```sh
abrightd --tui
abrightd --tui --interval-ms 100
```

```
abrightd  ·  AOSP auto-brightness
sensor   16.31 lx   (latest sample)
ambient  16.31 lx   slow 16.4  fast 16.3   ● steady
brightness
  ████                   20.0%
target 20.0%
adjust   ● steady
state    enabled=true  profile=default
q / Esc / Ctrl-C to quit
```

It reads the daemon's `Status` over D-Bus, so it shows the ambient lux (raw
sample and the slow/fast estimates), the commanded backlight with the ramp
target marked, the light trend and the current adjustment direction
(`▲ brightening` / `▼ darkening` / `● steady`).  Quit with `q`, `Esc` or
`Ctrl-C`.

## Configuration

`~/.config/abrightd/config.toml`; see
[`examples/abrightd.toml`](../../examples/abrightd.toml) for a fully commented
profile.  Sections:

| Section | Controls |
|---|---|
| `[als]` | sensor kind (`iio`/`replay`), device, poll rate, `lux_multiplier` |
| `[output]` | `kind` (`logind` / `sysfs` / `kde`), device, `min`/`max` brightness |
| `[ramp]` | how fast the backlight is allowed to move |
| `[curve]` | `max_gamma` and the lux → brightness control points |
| `[timing]` | horizons, sensor rates, debounce, warm-up |
| `[hysteresis]` | how much the lux / brightness must change to re-act |
| `[learning]` | short-term model timeout |
| `[integration]` | desktop behaviour: watch user changes, pause on lock/suspend |

The curve ships as a generic seed and is meant to be [calibrated](calibration.md)
for your panel.  After editing, `systemctl --user restart abrightd`.

## Command-line reference

```
abrightd [--config PATH] [--log-level LEVEL]            # run the daemon
abrightd --version                                      # print the version
abrightd --tui [--interval-ms MS]                       # live indicator
abrightd --replay CSV [--dump-brightness]               # deterministic replay
abrightd --dry-run                                      # sensor only, no output
abrightd init [--device NAME] [--yes] [--dry-run]        # detect sensor, write config

abrightd calibrate show                                 # curve + live snapshot
abrightd calibrate adjust --value A | --point LUX BRI   # set / infer adjustment
abrightd calibrate reset [--yes] [--backup]             # uncalibrated defaults
abrightd profile reset [--yes] [--backup]               # reset profile sections

abrightd integrate detect                               # desktop conflict check
```

## Control over D-Bus

While the daemon runs it owns `org.abrightd` on the session bus
(`/org/abrightd`):

```sh
busctl --user call org.abrightd /org/abrightd org.abrightd Status
busctl --user call org.abrightd /org/abrightd org.abrightd Enable b false
busctl --user call org.abrightd /org/abrightd org.abrightd SetAdjustment d 0.3
busctl --user call org.abrightd /org/abrightd org.abrightd AddUserPoint dd 40.0 0.2
busctl --user call org.abrightd /org/abrightd org.abrightd ResetCalibration
```

`Status` returns a `a{ss}` map with `enabled`, `lux`, `last_observed_lux`,
`slow_lux`, `fast_lux`, the ambient/screen thresholds, `controller_brightness`,
`output_brightness`, `adjustment` and `user_points`.

## Next

- [Calibration](calibration.md) — tune the curve for your panel.
- [Desktop integration](desktop-integration.md) — KDE/Plasma and the applet.
- [Troubleshooting](troubleshooting.md).
