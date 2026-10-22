pragma ComponentBehavior: Bound
import QtQuick
import QtCore
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.ApplicationWindow {
    id: dialog
    required property KeyStore keyStore
    /// Vault primary key, preselected when it can still encrypt.
    property string primaryKey: ""
    property var showInFolderAction: null
    readonly property bool opened: visible
    readonly property bool busy: backend.busy || keyStore.controller.operationBusy
    property bool exportMode: true
    property url inputPath: ""
    property url outputPath: ""
    property url completedExportPath: ""
    property bool completed: false
    property bool keysExpanded: false
    property var selectedFingerprints: []
    readonly property var availableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property var selectedKeys: availableKeys.filter(key => selectedFingerprints.includes(key.id))
    readonly property var selectedPrivateKeys: selectedKeys.filter(key => key.hasSecret)
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))
    readonly property bool ready: exportMode ? !!outputPath.toString() && eligibleSelection : !!inputPath.toString()
    readonly property string hint: completed ? "" : exportMode ? (!outputPath.toString() ? qsTr("Elige dónde guardar la copia") : !eligibleSelection ? qsTr("Elige al menos una clave") : "") : (!inputPath.toString() ? qsTr("Elige la copia que quieres restaurar") : "")
    signal openVaultRequested(url path)

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 30
    height: Kirigami.Units.gridUnit * 30
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 18
    title: exportMode ? qsTr("Crear copia de seguridad") : qsTr("Restaurar copia de seguridad")

    function prepare(exporting) {
        if (busy)
            return;
        if (visible && exportMode === exporting) {
            raise();
            requestActivate();
            return;
        }
        exportMode = exporting;
        inputPath = "";
        outputPath = "";
        completedExportPath = "";
        completed = false;
        keysExpanded = false;
        const primary = availableKeys.find(key => key.id.toUpperCase() === primaryKey.toUpperCase());
        const ownKeys = availableKeys.filter(key => key.hasSecret);
        selectedFingerprints = primary ? [primary.id] : ownKeys.length === 1 ? [ownKeys[0].id] : [];
        if (exporting && selectedFingerprints.length === 0)
            keysExpanded = true;
        backend.clearError();
        if (!visible) {
            x = screen.virtualX + Math.round((screen.width - width) / 2);
            y = screen.virtualY + Math.round((screen.height - height) / 2);
        }
        show();
        raise();
        requestActivate();
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

    function displayPath(path) {
        return decodeURIComponent(path.toString().replace(/^file:\/\//, ""));
    }

    function fileName(path) {
        const parts = displayPath(path).split("/");
        return parts[parts.length - 1];
    }

    function keyName(key) {
        return (key.title || key.email) + (key.hasSecret ? qsTr(" (tu clave)") : "");
    }

    function chooseRestoreFile(url) {
        if (busy)
            return;
        inputPath = url;
        backend.clearError();
    }

    function submit() {
        if (!ready || busy)
            return;
        if (exportMode)
            backend.exportBackup(outputPath, JSON.stringify(selectedFingerprints));
        else
            backend.restoreManagedBackup(inputPath);
    }

    onClosing: function (event) {
        if (busy)
            event.accepted = false;
    }
    onVisibleChanged: {
        if (!visible) {
            sourceDialog.close();
            destinationDialog.close();
            completedExportPath = "";
        }
    }

    Backend.BackupController {
        id: backend
        onBackupExported: function (path) {
            dialog.completedExportPath = path;
            dialog.completed = true;
        }
        onBackupRestored: function (path) {
            dialog.close();
            dialog.openVaultRequested(path);
        }
    }

    KeyOperationDialog {
        id: keyRecovery
        keyStore: dialog.keyStore
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.visible && !dialog.busy && !keyRecovery.opened && !sourceDialog.visible && !destinationDialog.visible
        onActivated: dialog.close()
    }

    header: PanelHeader {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                source: dialog.exportMode ? "document-save" : "document-revert"
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
            }
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: dialog.exportMode ? qsTr("Copia cifrada de la bóveda actual") : qsTr("Restaurar en una bóveda nueva")
                elide: Text.ElideRight
            }
        }
    }

    Controls.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: scroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: !!backend.errorMessage
                type: Kirigami.MessageType.Error
                text: backend.errorMessage
            }

            // Export: where the file goes.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.exportMode && !dialog.completed
                text: qsTr("Destino")
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: dialog.exportMode && !dialog.completed
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "document-save"
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.outputPath.toString() ? dialog.fileName(dialog.outputPath) : qsTr("Sin destino")
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        elide: Text.ElideMiddle
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.outputPath.toString() ? dialog.displayPath(dialog.outputPath) : qsTr("Elige una carpeta fuera de este equipo si puedes")
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideMiddle
                    }
                }
                Controls.Button {
                    visible: !dialog.outputPath.toString()
                    text: qsTr("Elegir…")
                    icon.name: "document-save-as"
                    enabled: !dialog.busy
                    onClicked: destinationDialog.open()
                }
                Controls.ToolButton {
                    visible: !!dialog.outputPath.toString()
                    text: qsTr("Cambiar…")
                    icon.name: "document-save-as"
                    enabled: !dialog.busy
                    onClicked: destinationDialog.open()
                }
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.exportMode && !dialog.completed
                text: qsTr("Cifrar con")
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: dialog.exportMode && !dialog.completed && !dialog.keysExpanded
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "view-certificate"
                    implicitWidth: Kirigami.Units.iconSizes.smallMedium
                    implicitHeight: Kirigami.Units.iconSizes.smallMedium
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: dialog.selectedKeys.map(key => dialog.keyName(key)).join(", ")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.ToolButton {
                    text: qsTr("Cambiar…")
                    icon.name: "document-edit"
                    enabled: !dialog.busy
                    onClicked: dialog.keysExpanded = true
                }
            }
            Repeater {
                model: dialog.exportMode && !dialog.completed && dialog.keysExpanded ? dialog.availableKeys : []
                delegate: Controls.CheckDelegate {
                    required property var modelData
                    Layout.fillWidth: true
                    text: dialog.keyName(modelData)
                    enabled: !dialog.busy
                    checked: dialog.selectedFingerprints.includes(modelData.id)
                    Accessible.description: modelData.id
                    onToggled: {
                        const selected = dialog.selectedFingerprints.filter(fingerprint => fingerprint !== modelData.id);
                        if (checked)
                            selected.push(modelData.id);
                        dialog.selectedFingerprints = selected;
                        backend.clearError();
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.exportMode && !dialog.completed && dialog.availableKeys.length === 0
                text: dialog.keyStore.controller.busy ? qsTr("Buscando tus claves…") : qsTr("No hay claves válidas para cifrar. Crea o importa una clave desde Claves y vuelve a intentarlo.")
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.topMargin: Kirigami.Units.largeSpacing
                visible: dialog.exportMode && !dialog.completed && dialog.eligibleSelection
                type: dialog.selectedPrivateKeys.length > 0 ? Kirigami.MessageType.Information : Kirigami.MessageType.Warning
                text: dialog.selectedPrivateKeys.length > 0 ? qsTr("La copia no incluye tus claves privadas. Guárdalas aparte para poder restaurarla en otro equipo.") : qsTr("No tienes aquí la clave privada de las claves elegidas. Para restaurar necesitarás importarla o usar el equipo donde está.")
                actions: [
                    Kirigami.Action {
                        visible: dialog.selectedPrivateKeys.length > 0
                        text: qsTr("Respaldar clave privada…")
                        icon.name: "document-save-as"
                        enabled: !dialog.busy && !dialog.keyStore.controller.busy
                        onTriggered: keyRecovery.openFor("backupPrivate", dialog.selectedPrivateKeys[0].id)
                    }
                ]
            }

            // Restore: which file.
            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: !dialog.exportMode
                text: qsTr("Copia")
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.bottomMargin: Kirigami.Units.smallSpacing
                visible: !dialog.exportMode
                type: Kirigami.MessageType.Information
                text: qsTr("¿Cambiaste de equipo? Importa primero la clave privada que protege la copia; sin ella no se puede recuperar.")
                actions: [
                    Kirigami.Action {
                        text: qsTr("Importar clave privada…")
                        icon.name: "document-import"
                        enabled: !dialog.busy && !dialog.keyStore.controller.busy
                        onTriggered: keyRecovery.openFor("import")
                    }
                ]
            }

            DropArea {
                id: drop
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                implicitHeight: emptyFile.implicitHeight + Kirigami.Units.gridUnit * 2
                visible: !dialog.exportMode && !dialog.inputPath.toString()
                enabled: !dialog.busy
                onDropped: function (event) {
                    if (event.hasUrls)
                        dialog.chooseRestoreFile(event.urls[0]);
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Kirigami.Units.cornerRadius
                    color: drop.containsDrag ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent"
                    border.width: 1
                    border.color: drop.containsDrag ? Kirigami.Theme.highlightColor : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
                }
                ColumnLayout {
                    id: emptyFile
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignHCenter
                        source: "document-revert"
                        implicitWidth: Kirigami.Units.iconSizes.large
                        implicitHeight: Kirigami.Units.iconSizes.large
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("Suelta aquí una copia de KryptoPass")
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: qsTr("Archivos admitidos: .kpb, .gpg, .pgp")
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Button {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        text: qsTr("Examinar…")
                        icon.name: "document-open"
                        enabled: drop.enabled
                        onClicked: sourceDialog.open()
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: !dialog.exportMode && !!dialog.inputPath.toString()
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "application-pgp-encrypted"
                    fallback: "text-x-generic"
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.fileName(dialog.inputPath)
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        elide: Text.ElideMiddle
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.displayPath(dialog.inputPath)
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideMiddle
                    }
                }
                Controls.ToolButton {
                    text: qsTr("Cambiar…")
                    icon.name: "document-open"
                    enabled: !dialog.busy
                    onClicked: sourceDialog.open()
                }
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: !dialog.exportMode
                text: qsTr("Qué pasará")
            }
            Repeater {
                model: dialog.exportMode ? [] : [
                    {
                        icon: "folder-new",
                        text: qsTr("Se crea una bóveda nueva y se abre al terminar.")
                    },
                    {
                        icon: "lock",
                        text: qsTr("Tus datos actuales no se mezclan ni se modifican.")
                    },
                    {
                        icon: "view-certificate",
                        text: qsTr("Se usa tu clave privada automáticamente; GnuPG pedirá su contraseña si hace falta.")
                    }
                ]
                delegate: RowLayout {
                    id: stepRow
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignTop
                        source: stepRow.modelData.icon
                        implicitWidth: Kirigami.Units.iconSizes.smallMedium
                        implicitHeight: Kirigami.Units.iconSizes.smallMedium
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: stepRow.modelData.text
                        wrapMode: Text.Wrap
                    }
                }
            }
            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.gridUnit * 3
                visible: dialog.exportMode && dialog.completed
                icon.name: "dialog-ok-apply"
                text: qsTr("Copia guardada")
                explanation: dialog.displayPath(dialog.completedExportPath)
                helpfulAction: dialog.showInFolderAction
            }

            Item {
                Layout.preferredHeight: Kirigami.Units.largeSpacing
            }
        }
    }

    footer: PanelFooter {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Controls.BusyIndicator {
                running: backend.busy
                visible: running
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
                Accessible.name: backend.status
            }
            Controls.Label {
                Layout.fillWidth: true
                text: backend.busy ? backend.status : dialog.hint
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
            }
            Controls.Button {
                text: dialog.completed ? qsTr("Cerrar") : qsTr("Cancelar")
                icon.name: dialog.completed ? "dialog-close" : "dialog-cancel"
                enabled: !dialog.busy
                onClicked: dialog.close()
            }
            Controls.Button {
                visible: !dialog.completed
                text: dialog.exportMode ? qsTr("Crear copia") : qsTr("Restaurar y abrir")
                icon.name: dialog.exportMode ? "document-save" : "document-revert"
                enabled: dialog.ready && !dialog.busy
                onClicked: dialog.submit()
            }
        }
    }

    Dialogs.FileDialog {
        id: sourceDialog
        title: qsTr("Seleccionar copia de seguridad")
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Copia cifrada de KryptoPass (*.kpb *.gpg *.pgp)"), qsTr("Todos los archivos (*)")]
        onAccepted: dialog.chooseRestoreFile(selectedFile)
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
