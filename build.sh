#!/bin/sh
# One-command build.  Reads the variant chosen by ./configure.
#
#   ./configure [kde]     # choose what to build for (default: daemon only)
#   ./build.sh            # build (FEATURES overridable)
set -e

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

desktop=none
if [ -f .abrightd.conf ]; then
    . ./.abrightd.conf
    desktop="${DESKTOP:-none}"
else
    echo "note: not configured — run ./configure first (building daemon only)."
fi

features="${FEATURES:-tui}"

echo "== building abrightd (release, features: $features, desktop: $desktop) =="
cargo build --release --features "$features"

echo
echo "binary: $here/target/release/abrightd"
echo "next:   ./install.sh"
