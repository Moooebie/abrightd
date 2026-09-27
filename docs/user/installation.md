# Installation

`abrightd` is a per-user daemon: it runs in your session, reads an ambient-light
sensor, and drives the backlight.  No root is needed at runtime (it talks to
`logind`/PowerDevil), apart from one optional udev rule if you use the raw
`sysfs` backend.

## Requirements

- Linux with an IIO ambient-light sensor (`/sys/bus/iio/devices`).
- A backlight under `/sys/class/backlight`.
- A systemd user session (for the service) and `logind`; on KDE, Plasma 6.
- Rust (stable) to build.  The desktop applet additionally needs CMake, Ninja,
  Extra CMake Modules and the Qt 6 / KDE Frameworks development packages.

## Quick install

From the repository root:

```sh
./build.sh          # one command: builds the release binary
./install.sh        # one command: installs and starts the daemon

./install-desktop.sh   # separate command: desktop components (KDE applet)
```

`make` targets do the same: `make build`, `make install`, `make install-desktop`.

### What `install.sh` does

1. Installs `abrightd` to `~/.local/bin/abrightd`.
2. Installs the systemd **user** unit to
   `~/.config/systemd/user/abrightd.service`.
3. Writes a default profile to `~/.config/abrightd/config.toml` **only if one
   does not already exist**.
4. Runs `systemctl --user daemon-reload` and `enable --now abrightd`.
5. Prints the (optional) udev rule command for the `sysfs` backend.

Check it:

```sh
systemctl --user status abrightd
journalctl --user -u abrightd -f
```

### What `install-desktop.sh` does

Desktop-specific pieces are kept out of the daemon install so headless/minimal
setups stay clean:

- **KDE Plasma**: builds and installs the `org.kde.abrightd` QML bridge and the
  *Auto Brightness* widget (see
  [desktop-integration.md](desktop-integration.md)).  The QML plugin goes into
  Qt's system import path, so this step uses `sudo`.

## Manual install (without the scripts)

```sh
cargo build --release --features tui
install -Dm755 target/release/abrightd ~/.local/bin/abrightd
install -Dm644 systemd/abrightd.service ~/.config/systemd/user/abrightd.service
mkdir -p ~/.config/abrightd
cp examples/abrightd.toml ~/.config/abrightd/config.toml   # if you have none yet
systemctl --user daemon-reload
systemctl --user enable --now abrightd
```

### The udev rule (only for `[output] kind = "sysfs"`)

The default outputs (`logind`, `kde`) need no privileges.  If you use `sysfs`,
grant the session write access:

```sh
sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight
```

## Build features

| Feature | Adds | Default? |
|---|---|---|
| *(none)* | daemon + `sysfs` output | |
| `dbus` | `logind` output, `org.abrightd` D-Bus service, `integrate`, calibration commands | |
| `tui` | the live brightness indicator (implies `dbus`) | `build.sh` uses this |

```sh
cargo build --release                 # minimal
cargo build --release --features dbus  # logind + D-Bus
cargo build --release --features tui   # + indicator
cargo test --all-features
```

## Uninstall

```sh
./uninstall.sh                              # daemon (service, binary, unit)
./install-desktop.sh --uninstall            # desktop components
```

`~/.config/abrightd/` and `~/.local/state/abrightd/` (your profile and
calibration) are left in place; remove them by hand if you want a clean slate.
