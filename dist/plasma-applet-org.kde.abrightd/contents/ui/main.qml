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
        Layout.minimumWidth: Kirigami.Units.gridUnit * 17
        Layout.preferredWidth: Kirigami.Units.gridUnit * 17
        spacing: Kirigami.Units.smallSpacing

        // --- on / off ---
        RowLayout {
            Layout.fillWidth: true
            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: "Automatic brightness"
                font.bold: true
            }
            PlasmaComponents.Switch {
                enabled: backend.available
                checked: backend.enabled
                onToggled: backend.enabled = checked
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            visible: backend.available
        }

        // --- global adjustment (disabled when off) ---
        ColumnLayout {
            Layout.fillWidth: true
            visible: backend.available
            enabled: backend.enabled
            spacing: 0

            RowLayout {
                Layout.fillWidth: true
                PlasmaComponents.Label {
                    Layout.fillWidth: true
                    text: "Adjustment"
                }
                PlasmaComponents.Label {
                    text: (backend.adjustment >= 0 ? "+" : "") + backend.adjustment.toFixed(2)
                    font.family: "monospace"
                    opacity: 0.8
                }
            }

            PlasmaComponents.Slider {
                id: adjustmentSlider
                Layout.fillWidth: true
                from: -1.0
                to: 1.0
                stepSize: 0.01
                value: backend.adjustment
                onMoved: backend.adjustment = value
            }
        }

        // --- point calibration indicator + reset (disabled when off) ---
        RowLayout {
            Layout.fillWidth: true
            visible: backend.available && backend.pointCalibrated
            enabled: backend.enabled
            PlasmaComponents.Label {
                Layout.fillWidth: true
                text: "Point calibration active"
                opacity: 0.8
            }
            PlasmaComponents.Button {
                text: "Reset calibration"
                icon.name: "edit-undo"
                onClicked: backend.resetCalibration()
            }
        }

        // --- live readings ---
        PlasmaComponents.Label {
            Layout.fillWidth: true
            opacity: 0.7
            font: Kirigami.Theme.smallFont
            text: backend.available
                ? backend.lux.toFixed(1) + " lx  ·  " + (backend.brightness * 100).toFixed(0) + "%"
                : "abrightd is not running"
        }
    }

    // Keep the slider in sync when the adjustment changes externally (poll) or
    // after a reset, but never fight the user mid-drag.
    Connections {
        target: backend
        function onAdjustmentChanged() {
            if (!adjustmentSlider.pressed) {
                adjustmentSlider.value = backend.adjustment;
            }
        }
    }
}
