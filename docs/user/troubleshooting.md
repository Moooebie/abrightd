# Troubleshooting

Useful commands:

```sh
systemctl --user status abrightd
journalctl --user -u abrightd -f
abrightd calibrate show            # curve + live snapshot
abrightd integrate detect          # desktop / conflict check
busctl --user call org.abrightd /org/abrightd org.abrightd Status
```

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
  `abrightd` probes all three names and applies `lux = raw * scale + offset`,
  then the optional `[als] lux_multiplier`.
* It only emits new values **on change**; the source synthesises a sample every
  poll interval so the slow/fast horizons and debounce timers behave correctly.
* `intel_backlight` is root-only; logind `Session.SetBrightness` works for the
  active session without root, so prefer `[output] kind = "logind"` (or `"kde"`).

## Common problems

**The sensor reading does not change.**
`Status` exposes two values: `last_observed_lux` (the latest raw sample) and
`lux` (the *accepted*, post-hysteresis value).  `last_observed_lux` frozen means
the sensor/driver (this HID ALS reports `in_illuminance_hysteresis_relative =
0.01` and has a low-light floor); `last_observed_lux` moving while `lux` lags is
normal hysteresis + debounce.  The TUI shows both.

**Wrong sensor, or several sensors.**
Run `abrightd init`: it lists every IIO device with an illuminance channel (and
a live reading) and lets you pick one.

**The brightness is systematically too high or too low.**
Calibrate it: `abrightd calibrate adjust --value ±X`, or `--point <lux> <bri>`
(see [Calibration](calibration.md)).  On KDE, drag the applet's adjustment
slider.

**The backlight does not change.**
Check the sink in the log (`backlight sink: …`), and compare
`Status.output_brightness` with `cat /sys/class/backlight/*/brightness`.  The
ramp moves in small steps, so give it a moment.  `--dry-run` never writes.

**`org.abrightd` is missing / D-Bus methods fail.**
The build lacks `--features dbus` (or `tui`).  Rebuild with `./build.sh`.
`calibrate adjust` then falls back to writing `state.toml` for the next start.

**Another auto-brightness tool is fighting.**
Run `abrightd integrate detect`.  If your desktop's own ALS auto-brightness is
enabled, disable it.  Desktop brightness *keys* are fine — abrightd learns from
them.

**Plasma brightness UI shows the wrong value.**
Use `[output] kind = "kde"` (see
[desktop integration](desktop-integration.md)).

**The applet says "abrightd is not running".**
The daemon isn't reachable on the session bus: check
`systemctl --user status abrightd`, and that the binary was built with `dbus`.

**Installing over a running daemon fails with `Text file busy`.**
`systemctl --user stop abrightd` first (the install script does this).

**The terminal is garbled after the TUI.**
It restores on normal exit; if it was killed with `SIGKILL`, run `reset` or
`stty sane`.
