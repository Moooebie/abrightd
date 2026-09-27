#!/bin/sh
# One-command install of the abrightd daemon for the current user.
#
# Installs the binary, the systemd user unit and (if absent) a default profile,
# then enables and starts the service.  It does not need root; only the optional
# udev rule for the raw `sysfs` output backend does (printed at the end).
#
# Desktop components are a separate step: ./install-desktop.sh
set -e

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

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

echo "== installing systemd user unit -> $UNIT_DIR/abrightd.service =="
mkdir -p "$UNIT_DIR"
sed -e "s|%h/.local/bin/abrightd|$BINDIR/abrightd|" \
    -e "s|%h/.config/abrightd/config.toml|$CONFIG_DIR/config.toml|" \
    systemd/abrightd.service > "$UNIT_DIR/abrightd.service"

echo "== profile =="
mkdir -p "$CONFIG_DIR"
if [ -f "$CONFIG_DIR/config.toml" ]; then
    echo "  keeping existing $CONFIG_DIR/config.toml"
else
    install -m 0644 examples/abrightd.toml "$CONFIG_DIR/config.toml"
    echo "  wrote $CONFIG_DIR/config.toml"
fi

echo "== (re)loading the user service =="
systemctl --user daemon-reload
systemctl --user enable --now abrightd

echo
echo "done."
echo "  status:  systemctl --user status abrightd"
echo "  logs:    journalctl --user -u abrightd -f"
echo
echo "Raw sysfs output backend only — grant backlight access:"
echo "  sudo install -Dm644 udev/90-abrightd-backlight.rules /etc/udev/rules.d/"
echo "  sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=backlight"
echo
echo "Desktop components: ./install-desktop.sh"
