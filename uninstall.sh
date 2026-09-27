#!/bin/sh
# Remove the abrightd daemon (service, unit and binary).
#
# Your profile (~/.config/abrightd) and calibration state
# (~/.local/state/abrightd) are left in place.
set -e

PREFIX="${PREFIX:-$HOME/.local}"
CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
UNIT_DIR="$CONFIG_HOME/systemd/user"

echo "== stopping and disabling the service =="
systemctl --user disable --now abrightd 2>/dev/null || true
rm -f "$UNIT_DIR/abrightd.service"
systemctl --user daemon-reload 2>/dev/null || true

echo "== removing binary =="
rm -f "$PREFIX/bin/abrightd"

echo
echo "done.  Keeping $CONFIG_HOME/abrightd (profile, calibration)."
