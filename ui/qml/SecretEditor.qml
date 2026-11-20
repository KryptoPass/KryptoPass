pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Pane {
    id: editor
    objectName: "entryEditor"

    required property SecretStore storeAdapter
    required property KeyStore keyStore
    required property real headerHeight
    required property Item focusedItem

    property string entryId: ""
    property string relatedEntryId: ""
    property string draftFolderId: ""
    property var draftIcon: null
    property url draftIconSource: ""
    property var revision: null
    property bool loaded: false
    property bool waitingForEditor: false
    property bool savePending: false
    property bool dirty: false
    property bool externalModalOpen: false
    property bool preparingSecretMaterial: false
    DraftBaseline {
        id: draftBaseline
    }
    property string requestedId: ""
    property string pendingEntryId: ""
    property string discardAction: ""
    property string saveError: ""
    property string invalidField: ""
    property var pendingRelatedEntry: null
    property bool opened: false
    property var pendingNavigation: null
    readonly property string title: entryId ? KI18n.i18nc("@title", "Edit Item") : KI18n.i18nc("@title", "New Item")

    readonly property bool saving: storeAdapter.controller.saving
    readonly property bool canSave: loaded && fieldsHost.ready && !saving && !savePending && !preparingSecretMaterial && !storeAdapter.controller.busy && !storeAdapter.controller.editorBusy && !storeAdapter.controller.iconBusy && titleField.text.trim().length > 0 && recipientChooser.eligibleSelection && (!entryId || dirty)
    readonly property string iconStateText: storeAdapter.controller.iconBusy ? KI18n.i18nc("@info:progress", "Preparing image…") : draftIcon?.kind === "image" ? KI18n.i18nc("@label item icon", "Custom image") : draftIcon?.kind === "theme" ? draftIcon.name : KI18n.i18nc("@label item icon", "Automatic")
    readonly property string saveBlockReason: {
        if (!loaded)
            return "";
        const missing = [];
        if (!titleField.text.trim())
            missing.push(KI18n.i18nc("@label", "Name"));
        for (const label of fieldsHost.missingRequiredFields)
            missing.push(label);
        if (missing.length === 1)
            return KI18n.i18n("Still missing: %1.", missing[0]);
        if (missing.length > 1)
            return KI18n.i18n("Still missing: %1 and %2.", missing[0], missing[1]);
        if (!recipientChooser.eligibleSelection)
            return KI18n.i18n("Choose at least one valid key to encrypt the item.");
        return "";
    }

    signal saveRequested(string draftJson)
    signal generatePasswordRequested
    signal windowCloseApproved
    signal vaultLockApproved
    signal vaultLockCancelled
    signal navigationCancelled

    visible: opened
    padding: 0

    function guardNavigation(action) {
        if (saving || savePending || waitingForEditor || externalModalOpen || preparingSecretMaterial || storeAdapter.controller.iconBusy)
            return;
        if (opened && dirty) {
            pendingNavigation = action;
            discardAction = "navigate";
            discardDialog.open();
        } else {
            if (opened)
                closeAndClear();
            action();
        }
    }

    function currentDraft() {
        return {
            id: entryId,
            folder: draftFolderId,
            title: titleField.text,
            content: {
                kind: fieldsHost.kind,
                fields: fieldsHost.currentFields()
            },
            icon: draftIcon,
            recipients: recipientChooser.selectedFingerprints.slice(),
            relatedEntryId: relatedEntryId,
            revision: entryId ? revision : null
        };
    }

    function updateDirty() {
        dirty = loaded && !draftBaseline.matches(currentDraft());
        if (saveError)
            saveError = "";
        if (invalidField)
            invalidField = "";
    }

    function requestEntry(id) {
        requestedId = id;
        waitingForEditor = true;
        storeAdapter.controller.openEntry(id);
    }

    function openEntry(entry) {
        const nextId = entry ? entry.id : "";
        if (saving || savePending || externalModalOpen || preparingSecretMaterial || storeAdapter.controller.iconBusy)
            return;
        if (waitingForEditor) {
            requestEntry(nextId);
            return;
        }
        if (storeAdapter.controller.editorBusy)
            return;
        if (opened && loaded && nextId === entryId)
            return;
        if (opened && dirty) {
            pendingEntryId = nextId;
            discardAction = "switch";
            discardDialog.open();
            return;
        }
        if (opened)
            closeAndClear();
        requestEntry(nextId);
    }

    function openRelatedEntry(entry) {
        if (!entry || entry.trashed || entry.relatedEntryId || storeAdapter.relatedEntryFor(entry) || storeAdapter.controller.locked || saving || savePending || waitingForEditor || externalModalOpen || preparingSecretMaterial || storeAdapter.controller.busy || storeAdapter.controller.editorBusy || storeAdapter.controller.iconBusy)
            return;
        const source = {
            id: entry.id,
            title: entry.title || "",
            folder: entry.folder || "",
            recipients: Array.isArray(entry.recipients) ? entry.recipients.slice() : []
        };
        guardNavigation(function () {
            editor.pendingRelatedEntry = source;
            editor.openEntry(null);
            if (!editor.waitingForEditor && !editor.opened)
                editor.pendingRelatedEntry = null;
        });
    }

    function acceptEditorReady() {
        if (!waitingForEditor)
            return;
        let draft;
        try {
            draft = JSON.parse(storeAdapter.controller.editorJson);
        } catch (error) {
            waitingForEditor = false;
            pendingRelatedEntry = null;
            return;
        }
        const resultId = draft.id || "";
        if (resultId !== requestedId)
            return;

        const kindIndex = kindField.model.findIndex(option => option.id === draft.content?.kind);
        if (kindIndex < 0) {
            waitingForEditor = false;
            pendingRelatedEntry = null;
            saveError = KI18n.i18n("This item type cannot be edited.");
            return;
        }
        const relatedPrefill = !resultId ? pendingRelatedEntry : null;
        pendingRelatedEntry = null;
        loaded = false;
        invalidField = "";
        entryId = resultId;
        draftFolderId = draft.folder || "";
        relatedEntryId = draft.relatedEntryId || "";
        revision = draft.revision === undefined ? null : draft.revision;
        titleField.text = draft.title || "";
        draftIcon = draft.icon?.kind === "favicon" ? null : draft.icon || null;
        draftIconSource = draft.iconSource || "";
        kindField.currentIndex = kindIndex;
        fieldsHost.loadContent(draft.content);
        recipientChooser.selectedFingerprints = Array.isArray(draft.recipients) ? draft.recipients.slice() : [];
        // New items start with the vault's primary key unless the folder already set recipients.
        const primary = (storeAdapter.controller.primaryKey || "").toUpperCase();
        if (!resultId && recipientChooser.selectedFingerprints.length === 0 && keyStore.entries.some(key => key.id.toUpperCase() === primary && key.statusCode === "valid" && key.canEncrypt))
            recipientChooser.selectedFingerprints = [primary];

        if (relatedPrefill) {
            const customIndex = kindField.model.findIndex(option => option.id === "custom");
            if (customIndex < 0) {
                waitingForEditor = false;
                saveError = KI18n.i18n("The linked item cannot be created.");
                return;
            }
            titleField.text = KI18n.i18nc("@label default title %1 is the original item", "Item linked to %1", relatedPrefill.title);
            draftFolderId = relatedPrefill.folder;
            kindField.currentIndex = customIndex;
            fieldsHost.loadContent({
                kind: "custom",
                fields: {}
            });
            recipientChooser.selectedFingerprints = relatedPrefill.recipients.slice();
            relatedEntryId = relatedPrefill.id;
        }

        waitingForEditor = false;
        loaded = true;
        draftBaseline.capture(currentDraft());
        dirty = false;
        opened = true;
        const flickable = editorScroll.contentItem as Flickable;
        if (flickable)
            flickable.contentY = 0;
        Qt.callLater(titleField.forceActiveFocus);
        Qt.callLater(ensureFocusVisible);
    }

    function submit() {
        if (!canSave)
            return;
        savePending = true;
        saveError = "";
        invalidField = "";
        saveRequested(JSON.stringify(currentDraft()));
    }

    function applyGeneratedPassword(value) {
        if (opened && loaded)
            fieldsHost.applyGeneratedPassword(value);
    }

    function requestClose() {
        if (saving || storeAdapter.controller.editorBusy || preparingSecretMaterial)
            return;
        if (dirty) {
            discardAction = "close";
            discardDialog.open();
        } else {
            closeAndClear();
        }
    }

    function requestWindowClose() {
        if (saving || storeAdapter.controller.editorBusy || preparingSecretMaterial)
            return;
        if (dirty) {
            discardAction = "window";
            discardDialog.open();
        } else {
            closeAndClear();
            windowCloseApproved();
        }
    }

    function requestVaultLock() {
        if (saving || savePending || waitingForEditor || storeAdapter.controller.editorBusy || preparingSecretMaterial)
            return;
        if (dirty) {
            discardAction = "lock";
            discardDialog.open();
        } else {
            closeAndClear();
            vaultLockApproved();
        }
    }

    function acceptDiscard() {
        const action = discardAction;
        const nextId = pendingEntryId;
        const navigation = pendingNavigation;
        pendingNavigation = null;
        discardAction = "";
        pendingEntryId = "";
        closeAndClear();
        if (action === "switch")
            requestEntry(nextId);
        else if (action === "window")
            windowCloseApproved();
        else if (action === "lock")
            vaultLockApproved();
        else if (action === "navigate" && navigation)
            navigation();
    }

    function closeAndClear() {
        relatedEntryPicker.closePicker();
        savePending = false;
        preparingSecretMaterial = false;
        waitingForEditor = false;
        loaded = false;
        dirty = false;
        draftBaseline.clear();
        requestedId = "";
        entryId = "";
        relatedEntryId = "";
        pendingRelatedEntry = null;
        draftFolderId = "";
        revision = null;
        saveError = "";
        invalidField = "";
        titleField.text = "";
        draftIcon = null;
        draftIconSource = "";
        fieldsHost.clearFields();
        recipientChooser.selectedFingerprints = [];
        opened = false;
        pendingNavigation = null;
        storeAdapter.controller.clearEditor();
    }

    function ensureFocusVisible() {
        const flickable = editorScroll.contentItem as Flickable;
        const item = focusedItem;
        if (!item || !flickable)
            return;
        let ancestor = item;
        while (ancestor && ancestor !== editorScroll)
            ancestor = ancestor.parent;
        if (!ancestor)
            return;
        const position = item.mapToItem(flickable.contentItem, 0, 0);
        const top = position.y;
        const bottom = top + item.height;
        if (top < flickable.contentY)
            flickable.contentY = top;
        else if (bottom > flickable.contentY + flickable.height)
            flickable.contentY = Math.max(0, bottom - flickable.height);
    }

    onFocusedItemChanged: {
        if (opened)
            Qt.callLater(ensureFocusVisible);
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        context: Qt.WindowShortcut
        enabled: editor.opened && !editor.externalModalOpen && !discardDialog.opened && !iconWindow.opened && !iconMenu.opened && !folderField.menuOpen
        onActivated: editor.requestClose()
    }

    Shortcut {
        sequences: [StandardKey.Save]
        context: Qt.WindowShortcut
        enabled: editor.opened && editor.canSave && !editor.externalModalOpen && !discardDialog.opened && !iconWindow.opened && !iconMenu.opened
        onActivated: editor.submit()
    }

    Shortcut {
        sequences: ["Ctrl+Return", "Ctrl+Enter"]
        context: Qt.WindowShortcut
        enabled: editor.opened && editor.canSave && !editor.externalModalOpen && !discardDialog.opened && !iconWindow.opened && !iconMenu.opened
        onActivated: editor.submit()
    }

    Dialogs.FileDialog {
        id: iconFileDialog
        title: KI18n.i18nc("@title:window", "Choose Item Image")
        nameFilters: [KI18n.i18n("Images (*.png *.jpg *.jpeg *.webp)")]
        onAccepted: {
            editor.externalModalOpen = false;
            editor.storeAdapter.controller.prepareEntryIcon(selectedFile);
        }
        onRejected: editor.externalModalOpen = false
    }

    EntryIconWindow {
        id: iconWindow
        onIconChosen: iconName => {
            editor.draftIcon = {
                kind: "theme",
                name: iconName
            };
            editor.draftIconSource = "";
            editor.updateDirty();
        }
        onOpenedChanged: editor.externalModalOpen = opened
    }

    Connections {
        target: editor.storeAdapter.controller

        function onIconPrepared(json) {
            if (!editor.opened || !editor.loaded)
                return;
            const result = JSON.parse(json);
            editor.draftIcon = result.icon;
            editor.draftIconSource = result.iconSource;
            editor.updateDirty();
            editor.saveError = "";
        }

        function onSshKeyPrepared(json) {
            editor.preparingSecretMaterial = false;
            if (!editor.opened || !editor.loaded || fieldsHost.kind !== "ssh-key")
                return;
            fieldsHost.applyPreparedSshKey(json);
            editor.updateDirty();
            editor.saveError = "";
        }

        function onTotpQrPrepared(json) {
            editor.preparingSecretMaterial = false;
            if (!editor.opened || !editor.loaded || fieldsHost.kind !== "totp")
                return;
            fieldsHost.applyPreparedTotp(json);
            editor.updateDirty();
            editor.saveError = "";
        }

        function onEditorReady() {
            editor.acceptEditorReady();
        }

        function onEntrySaved(id) {
            if (!editor.savePending || (editor.entryId && id !== editor.entryId))
                return;
            editor.savePending = false;
            editor.closeAndClear();
        }

        function onOperationFailed() {
            editor.preparingSecretMaterial = false;
            if (editor.opened && !editor.savePending && !editor.waitingForEditor)
                editor.saveError = editor.storeAdapter.controller.errorMessage;
            if (editor.savePending && !editor.storeAdapter.controller.saving) {
                editor.savePending = false;
                editor.invalidField = editor.storeAdapter.controller.editorErrorField || "";
                editor.saveError = editor.storeAdapter.controller.errorMessage || KI18n.i18n("The item could not be saved.");
                if (editor.invalidField)
                    Qt.callLater(fieldsHost.focusInvalidField);
            } else if (editor.waitingForEditor && !editor.storeAdapter.controller.editorBusy) {
                editor.waitingForEditor = false;
                editor.pendingRelatedEntry = null;
            }
        }
    }

    contentItem: ColumnLayout {
        spacing: 0

        PanelHeader {
            Layout.fillWidth: true
            headerHeight: editor.headerHeight
            contentItem: RowLayout {
                Kirigami.Heading {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    level: 3
                    text: editor.title
                    elide: Text.ElideRight
                }
                Controls.ToolButton {
                    text: KI18n.i18nc("@action:button", "Close Editor")
                    icon.name: "window-close"
                    display: Controls.AbstractButton.IconOnly
                    enabled: !editor.saving && !editor.savePending
                    onClicked: editor.requestClose()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
            }
        }

        Controls.ScrollView {
            id: editorScroll
            objectName: "entryEditorScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            background: null

            Controls.Pane {
                id: formPane
                width: editorScroll.availableWidth
                padding: 20
                background: null

                ColumnLayout {
                    id: editorForm
                    width: formPane.availableWidth
                    spacing: 24

                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        visible: !!editor.saveError
                        type: Kirigami.MessageType.Error
                        icon.name: "dialog-warning"
                        icon.color: Kirigami.Theme.negativeTextColor
                        text: editor.saveError
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        spacing: 16
                        GridLayout {
                            Layout.fillWidth: true
                            columns: editorForm.width < 360 ? 1 : 2
                            columnSpacing: 16
                            rowSpacing: 16
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 6
                                Controls.Label {
                                    text: KI18n.i18nc("@label item type", "Type")
                                }
                                Controls.ComboBox {
                                    id: kindField
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    model: editor.storeAdapter.kindOptions
                                    textRole: "name"
                                    valueRole: "id"
                                    Accessible.name: KI18n.i18n("Item type")
                                    Accessible.description: editor.entryId ? KI18n.i18n("The type cannot be changed when editing an existing item.") : KI18n.i18n("Change the type to pick another form before entering the data.")
                                    enabled: !editor.entryId && !editor.preparingSecretMaterial
                                    onActivated: {
                                        if (!editor.entryId && !editor.preparingSecretMaterial)
                                            fieldsHost.selectType(currentValue);
                                    }
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 6
                                Controls.Label {
                                    text: KI18n.i18nc("@label", "Folder")
                                }
                                FolderPicker {
                                    id: folderField
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    folderRows: editor.storeAdapter.folderRows
                                    selectedId: editor.draftFolderId
                                    accessibleLabel: KI18n.i18n("Item folder")
                                    onFolderSelected: function (folderId) {
                                        editor.draftFolderId = folderId;
                                        editor.updateDirty();
                                    }
                                }
                            }
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 12
                            Controls.Button {
                                id: iconButton
                                objectName: "entryIconButton"
                                Layout.alignment: Qt.AlignBottom
                                Layout.preferredWidth: titleField.implicitHeight
                                Layout.preferredHeight: titleField.implicitHeight
                                padding: 4
                                text: KI18n.i18nc("@action:button", "Change Item Icon")
                                display: Controls.AbstractButton.IconOnly
                                focusPolicy: Qt.ClickFocus
                                activeFocusOnTab: false
                                Accessible.name: text
                                Accessible.description: editor.iconStateText
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: KI18n.i18nc("@label two parts of a subtitle", "%1 · %2", text, editor.iconStateText)
                                onClicked: iconMenu.open()
                                contentItem: EntryIcon {
                                    entryIcon: editor.draftIcon
                                    imageSource: editor.draftIconSource
                                    fallbackIcon: editor.storeAdapter.kindIcon(kindField.currentValue || "login")
                                    iconSize: Kirigami.Units.iconSizes.medium
                                }
                                Controls.Menu {
                                    id: iconMenu
                                    y: iconButton.height
                                    Controls.MenuItem {
                                        text: KI18n.i18nc("@action:inmenu", "Use Automatic Favicon")
                                        onTriggered: {
                                            editor.draftIcon = null;
                                            editor.draftIconSource = "";
                                            editor.updateDirty();
                                        }
                                    }
                                    Controls.MenuItem {
                                        text: KI18n.i18nc("@action:inmenu", "Choose Image…")
                                        onTriggered: {
                                            editor.externalModalOpen = true;
                                            iconFileDialog.open();
                                        }
                                    }
                                    Controls.MenuItem {
                                        text: KI18n.i18nc("@action:inmenu", "Choose Theme Icon…")
                                        onTriggered: iconWindow.openWith(editor.draftIcon?.kind === "theme" ? editor.draftIcon.name : "", titleField.text.trim(), kindField.currentText, editor.storeAdapter.kindIcon(kindField.currentValue || "login"))
                                    }
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 6
                                Controls.Label {
                                    text: KI18n.i18nc("@label:textbox required field", "Name *")
                                }
                                Controls.TextField {
                                    id: titleField
                                    objectName: "entryNameField"
                                    Layout.fillWidth: true
                                    Accessible.name: KI18n.i18n("Item name")
                                    Accessible.description: KI18n.i18n("Required. Identifies the item in the list.")
                                    placeholderText: KI18n.i18nc("@info:placeholder", "For example: Personal email")
                                    onTextEdited: editor.updateDirty()
                                }
                            }
                        }
                    }

                    SecretFieldsHost {
                        id: fieldsHost
                        Layout.fillWidth: true
                        invalidField: editor.invalidField
                        enabled: !editor.saving && !editor.savePending && !editor.preparingSecretMaterial
                        onEdited: editor.updateDirty()
                        onGeneratePasswordRequested: editor.generatePasswordRequested()
                        onExternalModalRequested: function (opened) {
                            editor.externalModalOpen = opened;
                        }
                        onPrepareSshKeyRequested: function (fileUrl, passphrase) {
                            editor.preparingSecretMaterial = true;
                            editor.storeAdapter.controller.prepareSshKey(fileUrl, passphrase);
                        }
                        onPrepareTotpQrRequested: function (fileUrl) {
                            editor.preparingSecretMaterial = true;
                            editor.storeAdapter.controller.prepareTotpQr(fileUrl);
                        }
                    }

                    RecipientsChooser {
                        id: recipientChooser
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        keyStore: editor.keyStore
                        onSelectionChanged: function (fingerprints) {
                            editor.updateDirty();
                        }
                    }

                    RelatedEntryPicker {
                        id: relatedEntryPicker
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        entries: editor.storeAdapter.entries
                        kindOptions: editor.storeAdapter.kindOptions
                        folders: editor.storeAdapter.folders
                        folderRows: editor.storeAdapter.folderRows
                        entryId: editor.entryId
                        relatedEntryId: editor.relatedEntryId
                        onExternalModalRequested: function (opened) {
                            editor.externalModalOpen = opened;
                        }
                        onSelectionChanged: function (relatedId) {
                            editor.relatedEntryId = relatedId;
                            editor.updateDirty();
                        }
                    }
                }
            }
        }

        Controls.Pane {
            Layout.fillWidth: true
            leftPadding: 20
            rightPadding: 20
            topPadding: 12
            bottomPadding: 12
            background: null
            contentItem: RowLayout {
                spacing: 8
                Controls.Label {
                    Layout.fillWidth: true
                    visible: editor.opened && editor.loaded && editor.saveBlockReason.length > 0
                    text: editor.saveBlockReason
                    horizontalAlignment: Text.AlignRight
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Item {
                    Layout.fillWidth: true
                    visible: !editor.opened || !editor.loaded || editor.saveBlockReason.length === 0
                }
                Controls.BusyIndicator {
                    visible: editor.saving
                    running: visible
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: width
                    Accessible.name: KI18n.i18n("Saving item")
                }
                Controls.Button {
                    text: KI18n.i18nc("@action:button", "Cancel")
                    enabled: !editor.saving && !editor.savePending && !editor.storeAdapter.controller.editorBusy
                    onClicked: editor.requestClose()
                }
                Controls.Button {
                    objectName: "entrySaveButton"
                    text: editor.saving ? KI18n.i18nc("@info:progress", "Saving…") : editor.entryId ? KI18n.i18nc("@action:button", "Save Changes") : KI18n.i18nc("@action:button", "Create Item")
                    icon.name: "document-save"
                    highlighted: true
                    enabled: editor.canSave
                    Accessible.description: editor.saveBlockReason
                    onClicked: editor.submit()
                }
            }
        }
    }

    Controls.Dialog {
        id: discardDialog
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: KI18n.i18nc("@title:window", "Discard Changes")
        standardButtons: Controls.Dialog.Discard | Controls.Dialog.Cancel
        onDiscarded: {
            discardDialog.close();
            editor.acceptDiscard();
        }
        onRejected: {
            if (editor.discardAction === "navigate")
                editor.navigationCancelled();
            if (editor.discardAction === "lock")
                editor.vaultLockCancelled();
            editor.discardAction = "";
            editor.pendingEntryId = "";
            editor.pendingNavigation = null;
        }
        Controls.Label {
            width: Math.min(editor.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 28)
            text: KI18n.i18n("There are unsaved changes. If you continue, they will be discarded.")
            wrapMode: Text.Wrap
        }
    }
}
