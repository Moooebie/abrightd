#!/bin/sh
# One-command build of the abrightd daemon.
#
#   ./build.sh                 # release build with the TUI + D-Bus features
#   FEATURES=dbus ./build.sh   # choose cargo features
set -e

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

features="${FEATURES:-tui}"

echo "== building abrightd (release, features: $features) =="
cargo build --release --features "$features"

echo
echo "binary: $here/target/release/abrightd"
echo "next:   ./install.sh    (and ./install-desktop.sh for the KDE applet)"
