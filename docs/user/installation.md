# Installation

`abrightd` is a per-user daemon: it runs in your session, reads an ambient-light
sensor, and drives the backlight.  No root is needed at runtime (it talks to
`logind`/PowerDevil); only the optional udev rule for the raw `sysfs` backend,
and the KDE applet's QML plugin, use `sudo`.

Setup is four commands, and you never have to write a config by hand:

```sh
./configure     # 1. choose the variant (daemon only, or + KDE applet)
./build.sh      # 2. build
./install.sh    # 3. install + start (and the applet, if configured)
abrightd init   # 4. detect the sensor, pick one, write the config
```

## Requirements

- Linux with an IIO ambient-light sensor (`/sys/bus/iio/devices`).
- A backlight under `/sys/class/backlight`.
- A systemd user session (for the service) and `logind`; on KDE, Plasma 6.
- Rust (stable) to build.  The KDE applet additionally needs CMake, Ninja,
  Extra CMake Modules and the Qt 6 / KDE Frameworks development packages.

## 1. Configure the variant

```sh
./configure          # daemon only
./configure kde      # daemon + KDE Plasma applet
```

This just records your choice in `.abrightd.conf` (git-ignored); it doesn't
build anything.  GNOME is not supported yet.

## 2. Build

```sh
./build.sh
```

Runs `cargo build --release --features tui`.  Override the cargo features with
`FEATURES=dbus ./build.sh` (see [Build features](#build-features)).

## 3. Install

```sh
./install.sh
```

- installs the binary to `~/.local/bin/abrightd`;
- writes a starting profile to `~/.config/abrightd/config.toml` if none exists
  (auto-detecting the sensor and desktop output — `abrightd init` refines it);
- installs the systemd user unit and `systemctl --user enable --now abrightd`;
- if you configured `kde`, installs the *Auto Brightness* applet too.

Then `abrightd init` picks the sensor (see below), so the daemon is ready
without editing anything.

### The udev rule (only for `[output] kind = "sysfs"`)

The default outputs (`logind`, `kde`) need no privileges:

```sh
sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight
```

## 4. Initialize

```sh
abrightd init
```

```
abrightd init
  profile      /home/you/.config/abrightd/config.toml
  desktop      KDE Plasma
  sensors:
    [1] iio:device0  (/sys/bus/iio/devices/iio:device0, 17.3 lx)
  Select [1-1] (default 1):
  sensor       iio:device0  (/sys/bus/iio/devices/iio:device0)
  wrote        /home/you/.config/abrightd/config.toml
  reading      17.3 lx
  restarted    abrightd
```

It lists the sensors it finds (with a live reading), lets you choose one, writes
a complete profile with a desktop-appropriate output (PowerDevil on KDE, logind
elsewhere) and the default curve, then restarts the service.  Use
`abrightd init --device iio:device0` to skip the prompt, `--dry-run` to preview,
or `--no-restart` to not touch a running daemon.  Re-run it any time to change
sensors.

That's it — the daemon now tracks the ambient light.  Tuning the curve is
optional; see [Calibration](calibration.md).

## Desktop components

`./configure kde` + `./install.sh` installs the KDE applet as part of the normal
flow.  To (re)install or remove desktop components on their own:

```sh
./install-desktop.sh              # install / upgrade
./install-desktop.sh --uninstall  # remove
```

If the applet reports a missing QML module, reload Plasma once:

```sh
systemctl --user restart plasma-plasmashell.service
```

## Build features

| Feature | Adds | Default? |
|---|---|---|
| *(none)* | daemon + `sysfs` output | |
| `dbus` | `logind` output, `org.abrightd` D-Bus service, calibration | |
| `tui` | the live terminal indicator (implies `dbus`) | `build.sh` uses this |

## Uninstall

```sh
./uninstall.sh                      # daemon (service, binary, unit)
./install-desktop.sh --uninstall    # desktop components
```

`~/.config/abrightd/` (profile, calibration) and `~/.local/state/abrightd/` are
kept; remove them by hand for a clean slate.
