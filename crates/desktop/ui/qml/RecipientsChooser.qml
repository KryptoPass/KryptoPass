pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: chooser

    required property KeyStore keyStore
    property var selectedFingerprints: []
    readonly property var encryptableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.subkeys.some(subkey => subkey.statusCode === "valid" && subkey.capabilities.includes("Cifrar")))
    readonly property var unavailableFingerprints: selectedFingerprints.filter(fingerprint => !encryptableKeys.some(key => key.id === fingerprint))
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && unavailableFingerprints.length === 0

    signal selectionChanged(var fingerprints)

    spacing: 8

    function setChecked(fingerprint, checked) {
        const next = selectedFingerprints.filter(candidate => candidate !== fingerprint);
        if (checked)
            next.push(fingerprint);
        selectedFingerprints = next;
        selectionChanged(next);
    }

    Controls.Label {
        text: qsTr("Claves autorizadas")
        font.weight: Font.DemiBold
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige las claves OpenPGP que podrán descifrar este elemento.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Repeater {
            model: chooser.encryptableKeys

            delegate: ColumnLayout {
                id: keyRow
                required property var modelData
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 4
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Controls.CheckBox {
                        id: keyCheck
                        checked: chooser.selectedFingerprints.includes(keyRow.modelData.id)
                        Accessible.name: qsTr("Autorizar %1, %2, huella %3").arg(keyRow.modelData.title).arg(keyRow.modelData.email || "").arg(keyRow.modelData.id)
                        onToggled: chooser.setChecked(keyRow.modelData.id, checked)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 2
                        Controls.Label {
                            Layout.fillWidth: true
                            text: keyRow.modelData.title
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!keyRow.modelData.email
                            text: keyRow.modelData.email || ""
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                    }
                    Controls.ToolButton {
                        id: keyDetails
                        text: checked ? qsTr("Ocultar detalles de la clave") : qsTr("Ver detalles de la clave")
                        icon.name: checked ? "arrow-up" : "arrow-down"
                        display: Controls.AbstractButton.IconOnly
                        checkable: true
                        Accessible.name: qsTr("%1: %2").arg(text).arg(keyRow.modelData.title)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    visible: keyDetails.checked
                    text: qsTr("Huella OpenPGP: %1").arg(keyRow.modelData.id)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    color: Kirigami.Theme.disabledTextColor
                }
            }
        }

        Repeater {
            model: chooser.unavailableFingerprints

            delegate: RowLayout {
                id: unavailableRow
                required property string modelData
                Layout.fillWidth: true
                spacing: 8
                Controls.CheckBox {
                    checked: chooser.selectedFingerprints.includes(unavailableRow.modelData)
                    Accessible.name: qsTr("Destinatario original no disponible, huella %1").arg(unavailableRow.modelData)
                    onToggled: chooser.setChecked(unavailableRow.modelData, checked)
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("%1 · sin clave válida para cifrar").arg(unavailableRow.modelData)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
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
