pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.Dialog {
    id: dialog
    required property KeyStore keyStore
    readonly property bool busy: backend.busy
    property bool exportMode: true
    property url inputPath: ""
    property url outputPath: ""
    property url restoredPath: ""
    property string completionMessage: ""
    signal openVaultRequested(url path)

    anchors.centerIn: parent
    width: Math.min(560, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: exportMode ? qsTr("Crear backup cifrado") : qsTr("Restaurar en una bóveda nueva")

    function prepare(exporting) {
        if (busy)
            return;
        exportMode = exporting;
        inputPath = "";
        outputPath = "";
        restoredPath = "";
        completionMessage = "";
        recipients.selectedFingerprints = [];
        backend.clearError();
        open();
    }
    function openForExport() { prepare(true); }
    function openForRestore() { prepare(false); }

    Backend.BackupController {
        id: backend
        onBackupExported: function(path) {
            dialog.completionMessage = qsTr("Backup cifrado guardado.");
        }
        onBackupRestored: function(path) {
            dialog.restoredPath = path;
            dialog.completionMessage = qsTr("Bóveda nueva restaurada. La bóveda actual se conserva.");
        }
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.opened && !dialog.busy
        onActivated: dialog.close()
    }

    contentItem: Controls.ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        implicitHeight: Math.min(form.implicitHeight + Kirigami.Units.largeSpacing * 2,
                                 Math.max(200, dialog.parent.height - Kirigami.Units.gridUnit * 10))

        Controls.Pane {
            width: scroll.availableWidth
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                id: form
                spacing: Kirigami.Units.largeSpacing

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!backend.errorMessage
                    type: Kirigami.MessageType.Error
                    text: backend.errorMessage
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!dialog.completionMessage
                    type: Kirigami.MessageType.Positive
                    text: dialog.completionMessage
                }
                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: qsTr("El backup cifra también los nombres, carpetas y destinatarios. No incluye claves privadas: para recuperar cada secreto necesitas una de sus claves originales, además de una clave que abra el backup.")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.exportMode
                    wrapMode: Text.Wrap
                    text: qsTr("Elige un archivo nuevo dentro de una carpeta privada (permisos 0700). No se sobrescribe ni se fusiona una bóveda existente.")
                }
                Controls.Button {
                    visible: !dialog.exportMode
                    enabled: !dialog.busy
                    text: qsTr("Seleccionar backup…")
                    icon.name: "document-open"
                    onClicked: sourceDialog.open()
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.exportMode && !!dialog.inputPath.toString()
                    text: dialog.inputPath.toString()
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Controls.Button {
                    enabled: !dialog.busy
                    text: dialog.exportMode ? qsTr("Elegir destino del backup…") : qsTr("Elegir archivo de bóveda nueva…")
                    icon.name: "document-save-as"
                    onClicked: destinationDialog.open()
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!dialog.outputPath.toString()
                    text: dialog.outputPath.toString()
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                RecipientsChooser {
                    id: recipients
                    Layout.fillWidth: true
                    visible: dialog.exportMode
                    enabled: !dialog.busy
                    keyStore: dialog.keyStore
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!backend.status
                    text: backend.status
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cerrar")
            enabled: !dialog.busy
            onClicked: dialog.close()
        }
        Controls.Button {
            visible: !!dialog.restoredPath.toString()
            text: qsTr("Abrir bóveda restaurada")
            enabled: !dialog.busy
            onClicked: {
                dialog.openVaultRequested(dialog.restoredPath);
                dialog.close();
            }
        }
        Controls.Button {
            text: dialog.exportMode ? qsTr("Crear backup") : qsTr("Restaurar")
            icon.name: dialog.exportMode ? "document-save" : "document-revert"
            enabled: !dialog.busy && !dialog.completionMessage
                     && !!dialog.outputPath.toString()
                     && (dialog.exportMode ? recipients.eligibleSelection : !!dialog.inputPath.toString())
            onClicked: {
                if (dialog.exportMode)
                    backend.exportBackup(dialog.outputPath, JSON.stringify(recipients.selectedFingerprints));
                else
                    backend.restoreBackup(dialog.inputPath, dialog.outputPath);
            }
        }
    }

    Dialogs.FileDialog {
        id: sourceDialog
        title: qsTr("Abrir backup cifrado")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [qsTr("Backup OpenPGP (*.kpb *.gpg *.pgp)"), qsTr("Todos los archivos (*)")]
        onAccepted: dialog.inputPath = selectedFile
    }
    Dialogs.FileDialog {
        id: destinationDialog
        title: dialog.exportMode ? qsTr("Guardar backup cifrado") : qsTr("Crear bóveda restaurada")
        fileMode: Dialogs.FileDialog.SaveFile
        currentFolder: backend.vaultDirectoryUrl
        defaultSuffix: dialog.exportMode ? "kpb" : "sqlite3"
        nameFilters: dialog.exportMode ? [qsTr("Backup cifrado (*.kpb)")] : [qsTr("Bóveda SQLite (*.sqlite3)")]
        onAccepted: dialog.outputPath = selectedFile
    }
}
