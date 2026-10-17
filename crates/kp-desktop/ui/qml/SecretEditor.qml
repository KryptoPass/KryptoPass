pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop
import QtQuick.Dialogs as Dialogs
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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
    DraftBaseline { id: draftBaseline }
    property string requestedId: ""
    property string pendingEntryId: ""
    property string discardAction: ""
    property string saveError: ""
    property var pendingRelatedEntry: null
    property bool opened: false
    property var pendingNavigation: null
    readonly property string title: entryId ? qsTr("Editar elemento") : qsTr("Nuevo elemento")

    readonly property bool saving: storeAdapter.controller.saving
    readonly property bool canSave: loaded && fieldsHost.ready && !saving && !savePending && !preparingSecretMaterial && !storeAdapter.controller.busy && !storeAdapter.controller.editorBusy && !storeAdapter.controller.iconBusy && titleField.text.trim().length > 0 && recipientChooser.eligibleSelection && (!entryId || dirty)

    signal saveRequested(string draftJson)
    signal generatePasswordRequested()
    signal windowCloseApproved()
    signal vaultLockApproved()
    signal vaultLockCancelled()
    signal navigationCancelled()

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
            content: { kind: fieldsHost.kind, fields: fieldsHost.currentFields() },
            icon: draftIcon,
            recipients: recipientChooser.selectedFingerprints.slice(),
            relatedEntryId: relatedEntryId,
            revision: entryId ? revision : null
        };
    }

    function updateDirty() {
        dirty = loaded && !draftBaseline.matches(currentDraft());
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
        if (!entry || entry.trashed || entry.relatedEntryId || storeAdapter.relatedEntryFor(entry)
                || storeAdapter.controller.locked || saving || savePending || waitingForEditor
                || externalModalOpen || preparingSecretMaterial || storeAdapter.controller.busy
                || storeAdapter.controller.editorBusy || storeAdapter.controller.iconBusy)
            return;
        const source = {
            id: entry.id,
            title: entry.title || "",
            folder: entry.folder || "",
            recipients: Array.isArray(entry.recipients) ? entry.recipients.slice() : []
        };
        guardNavigation(function() {
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
            saveError = qsTr("No se puede editar este tipo de elemento.");
            return;
        }
        const relatedPrefill = !resultId ? pendingRelatedEntry : null;
        pendingRelatedEntry = null;
        loaded = false;
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

        if (relatedPrefill) {
            const customIndex = kindField.model.findIndex(option => option.id === "custom");
            if (customIndex < 0) {
                waitingForEditor = false;
                saveError = qsTr("No se puede crear el elemento de seguridad.");
                return;
            }
            titleField.text = qsTr("Seguridad de %1").arg(relatedPrefill.title);
            draftFolderId = relatedPrefill.folder;
            kindField.currentIndex = customIndex;
            fieldsHost.loadContent({ kind: "custom", fields: {} });
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
        enabled: editor.opened && !editor.externalModalOpen && !discardDialog.opened && !themeIconDialog.opened && !iconMenu.opened && !folderField.menuOpen
        onActivated: editor.requestClose()
    }

    Shortcut {
        sequences: [StandardKey.Save]
        context: Qt.WindowShortcut
        enabled: editor.opened && editor.canSave && !editor.externalModalOpen && !discardDialog.opened && !themeIconDialog.opened && !iconMenu.opened
        onActivated: editor.submit()
    }

    Dialogs.FileDialog {
        id: iconFileDialog
        title: qsTr("Elegir imagen del elemento")
        nameFilters: [qsTr("Imágenes (*.png *.jpg *.jpeg *.webp)")]
        onAccepted: {
            editor.externalModalOpen = false;
            editor.storeAdapter.controller.prepareEntryIcon(selectedFile);
        }
        onRejected: editor.externalModalOpen = false
    }

    Controls.Dialog {
        id: themeIconDialog
        title: qsTr("Icono del tema")
        modal: true
        anchors.centerIn: Controls.Overlay.overlay
        parent: Controls.Overlay.overlay
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onOpened: {
            themeIconName.text = editor.draftIcon?.kind === "theme" ? editor.draftIcon.name : "internet-web-browser";
            themeIconName.forceActiveFocus();
        }
        onAccepted: {
            if (!themeIconName.acceptableInput) return;
            editor.draftIcon = { kind: "theme", name: themeIconName.text };
            editor.draftIconSource = "";
            editor.updateDirty();
        }
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            EntryIcon {
                Layout.alignment: Qt.AlignHCenter
                iconSize: Kirigami.Units.iconSizes.large
                entryIcon: ({ kind: "theme", name: themeIconName.text })
                fallbackIcon: editor.storeAdapter.kindIcon(kindField.currentValue || "login")
            }
            GridLayout {
                columns: 4
                Repeater {
                    model: [
                        { name: qsTr("Internet"), icon: "internet-web-browser" },
                        { name: qsTr("Correo"), icon: "mail-message" },
                        { name: qsTr("Trabajo"), icon: "applications-office" },
                        { name: qsTr("Personal"), icon: "user-identity" },
                        { name: qsTr("Servidor"), icon: "network-server" },
                        { name: qsTr("Billetera"), icon: "wallet-open" },
                        { name: qsTr("Desarrollo"), icon: "utilities-terminal" },
                        { name: qsTr("Juegos"), icon: "applications-games" }
                    ]
                    delegate: Controls.ToolButton {
                        required property var modelData
                        icon.name: modelData.icon
                        display: Controls.AbstractButton.IconOnly
                        text: modelData.name
                        checkable: true
                        checked: themeIconName.text === modelData.icon
                        focusPolicy: Qt.StrongFocus
                        Accessible.name: modelData.name
                        Accessible.description: checked ? qsTr("Icono seleccionado") : qsTr("Seleccionar icono")
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 3
                        Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                        onClicked: themeIconName.text = modelData.icon
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
            }
            Controls.TextField {
                id: themeIconName
                Layout.fillWidth: true
                maximumLength: 128
                validator: RegularExpressionValidator { regularExpression: /[a-zA-Z0-9-]{1,128}/ }
                Accessible.name: qsTr("Nombre del icono del tema")
                placeholderText: "internet-web-browser"
            }
        }
        Binding {
            target: themeIconDialog.standardButton(Controls.Dialog.Ok)
            property: "enabled"
            value: themeIconName.acceptableInput
        }
    }

    Connections {
        target: editor.storeAdapter.controller

        function onIconPrepared(json) {
            if (!editor.opened || !editor.loaded) return;
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
                editor.saveError = editor.storeAdapter.controller.errorMessage || qsTr("No se pudo guardar el elemento.");
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
                    text: qsTr("Cerrar editor")
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
                        text: editor.saveError
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        spacing: 16
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            Controls.Label { text: qsTr("Nombre") }
                            Controls.TextField {
                                id: titleField
                                objectName: "entryNameField"
                                Layout.fillWidth: true
                                Accessible.name: qsTr("Nombre del elemento")
                                placeholderText: qsTr("Ej. Correo personal")
                                onTextEdited: editor.updateDirty()
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label { text: qsTr("Icono") }
                            RowLayout {
                                Layout.fillWidth: true
                                EntryIcon {
                                    entryIcon: editor.draftIcon
                                    imageSource: editor.draftIconSource
                                    fallbackIcon: editor.storeAdapter.kindIcon(kindField.currentValue || "login")
                                    Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                                    Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: editor.storeAdapter.controller.iconBusy ? qsTr("Preparando imagen…") : editor.draftIcon?.kind === "image" ? qsTr("Imagen personalizada") : editor.draftIcon?.kind === "theme" ? editor.draftIcon.name : qsTr("Automático")
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                }
                                Controls.Button {
                                    text: qsTr("Cambiar…")
                                    Accessible.name: qsTr("Cambiar icono del elemento")
                                    enabled: !editor.storeAdapter.controller.iconBusy
                                    onClicked: iconMenu.open()
                                    Controls.Menu {
                                        id: iconMenu
                                        Controls.MenuItem {
                                            text: qsTr("Usar favicon automático")
                                            onTriggered: {
                                                editor.draftIcon = null;
                                                editor.draftIconSource = "";
                                                editor.updateDirty();
                                            }
                                        }
                                        Controls.MenuItem {
                                            text: qsTr("Elegir imagen…")
                                            onTriggered: { editor.externalModalOpen = true; iconFileDialog.open(); }
                                        }
                                        Controls.MenuItem {
                                            text: qsTr("Elegir icono del tema…")
                                            onTriggered: themeIconDialog.open()
                                        }
                                    }
                                }
                            }
                        }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: editorForm.width < 360 ? 1 : 2
                            columnSpacing: 16
                            rowSpacing: 16
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 6
                                Controls.Label { text: qsTr("Tipo") }
                                Controls.ComboBox {
                                    id: kindField
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    model: editor.storeAdapter.kindOptions
                                    textRole: "name"
                                    valueRole: "id"
                                    Accessible.name: qsTr("Tipo de elemento")
                                    enabled: !editor.entryId && !editor.preparingSecretMaterial
                                    onActivated: { if (!editor.entryId && !editor.preparingSecretMaterial) fieldsHost.selectType(currentValue); }
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 6
                                Controls.Label { text: qsTr("Carpeta") }
                                FolderPicker {
                                    id: folderField
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    folderRows: editor.storeAdapter.folderRows
                                    selectedId: editor.draftFolderId
                                    accessibleLabel: qsTr("Carpeta del elemento")
                                    onFolderSelected: function(folderId) {
                                        editor.draftFolderId = folderId;
                                        editor.updateDirty();
                                    }
                                }
                            }
                        }
                    }

                    SecretFieldsHost {
                        id: fieldsHost
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending && !editor.preparingSecretMaterial
                        onEdited: editor.updateDirty()
                        onGeneratePasswordRequested: editor.generatePasswordRequested()
                        onExternalModalRequested: function(opened) { editor.externalModalOpen = opened; }
                        onPrepareSshKeyRequested: function(fileUrl, passphrase) {
                            editor.preparingSecretMaterial = true;
                            editor.storeAdapter.controller.prepareSshKey(fileUrl, passphrase);
                        }
                        onPrepareTotpQrRequested: function(fileUrl) {
                            editor.preparingSecretMaterial = true;
                            editor.storeAdapter.controller.prepareTotpQr(fileUrl);
                        }
                    }

                    RecipientsChooser {
                        id: recipientChooser
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        keyStore: editor.keyStore
                        onSelectionChanged: function(fingerprints) {
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
                        onExternalModalRequested: function(opened) { editor.externalModalOpen = opened; }
                        onSelectionChanged: function(relatedId) {
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
                Item { Layout.fillWidth: true }
                Controls.BusyIndicator {
                    visible: editor.saving
                    running: visible
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: width
                    Accessible.name: qsTr("Guardando elemento")
                }
                Controls.Button {
                    text: qsTr("Cancelar")
                    enabled: !editor.saving && !editor.savePending && !editor.storeAdapter.controller.editorBusy
                    onClicked: editor.requestClose()
                }
                Controls.Button {
                    objectName: "entrySaveButton"
                    text: editor.saving ? qsTr("Guardando…") : editor.entryId ? qsTr("Guardar cambios") : qsTr("Crear elemento")
                    icon.name: "document-save"
                    highlighted: true
                    enabled: editor.canSave
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
        title: qsTr("Descartar cambios")
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
            text: qsTr("Hay cambios sin guardar. Si continúas, se descartarán.")
            wrapMode: Text.Wrap
        }
    }
}
