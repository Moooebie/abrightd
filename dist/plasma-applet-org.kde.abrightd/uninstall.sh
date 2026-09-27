#!/bin/sh
# Remove the abrightd Plasma applet and its QML bridge.
set -e

here=$(cd "$(dirname "$0")" && pwd)
qml_dir=$(qmake6 -query QT_INSTALL_QML)

echo "== removing plasmoid =="
kpackagetool6 --type Plasma/Applet --remove org.kde.abrightd 2>/dev/null || true

echo "== removing QML plugin (sudo) =="
sudo rm -rf "$qml_dir/org/kde/abrightd"

echo "== removing translations (sudo) =="
for po in "$here"/po/*.po; do
    [ -f "$po" ] || continue
    lang=$(basename "$po" .po)
    sudo rm -f "/usr/share/locale/$lang/LC_MESSAGES/plasma_applet_org.kde.abrightd.mo"
done

echo "Done."
