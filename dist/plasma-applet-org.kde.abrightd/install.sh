#!/bin/sh
# Build and install the abrightd Plasma applet and its QML bridge.
#
# The QML plugin must live in Qt's QML import path (system-owned), so the
# install step uses sudo.  The plasmoid itself is installed for the current
# user.
set -e

here=$(cd "$(dirname "$0")" && pwd)

echo "== building QML plugin =="
cmake -S "$here/plugin" -B "$here/plugin/build" -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build "$here/plugin/build"

echo "== installing QML plugin (sudo) =="
sudo cmake --install "$here/plugin/build"

echo "== installing plasmoid =="
if kpackagetool6 --type Plasma/Applet --list 2>/dev/null | grep -q 'org.kde.abrightd'; then
    kpackagetool6 --type Plasma/Applet --upgrade "$here"
else
    kpackagetool6 --type Plasma/Applet --install "$here"
fi

echo
echo "Done. Add 'Auto Brightness' via: right-click panel > Add Widgets."
echo "If the widget reports a missing QML module, restart Plasma:"
echo "  systemctl --user restart plasma-plasmashell.service"
