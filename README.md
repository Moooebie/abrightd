# abrightd

> 简体中文文档：[README.zh_CN.md](README.zh_CN.md)

A standalone Rust userspace daemon that brings **Android's automatic
brightness** to GNU/Linux.  It reads an ambient-light sensor (IIO), runs the
real AOSP brightness pipeline, and drives the backlight through `logind`
D-Bus, KDE PowerDevil, or `sysfs`.

It is a faithful port of the AOSP display classes, not an approximation:

| AOSP component | Here |
|---|---|
| `AutomaticBrightnessController` | `src/controller.rs` |
| `AmbientLightRingBuffer` + weighted lux | `src/ring_buffer.rs` |
| `HysteresisLevels` | `src/hysteresis.rs` |
| `android.util.Spline` | `src/spline.rs` |
| `BrightnessMappingStrategy.SimpleMappingStrategy` | `src/mapping.rs` |
| `ShortTermModel` / user learning | `src/short_term.rs` |

**Highlights**

- Weighted slow/fast ambient smoothing, ambient and screen hysteresis, and
  debounce — exactly as on Android.
- Learns from your brightness keys/slider (short-term model) and remembers the
  result across reboots.
- Global adjustment (the AOSP "brightness slider" gamma) plus a reset.
- KDE Plasma integration: PowerDevil output keeps the Plasma UI/OSD in sync,
  and a small **Auto Brightness** panel widget.
- A live terminal indicator, a D-Bus control surface, and a deterministic
  replay harness (no hardware needed).
- Pure, I/O-free core with unit/property/golden/replay tests.

## Quick start

```sh
git clone https://github.com/Moooebie/abrightd.git && cd abrightd
./build.sh            # one command: release build
./install.sh          # one command: install + start the daemon

./install-desktop.sh  # separate command: desktop components (KDE applet)
```

`make build`, `make install` and `make install-desktop` are equivalent.

Then check it:

```sh
systemctl --user status abrightd
abrightd --tui
```

Requirements and details: [docs/user/installation.md](docs/user/installation.md).

## Documentation

The docs are split by audience:

### For users — [`docs/user/`](docs/user/)

| Document | Contents |
|---|---|
| [installation.md](docs/user/installation.md) | requirements, build/install, desktop components, uninstall |
| [usage.md](docs/user/usage.md) | running, the TUI, configuration, CLI and D-Bus reference |
| [calibration.md](docs/user/calibration.md) | tuning the curve, persistence, reset |
| [desktop-integration.md](docs/user/desktop-integration.md) | KDE Plasma, the applet, conflict handling |
| [troubleshooting.md](docs/user/troubleshooting.md) | hardware bring-up and common problems |

### For maintainers / AI agents — [`docs/agent/`](docs/agent/)

| Document | Contents |
|---|---|
| [SPECIFICATIONS.md](docs/agent/SPECIFICATIONS.md) | the full specification this port implements |
| [ARCHITECTURE.md](docs/agent/ARCHITECTURE.md) | core invariants, module map, AOSP fidelity, debugging playbook |

`docs/agent/rolling/` holds temporary, git-excluded working plans.

## Status

The full pipeline is implemented and running on the reference machine (Lenovo,
Intel HID ALS, `intel_backlight`, KDE Plasma 6): IIO input, the AOSP controller,
`logind`/`kde`/`sysfs` outputs, calibration (`show`/`adjust`/`reset`), persisted
learning, D-Bus control, the TUI, and the Plasma applet.  Still planned: sensor
lux calibration, a guided multi-point wizard, a longer-term learner, and GNOME
integration.  See [SPECIFICATIONS.md](docs/agent/SPECIFICATIONS.md) for the
roadmap.

## License

Apache-2.0.  Portions are derived from the Android Open Source Project
(`frameworks/base`); the original notices are retained.
