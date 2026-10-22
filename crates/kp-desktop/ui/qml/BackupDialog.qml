pragma ComponentBehavior: Bound
import QtQuick
import QtCore
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.Dialog {
    id: dialog
    required property KeyStore keyStore
    readonly property bool busy: backend.busy || keyStore.controller.operationBusy
    property bool exportMode: true
    property url inputPath: ""
    property url outputPath: ""
    property string completionMessage: ""
    property url completedExportPath: ""
    property var showInFolderAction: null
    signal openVaultRequested(url path)

    anchors.centerIn: parent
    width: Math.min(560, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: exportMode ? qsTr("Crear copia de seguridad") : qsTr("Restaurar copia de seguridad")

    function displayPath(path) {
        return decodeURIComponent(path.toString().replace(/^file:\/\//, ""));
    }

    function prepare(exporting) {
        if (busy)
            return;
        exportMode = exporting;
        inputPath = "";
        outputPath = "";
        completionMessage = "";
        completedExportPath = "";
        const ownKeys = recipients.availableKeys.filter(key => key.hasSecret);
        recipients.selectedFingerprints = ownKeys.length === 1 ? [ownKeys[0].id] : [];
        backend.clearError();
        open();
    }
    function openForExport() {
        prepare(true);
    }
    function openForRestore() {
        prepare(false);
    }
    function openForRestoreFile(path) {
        prepare(false);
        if (opened)
            inputPath = path;
    }

    Backend.BackupController {
        id: backend
        onBackupExported: function (path) {
            dialog.completedExportPath = path;
            dialog.completionMessage = qsTr("Copia de seguridad guardada en %1.").arg(dialog.displayPath(path));
        }
        onBackupRestored: function (path) {
            dialog.close();
            dialog.openVaultRequested(path);
        }
    }

    onClosed: completedExportPath = ""

    KeyOperationDialog {
        id: keyRecovery
        keyStore: dialog.keyStore
        onClosed: {
            if (completionMessage)
                backend.clearError();
        }
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.opened && !dialog.busy && !keyRecovery.opened
        onActivated: dialog.close()
    }

    contentItem: Controls.ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        implicitHeight: Math.min(form.implicitHeight + Kirigami.Units.largeSpacing * 2, Math.max(200, dialog.parent.height - Kirigami.Units.gridUnit * 10))

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
                    visible: !dialog.completionMessage
                    text: dialog.exportMode ? qsTr("Guarda una copia cifrada de los datos de la bóveda actual, incluidos nombres y carpetas.") : qsTr("Crearemos y abriremos una bóveda nueva con los datos de la copia. No se mezclan con los datos que estás viendo ahora.")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.completionMessage
                    wrapMode: Text.Wrap
                    text: dialog.exportMode ? qsTr("Para restaurar en otra PC también necesitas tus claves privadas. La copia de datos no las incluye: guárdalas por separado.") : qsTr("Usaremos automáticamente una de las claves con las que protegiste la copia. Su parte privada debe estar en tu llavero; no necesitas elegirla otra vez.")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.exportMode && !dialog.completionMessage
                    text: qsTr("¿Cambiaste de PC? Importa tu clave privada antes de restaurar. La clave pública no basta. Si perdiste todas las claves privadas que abren la copia, no se puede recuperar.")
                    wrapMode: Text.Wrap
                }
                Controls.Button {
                    visible: !dialog.exportMode && !dialog.completionMessage
                    enabled: !dialog.busy && !dialog.keyStore.controller.busy
                    text: qsTr("Importar clave privada…")
                    icon.name: "document-import"
                    onClicked: keyRecovery.openFor("import")
                }
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    visible: !dialog.completionMessage
                    text: dialog.exportMode ? qsTr("1. Dónde guardar la copia") : qsTr("1. Qué copia restaurar")
                }
                Controls.Button {
                    visible: !dialog.exportMode && !dialog.completionMessage
                    enabled: !dialog.busy
                    text: dialog.displayPath(dialog.inputPath) ? qsTr("Cambiar copia…") : qsTr("Seleccionar copia…")
                    icon.name: "document-open"
                    onClicked: sourceDialog.open()
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.exportMode && !!dialog.inputPath.toString() && !dialog.completionMessage
                    text: dialog.displayPath(dialog.inputPath)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Controls.Button {
                    visible: dialog.exportMode && !dialog.completionMessage
                    enabled: !dialog.busy
                    text: dialog.displayPath(dialog.outputPath) ? qsTr("Cambiar destino…") : qsTr("Elegir dónde guardar…")
                    icon.name: "document-save-as"
                    onClicked: destinationDialog.open()
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: dialog.exportMode && !!dialog.outputPath.toString() && !dialog.completionMessage
                    text: dialog.displayPath(dialog.outputPath)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    visible: !dialog.completionMessage
                    text: dialog.exportMode ? qsTr("2. Quién podrá abrir la copia") : qsTr("2. Dónde aparecerán tus datos")
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.exportMode && !dialog.completionMessage
                    text: qsTr("Al terminar verás los datos restaurados automáticamente. GnuPG pedirá la contraseña de la clave solo si necesita desbloquearla. Para leer tus secretos también necesitas sus claves originales.")
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: dialog.exportMode && !dialog.completionMessage
                    text: qsTr("Selecciona al menos una clave. Asegúrate de conservar su clave privada para poder restaurar la copia.")
                    wrapMode: Text.Wrap
                }
                ColumnLayout {
                    id: recipients
                    Layout.fillWidth: true
                    visible: dialog.exportMode && !dialog.completionMessage
                    enabled: !dialog.busy
                    property var selectedFingerprints: []
                    readonly property var availableKeys: dialog.keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
                    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))

                    Repeater {
                        model: recipients.availableKeys
                        delegate: Controls.CheckBox {
                            id: keyChoice
                            required property var modelData
                            Layout.fillWidth: true
                            text: (modelData.title || modelData.email) + (modelData.hasSecret ? qsTr(" (tu clave)") : "")
                            checked: recipients.selectedFingerprints.includes(modelData.id)
                            Accessible.description: modelData.id
                            onToggled: {
                                const selected = recipients.selectedFingerprints.filter(fingerprint => fingerprint !== modelData.id);
                                if (checked)
                                    selected.push(modelData.id);
                                recipients.selectedFingerprints = selected;
                                backend.clearError();
                            }
                        }
                    }
                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        visible: recipients.eligibleSelection && !recipients.availableKeys.some(key => key.hasSecret && recipients.selectedFingerprints.includes(key.id))
                        type: Kirigami.MessageType.Information
                        text: qsTr("No tienes aquí la clave privada de las claves seleccionadas. Para restaurar esta copia tendrás que importarla o usar la PC donde la conservas.")
                    }
                    Repeater {
                        model: recipients.availableKeys.filter(key => key.hasSecret && recipients.selectedFingerprints.includes(key.id))
                        delegate: Controls.Button {
                            id: privateKeyBackup
                            required property var modelData
                            Layout.fillWidth: true
                            text: qsTr("Respaldar clave privada…")
                            Accessible.name: qsTr("Respaldar la clave privada de %1").arg(modelData.title || modelData.email)
                            enabled: !dialog.busy && !dialog.keyStore.controller.busy
                            icon.name: "document-save-as"
                            onClicked: keyRecovery.openFor("backupPrivate", modelData.id)
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: recipients.availableKeys.length === 0
                        text: dialog.keyStore.controller.busy ? qsTr("Buscando tus claves…") : qsTr("No hay claves disponibles para proteger la copia. Importa o crea una clave desde Claves y vuelve a intentarlo.")
                        wrapMode: Text.Wrap
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !dialog.busy && !dialog.completionMessage && !backend.errorMessage
                    text: dialog.exportMode ? (!dialog.outputPath.toString() ? qsTr("Elige dónde guardar la copia para continuar.") : !recipients.eligibleSelection ? qsTr("Selecciona una clave válida para continuar.") : qsTr("Todo listo para crear la copia.")) : (!dialog.inputPath.toString() ? qsTr("Selecciona una copia para continuar.") : qsTr("Todo listo para restaurar en una bóveda nueva."))
                    wrapMode: Text.Wrap
                }
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    visible: dialog.busy
                    running: visible
                    Accessible.name: backend.status
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!backend.status && !backend.errorMessage
                    text: backend.status
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            visible: dialog.exportMode && !!dialog.completedExportPath.toString() && !!dialog.showInFolderAction
            action: dialog.showInFolderAction
        }
        Controls.Button {
            text: qsTr("Cerrar")
            enabled: !dialog.busy
            onClicked: dialog.close()
        }
        Controls.Button {
            visible: !dialog.completionMessage
            text: dialog.exportMode ? qsTr("Crear copia") : qsTr("Restaurar y abrir")
            icon.name: dialog.exportMode ? "document-save" : "document-revert"
            enabled: !dialog.busy && !dialog.completionMessage && (dialog.exportMode ? (!!dialog.outputPath.toString() && recipients.eligibleSelection) : !!dialog.inputPath.toString())
            onClicked: {
                if (dialog.exportMode)
                    backend.exportBackup(dialog.outputPath, JSON.stringify(recipients.selectedFingerprints));
                else
                    backend.restoreManagedBackup(dialog.inputPath);
            }
        }
    }

    Dialogs.FileDialog {
        id: sourceDialog
        title: qsTr("Seleccionar copia de seguridad")
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Copia cifrada de KryptoPass (*.kpb *.gpg *.pgp)"), qsTr("Todos los archivos (*)")]
        onAccepted: {
            dialog.inputPath = selectedFile;
            backend.clearError();
        }
    }
    Dialogs.FileDialog {
        id: destinationDialog
        title: qsTr("Guardar copia de seguridad")
        fileMode: Dialogs.FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        defaultSuffix: "kpb"
        nameFilters: [qsTr("Copia cifrada de KryptoPass (*.kpb)")]
        onAccepted: {
            dialog.outputPath = selectedFile;
            backend.clearError();
        }
    }
}
