pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretFields {
    id: root

    property string importToken: ""
    property string passphrase: ""
    property string user: ""
    property var hosts: []
    property string notes: ""
    property string publicKey: ""
    property string fingerprint: ""
    property string algorithm: ""

    spacing: Kirigami.Units.largeSpacing

    onImportTokenChanged: edited()
    onPassphraseChanged: edited()
    onUserChanged: edited()
    onHostsChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        importToken = String(fields.importToken || "");
        passphrase = String(fields.passphrase || "");
        user = String(fields.user || "");
        hosts = (Array.isArray(fields.hosts) ? fields.hosts : []).map(host => String(host || ""));
        notes = String(fields.notes || "");
        notesEditor.resetSource(notes);
        publicKey = String(fields.publicKey || "");
        fingerprint = String(fields.fingerprint || "");
        algorithm = String(fields.algorithm || "");
    }

    function currentFields() {
        return {
            importToken: importToken,
            passphrase: "",
            user: user,
            hosts: hosts.slice(),
            notes: notes
        };
    }

    function clearFields() {
        loadFields({});
    }

    function applyPreparedSshKey(json) {
        let prepared = json;
        if (typeof json === "string") {
            try {
                prepared = JSON.parse(json);
            } catch (error) {
                return;
            }
        }
        if (!prepared || typeof prepared.importToken !== "string")
            return;
        importToken = prepared.importToken;
        publicKey = String(prepared.publicKey || "");
        fingerprint = String(prepared.fingerprint || "");
        algorithm = String(prepared.algorithm || "");
        passphrase = "";
    }

    Dialogs.FileDialog {
        id: keyFileDialog
        title: KI18n.i18nc("@title:window", "Import OpenSSH Private Key")
        nameFilters: [KI18n.i18n("OpenSSH private keys (*)"), KI18n.i18n("All files (*)")]
        onVisibleChanged: root.externalModalRequested(visible)
        onAccepted: root.prepareSshKeyRequested(selectedFile, root.passphrase)
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18nc("@title:group", "Private key")
                font.weight: Font.DemiBold
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("Import an OpenSSH private key file. If it is encrypted, enter its passphrase.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            SecretInput {
                Layout.fillWidth: true
                label: KI18n.i18nc("@label:textbox", "Passphrase")
                value: root.passphrase
                concealed: true
                onValueEdited: function (value) {
                    root.passphrase = value;
                }
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Choose Key File…")
                icon.name: "document-open"
                onClicked: keyFileDialog.open()
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!root.importToken
                text: KI18n.i18n("The selected key is ready to save.")
                color: Kirigami.Theme.positiveTextColor
                wrapMode: Text.Wrap
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            visible: !!root.publicKey || !!root.fingerprint || !!root.algorithm
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18nc("@title:group", "Public key data")
                    font.weight: Font.DemiBold
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!root.algorithm
                    text: KI18n.i18n("Algorithm: %1", root.algorithm)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!root.fingerprint
                    text: KI18n.i18n("Fingerprint: %1", root.fingerprint)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    Accessible.name: KI18n.i18n("SSH key fingerprint")
                }
                Controls.TextArea {
                    Layout.fillWidth: true
                    visible: !!root.publicKey
                    text: root.publicKey
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.WrapAnywhere
                    textFormat: TextEdit.PlainText
                    Accessible.name: KI18n.i18n("SSH public key")
                }
            }
        }

        SecretInput {
            Layout.fillWidth: true
            label: KI18n.i18nc("@label:textbox", "User")
            value: root.user
            onValueEdited: function (value) {
                root.user = value;
            }
        }
        StringListEditor {
            objectName: "sshHostsEditor"
            Layout.fillWidth: true
            label: KI18n.i18nc("@title:group", "Hosts")
            inputLabel: KI18n.i18nc("@label:textbox", "Host")
            addLabel: KI18n.i18nc("@action:button", "Add Host")
            values: root.hosts
            onValuesEdited: function (values) {
                root.hosts = values;
            }
        }
        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
