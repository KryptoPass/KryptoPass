pragma ComponentBehavior: Bound
import QtQuick
import QtCore
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend
import org.kde.ki18n

Controls.ApplicationWindow {
    id: dialog
    required property KeyStore keyStore
    /// Vault primary key, preselected when it can still encrypt.
    property string primaryKey: ""
    property bool hasVault: false
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
    readonly property bool hasPrivateKeys: keyStore.entries.some(key => key.hasSecret)
    readonly property var selectedKeys: availableKeys.filter(key => selectedFingerprints.includes(key.id))
    readonly property var selectedPrivateKeys: selectedKeys.filter(key => key.hasSecret)
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))
    readonly property bool ready: exportMode ? !!outputPath.toString() && eligibleSelection : !!inputPath.toString()
    signal openVaultRequested(url path)

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 30
    height: Kirigami.Units.gridUnit * 30
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 18
    title: exportMode ? KI18n.i18nc("@title:window", "Export Backup") : KI18n.i18nc("@title:window", "Restore Backup")

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
        restoreKeyNotice.visible = !exporting;
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
        return (key.title || key.email) + (key.hasSecret ? KI18n.i18nc("@label suffix after a key name", " (your key)") : "");
    }

    function chooseRestoreFile(url) {
        if (busy)
            return;
        inputPath = url;
        backend.clearError();
    }

    /// First local file of a drop. On Wayland, Qt can hand back the portal
    /// transfer key as if it were the URL when the portal fails, so only
    /// file: URLs count, and the KDE/Mozilla lists (not routed through the
    /// portal) are read too.
    function droppedFileUrl(event) {
        const isFile = value => /^file:\/\//i.test(value);
        const fromList = text => (text || "").split(/\r?\n/).map(line => line.replace(/\0/g, "").trim()).find(line => isFile(line)) || "";
        for (const url of event.urls || []) {
            if (isFile(url.toString()))
                return url.toString();
        }
        for (const format of ["application/x-kde4-urilist", "text/x-moz-url", "text/uri-list"]) {
            if (!event.formats.includes(format))
                continue;
            const found = fromList(event.getDataAsString(format));
            if (found)
                return found;
        }
        return fromList(event.text);
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

    Controls.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: scroll.availableWidth
            height: Math.max(scroll.availableHeight, implicitHeight)
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
                text: KI18n.i18nc("@title:group", "Destination")
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
                        text: dialog.outputPath.toString() ? dialog.fileName(dialog.outputPath) : KI18n.i18n("No destination")
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        elide: Text.ElideMiddle
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.outputPath.toString() ? dialog.displayPath(dialog.outputPath) : KI18n.i18n("Choose a folder outside this computer if you can")
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideMiddle
                    }
                }
                Controls.Button {
                    visible: !dialog.outputPath.toString()
                    text: KI18n.i18nc("@action:button", "Choose…")
                    icon.name: "document-save-as"
                    enabled: !dialog.busy
                    onClicked: destinationDialog.open()
                }
                Controls.ToolButton {
                    visible: !!dialog.outputPath.toString()
                    text: KI18n.i18nc("@action:button", "Change…")
                    icon.name: "document-save-as"
                    enabled: !dialog.busy
                    onClicked: destinationDialog.open()
                }
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.exportMode && !dialog.completed
                text: KI18n.i18nc("@title:group", "Encrypt with")
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
                    text: KI18n.i18nc("@action:button", "Change…")
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
                text: dialog.keyStore.controller.busy ? KI18n.i18n("Looking for your keys…") : KI18n.i18n("There are no valid keys to encrypt with. Create or import a key from Keys and try again.")
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
                text: dialog.selectedPrivateKeys.length > 0 ? KI18n.i18n("The backup does not include your private keys. Keep them separately so you can restore it on another computer.") : KI18n.i18n("You do not have the private key of the chosen keys here. To restore, you will need to import it or use the computer where it is.")
                actions: [
                    Kirigami.Action {
                        visible: dialog.selectedPrivateKeys.length > 0
                        text: KI18n.i18nc("@action:button", "Back Up Private Key…")
                        icon.name: "document-save-as"
                        enabled: !dialog.busy && !dialog.keyStore.controller.busy
                        onTriggered: keyRecovery.openFor("backupPrivate", dialog.selectedPrivateKeys[0].id)
                    }
                ]
            }

            // Restore: which file.
            Kirigami.InlineMessage {
                id: restoreKeyNotice
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.topMargin: Kirigami.Units.largeSpacing
                Layout.bottomMargin: Kirigami.Units.smallSpacing
                visible: !dialog.exportMode
                type: dialog.hasPrivateKeys ? Kirigami.MessageType.Information : Kirigami.MessageType.Warning
                icon.name: dialog.hasPrivateKeys ? "dialog-information" : "dialog-warning"
                showCloseButton: dialog.hasPrivateKeys
                text: KI18n.i18n("Did you change computers? First import the private key that protects the backup; without it, the backup cannot be recovered.")
                actions: [
                    Kirigami.Action {
                        text: KI18n.i18nc("@action:button", "Import Private Key…")
                        icon.name: "view-certificate-import"
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
                Layout.topMargin: Kirigami.Units.smallSpacing
                Layout.bottomMargin: Kirigami.Units.smallSpacing
                Layout.fillHeight: true
                implicitHeight: emptyFile.implicitHeight + Kirigami.Units.gridUnit * 2
                visible: !dialog.exportMode
                enabled: !dialog.busy
                onEntered: function (drag) {
                    drag.accepted = drag.hasUrls || drag.formats.includes("text/uri-list");
                    if (drag.accepted)
                        drag.acceptProposedAction();
                }
                onDropped: function (event) {
                    const url = dialog.droppedFileUrl(event);
                    console.info("Backup drop formats=" + event.formats.join(",") + " local=" + !!url);
                    if (!url) {
                        backend.clearError();
                        return;
                    }
                    event.acceptProposedAction();
                    dialog.chooseRestoreFile(url);
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
                    visible: !dialog.inputPath.toString()
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
                        text: KI18n.i18n("Drop a KryptoPass backup here")
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: KI18n.i18n("Supported files: .kpb, .gpg, .pgp")
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Button {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        text: KI18n.i18nc("@action:button", "Browse…")
                        icon.name: "document-open"
                        enabled: drop.enabled
                        onClicked: sourceDialog.open()
                    }
                }
                RowLayout {
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    visible: !!dialog.inputPath.toString()
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
                    Controls.Button {
                        text: KI18n.i18nc("@action:button", "Change…")
                        icon.name: "document-open"
                        enabled: !dialog.busy
                        onClicked: sourceDialog.open()
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.topMargin: Kirigami.Units.smallSpacing
                visible: !dialog.exportMode
                text: dialog.hasVault ? KI18n.i18n("The backup is restored without overwriting the current vault.") : KI18n.i18n("A new vault will be created from this backup, and GnuPG will ask for your private key's passphrase.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.gridUnit * 3
                visible: dialog.exportMode && dialog.completed
                icon.name: "dialog-ok-apply"
                text: KI18n.i18nc("@title", "Backup saved")
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
                visible: backend.busy
                text: backend.status
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
            }
            Item {
                Layout.fillWidth: true
            }
            Controls.Button {
                text: dialog.completed ? KI18n.i18nc("@action:button", "Close") : KI18n.i18nc("@action:button", "Cancel")
                icon.name: dialog.completed ? "dialog-close" : "dialog-cancel"
                enabled: !dialog.busy
                onClicked: dialog.close()
            }
            Controls.Button {
                visible: !dialog.completed
                text: dialog.exportMode ? KI18n.i18nc("@action:button", "Create Backup") : KI18n.i18nc("@action:button", "Restore and Open")
                icon.name: dialog.exportMode ? "document-save" : "document-revert"
                enabled: dialog.ready && !dialog.busy
                onClicked: dialog.submit()
            }
        }
    }

    Dialogs.FileDialog {
        id: sourceDialog
        title: KI18n.i18nc("@title:window", "Select Backup")
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [KI18n.i18n("Encrypted KryptoPass backup (*.kpb *.gpg *.pgp)"), KI18n.i18n("All files (*)")]
        onAccepted: dialog.chooseRestoreFile(selectedFile)
    }
    Dialogs.FileDialog {
        id: destinationDialog
        title: KI18n.i18nc("@title:window", "Save Backup")
        fileMode: Dialogs.FileDialog.SaveFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        defaultSuffix: "kpb"
        nameFilters: [KI18n.i18n("Encrypted KryptoPass backup (*.kpb)")]
        onAccepted: {
            dialog.outputPath = selectedFile;
            backend.clearError();
        }
    }
}
