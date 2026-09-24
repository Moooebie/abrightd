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

    // Panel representation: a clickable icon that toggles auto-brightness.
    compactRepresentation: Item {
        implicitWidth: Kirigami.Units.iconSizes.small
        implicitHeight: Kirigami.Units.iconSizes.small

        PlasmaComponents.ToolButton {
            anchors.fill: parent
            icon.name: backend.enabled ? "brightness-high" : "brightness-low"
            enabled: backend.available
            onClicked: backend.enabled = !backend.enabled
            PlasmaComponents.ToolTip.visible: hovered
            PlasmaComponents.ToolTip.text: backend.enabled
                ? "Automatic brightness: On"
                : "Automatic brightness: Off"
        }
    }

    // Popup representation: the on/off switch and live readings.
    fullRepresentation: ColumnLayout {
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
