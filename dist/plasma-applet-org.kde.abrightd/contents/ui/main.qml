import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami
import org.kde.abrightd 1.0

PlasmoidItem {
    id: root

    Controller {
        id: backend
    }

    // Middle-click the panel icon to toggle.  Left-click is not accepted here,
    // so it still opens the popup (handled by Plasma).
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onClicked: backend.enabled = !backend.enabled
    }

    Plasmoid.title: "Automatic brightness"
    Plasmoid.icon: backend.enabled ? "brightness-high" : "brightness-low"
    toolTipMainText: "Automatic brightness"
    toolTipSubText: backend.available
        ? (backend.enabled ? "On" : "Off") +
          "  ·  " + backend.lux.toFixed(1) + " lx" +
          "  ·  " + (backend.brightness * 100).toFixed(0) + "%"
        : "abrightd is not running"

    fullRepresentation: ColumnLayout {
        implicitWidth: Kirigami.Units.gridUnit * 14
        implicitHeight: Kirigami.Units.gridUnit * 6
        spacing: Kirigami.Units.smallSpacing

        PlasmaComponents.Label {
            Layout.alignment: Qt.AlignHCenter
            text: "Automatic brightness"
            font.bold: true
        }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: Kirigami.Units.smallSpacing

            PlasmaComponents.Switch {
                id: toggle
                enabled: backend.available
                checked: backend.enabled
                onToggled: backend.enabled = checked
            }

            PlasmaComponents.Label {
                text: backend.enabled ? "On" : "Off"
            }
        }

        PlasmaComponents.Label {
            Layout.alignment: Qt.AlignHCenter
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: backend.available
                ? backend.lux.toFixed(1) + " lx  ·  " + (backend.brightness * 100).toFixed(0) + "%"
                : "abrightd is not running"
        }
    }
}
