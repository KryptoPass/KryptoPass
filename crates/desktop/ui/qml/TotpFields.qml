pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string totpInput: ""
    property string notes: ""
    property bool inputVisible: false

    spacing: Kirigami.Units.largeSpacing

    onTotpInputChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        totpInput = String(fields.totpInput || "");
        notes = String(fields.notes || "");
        inputVisible = false;
    }

    function currentFields() {
        return { totpInput: totpInput, notes: notes };
    }

    function clearFields() { loadFields({}); }

    function applyPreparedTotp(json) {
        let prepared = json;
        if (typeof json === "string") {
            try {
                prepared = JSON.parse(json);
            } catch (error) {
                return;
            }
        }
        if (!prepared || typeof prepared.totpInput !== "string")
            return;
        totpInput = prepared.totpInput;
        inputVisible = false;
    }

    Dialogs.FileDialog {
        id: qrFileDialog
        title: qsTr("Importar código QR de TOTP")
        nameFilters: [qsTr("Imágenes PNG o JPEG (*.png *.jpg *.jpeg)"), qsTr("Todos los archivos (*)")]
        onVisibleChanged: root.externalModalRequested(visible)
        onAccepted: root.prepareTotpQrRequested(selectedFile)
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Semilla o URI TOTP")
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.TextField {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: root.totpInput
                    echoMode: root.inputVisible ? TextInput.Normal : TextInput.Password
                    placeholderText: qsTr("Base32 u otpauth://")
                    Accessible.name: qsTr("Semilla o URI TOTP, inicialmente oculta")
                    onTextEdited: root.totpInput = text
                }
                Controls.ToolButton {
                    text: root.inputVisible ? qsTr("Ocultar semilla TOTP") : qsTr("Mostrar semilla TOTP")
                    icon.name: root.inputVisible ? "view-hidden" : "view-visible"
                    display: Controls.AbstractButton.IconOnly
                    checkable: true
                    checked: root.inputVisible
                    Accessible.name: text
                    onClicked: root.inputVisible = !root.inputVisible
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Introduce la semilla Base32 completa o una URI otpauth://. También puedes leerla desde una imagen QR.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Controls.Button {
                text: qsTr("Leer código QR de una imagen…")
                icon.name: "view-barcode-qr"
                onClicked: qrFileDialog.open()
            }
        }

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Notas")
            value: root.notes
            multiline: true
            onValueEdited: function(value) { root.notes = value; }
        }
    }
}
