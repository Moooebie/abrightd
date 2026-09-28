#!/bin/sh
# One-command install of the abrightd daemon (for the current user), plus any
# desktop components chosen with ./configure.
#
# It installs the binary, writes a working profile (no hand-editing needed),
# installs and starts the systemd user service, and — if configured with
# `./configure kde` — the KDE Plasma applet.
#
# Only the KDE applet's QML plugin needs root; everything else is per-user.
set -e

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

desktop=none
if [ -f .abrightd.conf ]; then
    . ./.abrightd.conf
    desktop="${DESKTOP:-none}"
else
    echo "note: not configured — run ./configure first (installing daemon only)."
fi

PREFIX="${PREFIX:-$HOME/.local}"
BINDIR="$PREFIX/bin"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
CONFIG_DIR="$CONFIG_HOME/abrightd"
UNIT_DIR="$CONFIG_HOME/systemd/user"

if [ ! -x target/release/abrightd ]; then
    echo "abrightd is not built yet — running ./build.sh"
    FEATURES="${FEATURES:-tui}" "$here/build.sh"
fi

echo "== installing binary -> $BINDIR/abrightd =="
mkdir -p "$BINDIR"
install -m 0755 target/release/abrightd "$BINDIR/abrightd"

echo "== profile =="
mkdir -p "$CONFIG_DIR"
if [ -f "$CONFIG_DIR/config.toml" ]; then
    echo "  keeping existing $CONFIG_DIR/config.toml"
else
    # Detect the sensor and write a ready-to-use profile (no manual editing).
    "$BINDIR/abrightd" --config "$CONFIG_DIR/config.toml" init --yes --no-restart
fi

echo "== installing systemd user unit -> $UNIT_DIR/abrightd.service =="
mkdir -p "$UNIT_DIR"
sed -e "s|%h/.local/bin/abrightd|$BINDIR/abrightd|" \
    -e "s|%h/.config/abrightd/config.toml|$CONFIG_DIR/config.toml|" \
    systemd/abrightd.service > "$UNIT_DIR/abrightd.service"

echo "== (re)loading the user service =="
systemctl --user daemon-reload
systemctl --user enable --now abrightd

if [ "$desktop" = kde ]; then
    echo
    echo "== desktop components (KDE Plasma) =="
    "$here/dist/plasma-applet-org.kde.abrightd/install.sh"
fi

echo
echo "done."
echo "  status:  systemctl --user status abrightd"
echo "  logs:    journalctl --user -u abrightd -f"
echo "  adjust:  abrightd init            # re-pick the sensor"
echo
echo "Raw sysfs output backend only — grant backlight access:"
echo "  sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/"
echo "  sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight"
