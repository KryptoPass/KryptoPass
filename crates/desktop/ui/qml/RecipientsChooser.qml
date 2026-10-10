pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.GroupBox {
    id: chooser

    required property KeyStore keyStore
    property var selectedFingerprints: []
    readonly property var encryptableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.subkeys.some(subkey => subkey.statusCode === "valid" && subkey.capabilities.includes("Cifrar")))
    readonly property var unavailableFingerprints: selectedFingerprints.filter(fingerprint => !encryptableKeys.some(key => key.id === fingerprint))
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && unavailableFingerprints.length === 0

    signal selectionChanged(var fingerprints)

    padding: Kirigami.Units.smallSpacing
    title: qsTr("Acceso")

    function setChecked(fingerprint, checked) {
        const next = selectedFingerprints.filter(candidate => candidate !== fingerprint);
        if (checked)
            next.push(fingerprint);
        selectedFingerprints = next;
        selectionChanged(next);
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige las claves OpenPGP que podrán descifrar este elemento.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Repeater {
            model: chooser.encryptableKeys

            delegate: Controls.CheckBox {
                required property var modelData
                Layout.fillWidth: true
                checked: chooser.selectedFingerprints.includes(modelData.id)
                text: modelData.title + (modelData.email ? " · " + modelData.email : "") + "\n" + modelData.id
                Accessible.name: qsTr("Destinatario %1, huella %2").arg(modelData.title).arg(modelData.id)
                onToggled: chooser.setChecked(modelData.id, checked)
            }
        }

        Repeater {
            model: chooser.unavailableFingerprints

            delegate: Controls.CheckBox {
                required property string modelData
                Layout.fillWidth: true
                checked: chooser.selectedFingerprints.includes(modelData)
                text: qsTr("%1 · sin clave válida para cifrar").arg(modelData)
                Accessible.name: qsTr("Destinatario original no disponible, huella %1").arg(modelData)
                onToggled: chooser.setChecked(modelData, checked)
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0
            text: qsTr("Consultando claves OpenPGP…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0 && chooser.unavailableFingerprints.length === 0
            text: qsTr("No hay claves válidas con capacidad de cifrado. Actualiza el llavero o revisa las claves.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.unavailableFingerprints.length > 0
            text: qsTr("Se conserva cada huella original. Desmarca las claves no disponibles o elige una clave válida antes de guardar.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }
}
