pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Standard KDE "Configure" entry point for the settings that apply immediately:
// the shell persists the idle lock and the secure clipboard persists its timeout.
Controls.Dialog {
    id: root

    property int idleLockSeconds: 0
    property int autoClearSeconds: 0

    signal idleLockSecondsRequested(int seconds)
    signal autoClearSecondsRequested(int seconds)

    readonly property var idleOptions: [
        {
            label: qsTr("Desactivado"),
            seconds: 0
        },
        {
            label: qsTr("1 minuto"),
            seconds: 60
        },
        {
            label: qsTr("5 minutos"),
            seconds: 300
        },
        {
            label: qsTr("15 minutos"),
            seconds: 900
        }
    ]
    readonly property var clipboardOptions: [
        {
            label: qsTr("Desactivada"),
            seconds: 0
        },
        {
            label: qsTr("15 segundos"),
            seconds: 15
        },
        {
            label: qsTr("30 segundos"),
            seconds: 30
        },
        {
            label: qsTr("60 segundos"),
            seconds: 60
        }
    ]

    function selectIndex(options, seconds) {
        return Math.max(0, options.findIndex(option => option.seconds === seconds));
    }

    function syncSelections() {
        idleLock.currentIndex = selectIndex(idleOptions, idleLockSeconds);
        clipboardClear.currentIndex = selectIndex(clipboardOptions, autoClearSeconds);
    }

    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 28, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    title: qsTr("Configurar %1").arg(Application.displayName)

    onOpened: syncSelections()
    onIdleLockSecondsChanged: syncSelections()
    onAutoClearSecondsChanged: syncSelections()

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Kirigami.FormLayout {
            Layout.fillWidth: true
            Controls.ComboBox {
                id: idleLock
                Kirigami.FormData.label: qsTr("Bloquear la bóveda tras inactividad:")
                model: root.idleOptions
                textRole: "label"
                Accessible.name: qsTr("Bloquear la bóveda tras inactividad")
                onActivated: root.idleLockSecondsRequested(root.idleOptions[currentIndex].seconds)
            }
            Controls.ComboBox {
                id: clipboardClear
                Kirigami.FormData.label: qsTr("Borrar del portapapeles tras:")
                model: root.clipboardOptions
                textRole: "label"
                Accessible.name: qsTr("Borrar del portapapeles tras")
                onActivated: root.autoClearSecondsRequested(root.clipboardOptions[currentIndex].seconds)
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Los cambios se guardan al elegirlos.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }

    footer: RowLayout {
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: qsTr("Cerrar")
            onClicked: root.close()
        }
    }
}
