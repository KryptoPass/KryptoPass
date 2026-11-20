pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
        notesEditor.resetSource(notes);
        inputVisible = false;
    }

    function currentFields() {
        return {
            totpInput: totpInput,
            notes: notes
        };
    }

    function clearFields() {
        loadFields({});
    }

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
        title: KI18n.i18nc("@title:window", "Import TOTP QR Code")
        nameFilters: [KI18n.i18n("PNG or JPEG images (*.png *.jpg *.jpeg)"), KI18n.i18n("All files (*)")]
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
                text: KI18n.i18nc("@title:group", "TOTP secret or URI")
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Controls.TextField {
                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: root.totpInput
                    echoMode: root.inputVisible ? TextInput.Normal : TextInput.Password
                    passwordMaskDelay: 0
                    placeholderText: KI18n.i18nc("@info:placeholder", "Base32 or otpauth://")
                    Accessible.name: KI18n.i18n("TOTP secret or URI, hidden at first")
                    onTextEdited: root.totpInput = text
                }
                Controls.ToolButton {
                    text: root.inputVisible ? KI18n.i18nc("@action:button", "Hide TOTP Secret") : KI18n.i18nc("@action:button", "Show TOTP Secret")
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
                text: KI18n.i18n("Enter the full Base32 secret or an otpauth:// URI. You can also read it from a QR image.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Read QR Code from Image…")
                icon.name: "view-barcode-qr"
                onClicked: qrFileDialog.open()
            }
        }

        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
