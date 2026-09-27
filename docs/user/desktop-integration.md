# Desktop integration

`abrightd` keeps the daemon and your desktop from fighting over the backlight.

- [KDE Plasma](#kde-plasma)
- [The Plasma applet](#plasma-applet)
- [GNOME](#gnome)

## KDE Plasma

Use the PowerDevil output so the Plasma brightness UI and on-screen display stay
in sync, and abrightd can learn from the brightness keys:

```toml
[output]
kind = "kde"          # drives PowerDevil's BrightnessControl

[integration]
watch_user_changes = true   # treat brightness keys/slider as user intent
pause_when_locked = true    # stop adjusting while the session is locked
pause_on_suspend = true
```

Then:

```sh
abrightd integrate detect
```

```
abrightd desktop integration
  desktop        KDE Plasma
  output kind    kde (configured)
  watch user     true
  pause          locked=true suspend=true
  powerdevil     1425 / 10000 (14.2%)
  sysfs panel    2190 / 15360 (14.3%)
  DE ALS auto    not supported in this Plasma build (no conflict)
```

`integrate detect` reports the desktop, the PowerDevil brightness (and whether
it disagrees with the real panel — i.e. whether `kind = "kde"` is worth using),
and whether the desktop has its own ambient-light auto-brightness that would
conflict.

When you press the brightness keys or move the Plasma slider, PowerDevil emits
`brightnessChanged`; abrightd treats that as a user override, records it against
the current lux and adopts it instead of overwriting it.  Automatic
adjustments are written *silently* so they do not spam the OSD.

> `kind = "logind"` still works everywhere and needs fewer moving parts, but the
> Plasma brightness UI will show a stale value.  `kind = "kde"` is the right
> choice on Plasma.

## Plasma applet

Install the widget (separate from the daemon install):

```sh
./install-desktop.sh
```

Then add **Auto Brightness** via *right-click panel → Add Widgets*.  The panel
icon and its popup provide:

- an **on/off switch** — the panel icon reflects the state, and **middle-click**
  the icon toggles it;
- a **global adjustment slider** (`-1.00 … +1.00`, step `0.01`) with the current
  value shown — the same global gamma as `calibrate adjust`;
- a **point-calibration** indicator and a **Reset calibration** button (enabled
  when there is something to reset);
- live lux / brightness.

The slider and reset button are **disabled while abrightd is switched off** or
when the daemon is not reachable.

If the widget reports a missing QML module, reload Plasma once:

```sh
systemctl --user restart plasma-plasmashell.service
```

Remove the widget with `./install-desktop.sh --uninstall`.

## GNOME

Not implemented yet.  The core is desktop-agnostic: on GNOME, keep the default
`logind` output and use the CLI/TUI.  The user-override and status plumbing is
already modelled in `src/desktop` and can be reused for `gsd-power` and a Shell
extension later.
