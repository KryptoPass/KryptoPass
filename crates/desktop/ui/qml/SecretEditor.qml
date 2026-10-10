pragma ComponentBehavior: Bound
import QtQuick
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
    property string draftFolderId: ""
    property var revision: null
    property bool loaded: false
    property bool waitingForEditor: false
    property bool savePending: false
    property bool dirty: false
    property bool externalModalOpen: false
    property string baselineJson: ""
    property string requestedId: ""
    property string pendingEntryId: ""
    property string discardAction: ""
    property string saveError: ""
    property bool opened: false
    property var pendingNavigation: null
    readonly property string title: entryId ? qsTr("Editar elemento") : qsTr("Nuevo elemento")

    readonly property bool saving: storeAdapter.controller.saving
    readonly property bool canSave: loaded && !saving && !savePending && !storeAdapter.controller.busy && !storeAdapter.controller.editorBusy && titleField.text.trim().length > 0 && recipientChooser.eligibleSelection && (!entryId || dirty)

    signal saveRequested(string draftJson)
    signal generatePasswordRequested()
    signal windowCloseApproved()
    signal vaultLockApproved()
    signal vaultLockCancelled()
    signal navigationCancelled()

    visible: opened
    padding: 0

    function guardNavigation(action) {
        if (saving || savePending || waitingForEditor || externalModalOpen)
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
        const isNote = kindField.currentValue === "note";
        return {
            id: entryId,
            folder: draftFolderId,
            title: titleField.text,
            kind: kindField.currentValue || "login",
            username: isNote ? "" : loginFields.username,
            password: isNote ? "" : loginFields.password,
            url: isNote ? "" : loginFields.url,
            notes: isNote ? noteFields.notes : loginFields.notes,
            value: isNote ? noteFields.value : "",
            recipients: recipientChooser.selectedFingerprints.slice(),
            revision: entryId ? revision : null
        };
    }

    function updateDirty() {
        dirty = loaded && baselineJson !== JSON.stringify(currentDraft());
    }

    function requestEntry(id) {
        requestedId = id;
        waitingForEditor = true;
        storeAdapter.controller.openEntry(id);
    }

    function openEntry(entry) {
        const nextId = entry ? entry.id : "";
        if (saving || savePending || externalModalOpen)
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

    function acceptEditorReady() {
        if (!waitingForEditor)
            return;
        let draft;
        try {
            draft = JSON.parse(storeAdapter.controller.editorJson);
        } catch (error) {
            waitingForEditor = false;
            return;
        }
        const resultId = draft.id || "";
        if (resultId !== requestedId)
            return;

        entryId = resultId;
        draftFolderId = draft.folder || "";
        revision = draft.revision === undefined ? null : draft.revision;
        titleField.text = draft.title || "";
        kindField.currentIndex = Math.max(0, kindField.model.findIndex(option => option.id === (draft.kind === "note" ? "note" : "login")));
        loginFields.username = draft.username || "";
        loginFields.password = draft.password || "";
        loginFields.url = draft.url || "";
        loginFields.notes = draft.kind === "note" ? "" : draft.notes || "";
        loginFields.passwordVisible = false;
        noteFields.value = draft.kind === "note" ? draft.value || "" : "";
        noteFields.notes = draft.kind === "note" ? draft.notes || "" : "";
        recipientChooser.selectedFingerprints = Array.isArray(draft.recipients) ? draft.recipients.slice() : [];

        waitingForEditor = false;
        loaded = true;
        baselineJson = JSON.stringify(currentDraft());
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
        if (!opened || !loaded || kindField.currentValue !== "login")
            return;
        loginFields.password = value;
        updateDirty();
    }

    function requestClose() {
        if (saving || storeAdapter.controller.editorBusy)
            return;
        if (dirty) {
            discardAction = "close";
            discardDialog.open();
        } else {
            closeAndClear();
        }
    }

    function requestWindowClose() {
        if (saving || storeAdapter.controller.editorBusy)
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
        if (saving || savePending || waitingForEditor || storeAdapter.controller.editorBusy)
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
        savePending = false;
        waitingForEditor = false;
        loaded = false;
        dirty = false;
        baselineJson = "";
        requestedId = "";
        entryId = "";
        draftFolderId = "";
        revision = null;
        saveError = "";
        titleField.text = "";
        loginFields.username = "";
        loginFields.password = "";
        loginFields.url = "";
        loginFields.notes = "";
        loginFields.passwordVisible = false;
        noteFields.value = "";
        noteFields.notes = "";
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
        enabled: editor.opened && !editor.externalModalOpen && !discardDialog.opened && !folderField.menuOpen
        onActivated: editor.requestClose()
    }

    Shortcut {
        sequences: [StandardKey.Save]
        context: Qt.WindowShortcut
        enabled: editor.opened && editor.canSave && !editor.externalModalOpen && !discardDialog.opened
        onActivated: editor.submit()
    }

    Connections {
        target: editor.storeAdapter.controller

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
            if (editor.savePending && !editor.storeAdapter.controller.saving) {
                editor.savePending = false;
                editor.saveError = editor.storeAdapter.controller.errorMessage || qsTr("No se pudo guardar el elemento.");
            } else if (editor.waitingForEditor && !editor.storeAdapter.controller.editorBusy) {
                editor.waitingForEditor = false;
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
                                    onActivated: editor.updateDirty()
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

                    ColumnLayout {
                        Layout.fillWidth: true
                        enabled: !editor.saving && !editor.savePending
                        spacing: 0
                        LoginFields {
                            id: loginFields
                            Layout.fillWidth: true
                            visible: kindField.currentValue !== "note"
                            onUsernameChanged: editor.updateDirty()
                            onPasswordChanged: editor.updateDirty()
                            onUrlChanged: editor.updateDirty()
                            onNotesChanged: editor.updateDirty()
                            onGeneratePasswordRequested: editor.generatePasswordRequested()
                        }
                        NoteFields {
                            id: noteFields
                            Layout.fillWidth: true
                            visible: kindField.currentValue === "note"
                            onValueChanged: editor.updateDirty()
                            onNotesChanged: editor.updateDirty()
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
