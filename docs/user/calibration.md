# Calibration

The shipped curve is a generic AOSP seed, so it usually needs adjusting for your
panel.  All of this is optional and reversible.

## See the current curve

```sh
abrightd calibrate show
```

```
abrightd calibration
  config      /home/you/.config/abrightd/config.toml
  state       /home/you/.local/state/abrightd/state.toml
  adjustment  +0.302   (max_gamma 3.0)
  sensor      iio:/sys/bus/iio/devices/iio:device0 (scale=0.001, offset=0)
  user point  lux 40.00, brightness 0.2000

         lux        base    adjusted
         0.0      0.0300      0.0808
        10.0      0.0600      0.1329
       100.0      0.2000      0.3152
      1000.0      0.5000      0.6082
     10000.0      0.8500      0.8899
```

`base` is the raw profile curve; `adjusted` is what the daemon actually uses
(global adjustment + any learned point).

## Global adjustment

AOSP's whole-curve gamma, `b' = b^(max_gamma^−adjustment)`, in `[-1, +1]`.
Positive values raise the whole curve:

```sh
abrightd calibrate adjust --value 0.3
```

Or infer it from a brightness you like at a given light level:

```sh
abrightd calibrate adjust --point 40 0.20      # at 40 lx, prefer 20% brightness
```

`--point` infers the adjustment against the *unadjusted* base curve using AOSP's
`inferAutoBrightnessAdjustment`, so the curve then passes through your chosen
brightness at that lux.

On KDE you can also drag the adjustment slider in the
[Plasma applet](desktop-integration.md#plasma-applet), or use
`SetAdjustment` over D-Bus.

## Learning from brightness keys

While the daemon runs, changing the brightness with the desktop (keys / Plasma
slider / GNOME) is treated as *user intent*: abrightd records it against the
current lux (AOSP short-term learning) and adopts it instead of overwriting it.

## Persistence

The **net effect** of calibration — the global adjustment *and* any learned
user point(s) — is saved to
`$XDG_STATE_HOME/abrightd/state.toml` (`~/.local/state/abrightd/state.toml` by
default) and restored on the next start, so your tuned curve survives a reboot.
Writes are debounced (~0.5 s).

AOSP semantics apply: adding a point recomputes and **replaces** the global
adjustment, so the saved pair reproduces exactly the curve you were looking at.

## Reset

```sh
# Back to the uncalibrated curve (adjustment 0, no learned point)
abrightd calibrate reset            # prompts; add --yes; --backup keeps a copy

# Restore the profile's calibration sections to the shipped defaults
abrightd profile reset --backup     # keeps [als], [output], [integration]
```

`calibrate reset` clears the saved calibration and applies live through
`org.abrightd` when the daemon is running.  `profile reset` rewrites
`[curve]`, `[hysteresis]`, `[timing]`, `[ramp]` and `[learning]` to defaults,
preserves your sensor/backlight/desktop sections, and backs the old file up to
`config.toml.bak`; restart the daemon to apply it.

There is also a **Reset calibration** button in the Plasma applet.

## Planned tiers

Sensor (lux) calibration, a guided multi-point wizard, and a longer-term learner
that fits a correction curve from a history of overrides.
