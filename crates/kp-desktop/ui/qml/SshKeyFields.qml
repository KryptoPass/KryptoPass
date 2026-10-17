pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami

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
        title: qsTr("Importar clave privada OpenSSH")
        nameFilters: [qsTr("Claves privadas OpenSSH (*)"), qsTr("Todos los archivos (*)")]
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
                text: qsTr("Clave privada")
                font.weight: Font.DemiBold
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Importa un archivo de clave privada OpenSSH. Si está cifrado, escribe su frase de contraseña.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Frase de contraseña")
                value: root.passphrase
                concealed: true
                onValueEdited: function(value) { root.passphrase = value; }
            }
            Controls.Button {
                text: qsTr("Elegir archivo de clave…")
                icon.name: "document-open"
                onClicked: keyFileDialog.open()
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!root.importToken
                text: qsTr("La clave seleccionada está lista para guardar.")
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
                    text: qsTr("Datos públicos de la clave")
                    font.weight: Font.DemiBold
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!root.algorithm
                    text: qsTr("Algoritmo: %1").arg(root.algorithm)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!root.fingerprint
                    text: qsTr("Huella: %1").arg(root.fingerprint)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    Accessible.name: qsTr("Huella de la clave SSH")
                }
                Controls.TextArea {
                    Layout.fillWidth: true
                    visible: !!root.publicKey
                    text: root.publicKey
                    readOnly: true
                    selectByMouse: true
                    wrapMode: TextEdit.WrapAnywhere
                    textFormat: TextEdit.PlainText
                    Accessible.name: qsTr("Clave pública SSH")
                }
            }
        }

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Usuario")
            value: root.user
            onValueEdited: function(value) { root.user = value; }
        }
        StringListEditor {
            objectName: "sshHostsEditor"
            Layout.fillWidth: true
            label: qsTr("Equipos")
            inputLabel: qsTr("Equipo o host")
            addLabel: qsTr("Agregar equipo")
            values: root.hosts
            onValuesEdited: function(values) { root.hosts = values; }
        }
        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
