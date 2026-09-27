#!/bin/sh
# Install desktop-specific components (currently the KDE Plasma applet).
#
#   ./install-desktop.sh              # install / upgrade
#   ./install-desktop.sh --uninstall  # remove
#
# The applet's QML bridge goes into Qt's system import path, so installing it
# uses sudo.  Nothing here is needed for the daemon itself.
set -e

here=$(cd "$(dirname "$0")" && pwd)

if [ "$1" = "--uninstall" ] || [ "$1" = "uninstall" ]; then
    exec "$here/dist/plasma-applet-org.kde.abrightd/uninstall.sh"
fi

case "${XDG_CURRENT_DESKTOP:-} ${DESKTOP_SESSION:-}" in
    *KDE*|*kde*|*Plasma*|*plasma*) ;;
    *) echo "note: KDE Plasma not detected; installing the applet anyway." ;;
esac

exec "$here/dist/plasma-applet-org.kde.abrightd/install.sh"
