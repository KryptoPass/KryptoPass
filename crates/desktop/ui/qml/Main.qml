pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: root
    width: 1280
    height: 800
    minimumWidth: 900
    minimumHeight: 580
    visible: true
    title: qsTr("KryptoPass — Demostración")
    LayoutMirroring.enabled: Qt.locale().textDirection === Qt.RightToLeft
    LayoutMirroring.childrenInherit: true
    MockStore {
        id: store
        objectName: "mockStore"
    }
    property bool revealed: false
    readonly property string scopeTitle: store.scope === "all" ? qsTr("Todos los elementos") : store.scope === "favorites" ? qsTr("Favoritos") : store.scope === "trash" ? qsTr("Papelera") : store.folderName(store.scope)
    Connections {
        target: store
        function onSelectedIdChanged() {
            root.revealed = false;
        }
        function onQueryChanged() {
            if (search.text !== store.query)
                search.text = store.query;
        }
    }

    Controls.Action {
        id: newEntry
        text: qsTr("Nuevo elemento…")
        icon.name: "document-new"
        shortcut: StandardKey.New
        onTriggered: editor.openEntry(null)
    }
    Controls.Action {
        id: editEntry
        text: qsTr("Editar…")
        icon.name: "document-edit"
        enabled: !!store.selected && !store.selected.trashed
        shortcut: "F2"
        onTriggered: editor.openEntry(store.selected)
    }
    Controls.Action {
        id: trashEntry
        text: qsTr("Mover a la papelera")
        icon.name: "user-trash"
        enabled: !!store.selected && !store.selected.trashed
        shortcut: StandardKey.Delete
        onTriggered: {
            store.updateEntry(store.selectedId, {
                trashed: true
            });
            store.selectedId = store.visibleEntries.length ? store.visibleEntries[0].id : "";
        }
    }
    Controls.Action {
        id: newFolder
        text: qsTr("Nueva carpeta…")
        icon.name: "folder-new"
        shortcut: "Ctrl+Shift+N"
        onTriggered: folderEditor.openFolder(false)
    }
    menuBar: Controls.MenuBar {
        Controls.Menu {
            title: qsTr("&Archivo")
            Controls.MenuItem {
                action: newEntry
            }
            Controls.MenuItem {
                action: newFolder
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("&Salir")
                onTriggered: root.close()
            }
        }
        Controls.Menu {
            title: qsTr("&Editar")
            Controls.MenuItem {
                action: editEntry
            }
            Controls.MenuItem {
                text: qsTr("Mover a otra carpeta…")
                enabled: editEntry.enabled
                onTriggered: moveDialog.openMove()
            }
            Controls.MenuItem {
                action: trashEntry
            }
        }
        Controls.Menu {
            title: qsTr("&Ver")
            Controls.MenuItem {
                text: qsTr("Markdown experimental…")
                onTriggered: markdownDemo.open()
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Todos los elementos")
                onTriggered: store.selectScope("all")
            }
            Controls.MenuItem {
                text: qsTr("Favoritos")
                onTriggered: store.selectScope("favorites")
            }
            Controls.MenuItem {
                text: qsTr("Papelera")
                onTriggered: store.selectScope("trash")
            }
        }
        Controls.Menu {
            title: qsTr("A&yuda")
            Controls.MenuItem {
                text: qsTr("Acerca de KryptoPass")
                onTriggered: aboutDialog.open()
            }
        }
    }
    readonly property real panelHeaderHeight: Kirigami.Units.gridUnit * 2.5

    component PanelHeader: Controls.ToolBar {
        id: panelBar
        implicitHeight: root.panelHeaderHeight
        Kirigami.Theme.inherit: false
        Kirigami.Theme.colorSet: Kirigami.Theme.Header
        leftPadding: Kirigami.Units.largeSpacing
        rightPadding: Kirigami.Units.largeSpacing
        topPadding: Kirigami.Units.smallSpacing
        bottomPadding: Kirigami.Units.smallSpacing
        background: Rectangle {
            color: Kirigami.Theme.backgroundColor
            Kirigami.Separator {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: panelBar.position === Controls.ToolBar.Footer ? parent.top : undefined
                anchors.bottom: panelBar.position === Controls.ToolBar.Header ? parent.bottom : undefined
            }
        }
    }

    component PanelFooter: PanelHeader {
        position: Controls.ToolBar.Footer
    }

    component SplitterHandle: Kirigami.Separator {
        id: splitterHandle
        implicitWidth: 1
        implicitHeight: 1
        Kirigami.Theme.inherit: false
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        weight: Kirigami.Separator.Weight.Light
        containmentMask: Item {
            readonly property real margin: Kirigami.Units.largeSpacing
            x: -margin
            width: splitterHandle.width + margin * 2
            height: splitterHandle.height
        }
        HoverHandler {
            cursorShape: Qt.SplitHCursor
        }
    }

    component NavigationItem: Controls.ItemDelegate {
        id: navigationItem
        property string scopeId
        property int depth: 0
        leftPadding: Kirigami.Units.largeSpacing + depth * Kirigami.Units.largeSpacing
        rightPadding: Kirigami.Units.largeSpacing
        highlighted: store.scope === scopeId
        onClicked: store.selectScope(scopeId)
        contentItem: RowLayout {
            Kirigami.IconTitleSubtitle {
                Layout.fillWidth: true
                title: navigationItem.text
                icon: icon.fromControlsIcon(navigationItem.icon)
                selected: navigationItem.highlighted || navigationItem.down
                font: navigationItem.font
            }
            Controls.Label {
                text: store.count(navigationItem.scopeId)
                color: navigationItem.highlighted || navigationItem.down ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
            }
        }
    }

    Shortcut {
        sequences: [StandardKey.Find]
        onActivated: search.forceActiveFocus()
    }
    Shortcut {
        sequences: [StandardKey.Quit]
        onActivated: root.close()
    }

    Controls.SplitView {
        anchors.fill: parent
        orientation: Qt.Horizontal
        handle: SplitterHandle {}

        Controls.Pane {
            Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 14
            Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 11
            padding: 0
            Kirigami.Theme.inherit: false
            Kirigami.Theme.colorSet: Kirigami.Theme.View
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                PanelHeader {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 3
                            text: qsTr("Carpetas")
                        }
                        Controls.ToolButton {
                            action: newFolder
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: text
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
                }
                Repeater {
                    model: [
                        {
                            id: "all",
                            name: qsTr("Todos los elementos"),
                            icon: "view-list-details"
                        },
                        {
                            id: "favorites",
                            name: qsTr("Favoritos"),
                            icon: "rating"
                        },
                        {
                            id: "trash",
                            name: qsTr("Papelera"),
                            icon: "user-trash"
                        }
                    ]
                    delegate: NavigationItem {
                        required property var modelData
                        Layout.fillWidth: true
                        scopeId: modelData.id
                        text: modelData.name
                        icon.name: modelData.icon
                    }
                }
                Kirigami.ListSectionHeader {
                    Layout.fillWidth: true
                    text: qsTr("Mis carpetas")
                }
                Controls.ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ListView {
                        id: folderList
                        model: store.folderRows
                        activeFocusOnTab: true
                        keyNavigationWraps: true
                        Accessible.role: Accessible.List
                        delegate: NavigationItem {
                            required property var modelData
                            width: folderList.width
                            scopeId: modelData.id
                            depth: modelData.depth
                            text: modelData.name
                            icon.name: store.scope === modelData.id ? "folder-open" : "folder"
                            Keys.onReturnPressed: clicked()
                            Keys.onEnterPressed: clicked()
                        }
                    }
                }
                PanelFooter {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        Controls.ToolButton {
                            text: qsTr("Renombrar carpeta")
                            icon.name: "edit-rename"
                            display: Controls.AbstractButton.IconOnly
                            enabled: !["all", "favorites", "trash"].includes(store.scope)
                            onClicked: folderEditor.openFolder(true)
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: text
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        Controls.ToolButton {
                            text: qsTr("Eliminar carpeta")
                            icon.name: "edit-delete"
                            display: Controls.AbstractButton.IconOnly
                            enabled: !["all", "favorites", "trash"].includes(store.scope)
                            onClicked: deleteFolderDialog.open()
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: text
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                }
            }
        }

        Controls.Pane {
            Controls.SplitView.fillWidth: true
            Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 36
            padding: 0
            ColumnLayout {
                anchors.fill: parent
                spacing: 0
                PanelHeader {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Controls.ToolButton {
                            action: newEntry
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Kirigami.SearchField {
                            id: search
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            placeholderText: qsTr("Buscar en esta vista…")
                            onTextChanged: store.query = text
                        }
                    }
                }
                Controls.SplitView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    orientation: Qt.Horizontal
                    handle: SplitterHandle {}
                    Controls.Pane {
                        Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 18
                        Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 13
                        padding: 0
                        Kirigami.Theme.inherit: false
                        Kirigami.Theme.colorSet: Kirigami.Theme.View
                        ColumnLayout {
                            anchors.fill: parent
                            spacing: 0
                            PanelHeader {
                                Layout.fillWidth: true
                                contentItem: Kirigami.Heading {
                                    level: 3
                                    text: root.scopeTitle
                                    elide: Text.ElideRight
                                    verticalAlignment: Text.AlignVCenter
                                }
                            }
                            Controls.ScrollView {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                clip: true
                                ListView {
                                    id: entryList
                                    model: store.visibleEntries
                                    currentIndex: store.visibleEntries.findIndex(e => e.id === store.selectedId)
                                    activeFocusOnTab: true
                                    keyNavigationEnabled: true
                                    Accessible.role: Accessible.List
                                    onCurrentIndexChanged: {
                                        if (activeFocus && currentIndex >= 0 && currentIndex < store.visibleEntries.length)
                                            store.selectedId = store.visibleEntries[currentIndex].id;
                                    }
                                    delegate: Controls.ItemDelegate {
                                        id: entryDelegate
                                        required property var modelData
                                        width: entryList.width
                                        text: modelData.title
                                        icon.name: modelData.format === "Binario" ? "application-octet-stream" : "document-encrypt"
                                        icon.width: Kirigami.Units.iconSizes.smallMedium
                                        icon.height: Kirigami.Units.iconSizes.smallMedium
                                        highlighted: store.selectedId === modelData.id
                                        topPadding: Kirigami.Units.largeSpacing
                                        bottomPadding: Kirigami.Units.largeSpacing
                                        leftPadding: Kirigami.Units.largeSpacing
                                        rightPadding: Kirigami.Units.largeSpacing
                                        onClicked: {
                                            store.selectedId = modelData.id;
                                            entryList.forceActiveFocus();
                                        }
                                        onDoubleClicked: {
                                            if (!modelData.trashed)
                                                editor.openEntry(modelData);
                                        }
                                        Keys.onReturnPressed: clicked()
                                        Keys.onEnterPressed: clicked()
                                        contentItem: RowLayout {
                                            Kirigami.IconTitleSubtitle {
                                                Layout.fillWidth: true
                                                title: entryDelegate.text
                                                subtitle: store.folderName(entryDelegate.modelData.folder)
                                                icon: icon.fromControlsIcon(entryDelegate.icon)
                                                selected: entryDelegate.highlighted || entryDelegate.down
                                                font: entryDelegate.font
                                                subtitleFont: entryDelegate.font
                                            }
                                            Kirigami.Icon {
                                                visible: entryDelegate.modelData.favorite
                                                source: "rating"
                                                selected: entryDelegate.highlighted || entryDelegate.down
                                                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                                Layout.preferredHeight: width
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Kirigami.PlaceholderMessage {
                            anchors.centerIn: parent
                            width: parent.width - Kirigami.Units.largeSpacing * 4
                            visible: entryList.count === 0
                            icon.name: store.query ? "edit-find" : "document-new"
                            text: store.query ? qsTr("Sin coincidencias") : qsTr("Carpeta vacía")
                            explanation: store.query ? qsTr("Prueba con otro título o nota.") : qsTr("Crea un elemento para empezar.")
                        }
                    }
                    Controls.Pane {
                        Controls.SplitView.fillWidth: true
                        Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 21
                        padding: 0
                        ColumnLayout {
                            anchors.fill: parent
                            spacing: 0
                            PanelHeader {
                                Layout.fillWidth: true
                                contentItem: RowLayout {
                                    Kirigami.Heading {
                                        Layout.fillWidth: true
                                        level: 3
                                        text: qsTr("Detalle")
                                    }
                                    Controls.ToolButton {
                                        text: qsTr("Favorito")
                                        icon.name: "rating"
                                        checkable: true
                                        checked: store.selected?.favorite || false
                                        enabled: editEntry.enabled
                                        onClicked: store.updateEntry(store.selectedId, {
                                            favorite: !store.selected.favorite
                                        })
                                        Controls.ToolTip.visible: hovered
                                        Controls.ToolTip.text: text
                                    }
                                    Controls.ToolButton {
                                        action: editEntry
                                    }
                                    Controls.ToolButton {
                                        action: trashEntry
                                        display: Controls.AbstractButton.IconOnly
                                        Controls.ToolTip.visible: hovered
                                        Controls.ToolTip.text: text
                                    }
                                }
                            }
                            Controls.ScrollView {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                contentWidth: availableWidth
                                clip: true
                                ColumnLayout {
                                    width: parent.width
                                    spacing: Kirigami.Units.largeSpacing
                                    visible: !!store.selected
                                    Kirigami.Heading {
                                        Layout.fillWidth: true
                                        Layout.margins: Kirigami.Units.largeSpacing
                                        Layout.bottomMargin: 0
                                        level: 2
                                        text: store.selected?.title || ""
                                        wrapMode: Text.Wrap
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        Layout.leftMargin: Kirigami.Units.largeSpacing
                                        Layout.rightMargin: Kirigami.Units.largeSpacing
                                        text: qsTr("%1 · %2").arg(store.folderName(store.selected?.folder || "")).arg(store.selected?.format || "")
                                        color: Kirigami.Theme.disabledTextColor
                                        elide: Text.ElideRight
                                    }
                                    Kirigami.Separator {
                                        Layout.fillWidth: true
                                    }
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        Layout.margins: Kirigami.Units.largeSpacing
                                        spacing: Kirigami.Units.largeSpacing
                                        Kirigami.Heading {
                                            level: 3
                                            text: qsTr("Contenido")
                                        }
                                        Controls.TextArea {
                                            id: contentPreview
                                            Layout.fillWidth: true
                                            Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 3)
                                            readOnly: true
                                            selectByMouse: true
                                            wrapMode: TextEdit.Wrap
                                            text: root.revealed ? store.selected?.value || "" : qsTr("Contenido oculto")
                                        }
                                        RowLayout {
                                            Controls.Button {
                                                text: root.revealed ? qsTr("Ocultar") : qsTr("Mostrar")
                                                icon.name: root.revealed ? "view-hidden" : "view-visible"
                                                onClicked: root.revealed = !root.revealed
                                            }
                                            Controls.Button {
                                                text: qsTr("Copiar")
                                                icon.name: "edit-copy"
                                                enabled: root.revealed
                                                onClicked: {
                                                    contentPreview.selectAll();
                                                    contentPreview.copy();
                                                    contentPreview.deselect();
                                                }
                                            }
                                        }
                                    }
                                    Kirigami.Separator {
                                        Layout.fillWidth: true
                                    }
                                    Kirigami.FormLayout {
                                        Layout.fillWidth: true
                                        Layout.margins: Kirigami.Units.largeSpacing
                                        Controls.Label {
                                            Kirigami.FormData.label: qsTr("Destinatarios:")
                                            text: store.selected?.recipients || ""
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                        Controls.Label {
                                            Kirigami.FormData.label: qsTr("Notas:")
                                            text: store.selected?.notes || qsTr("Sin notas")
                                            wrapMode: Text.Wrap
                                            Layout.fillWidth: true
                                        }
                                    }
                                    Controls.Button {
                                        Layout.leftMargin: Kirigami.Units.largeSpacing
                                        text: qsTr("Mover a otra carpeta…")
                                        icon.name: "folder-move"
                                        enabled: editEntry.enabled
                                        visible: !store.selected?.trashed
                                        onClicked: moveDialog.openMove()
                                    }
                                    ColumnLayout {
                                        Layout.margins: Kirigami.Units.largeSpacing
                                        visible: store.selected?.trashed || false
                                        Controls.Button {
                                            text: qsTr("Restaurar")
                                            icon.name: "edit-undo"
                                            onClicked: {
                                                store.updateEntry(store.selectedId, {
                                                    trashed: false
                                                });
                                                store.selectScope("all");
                                            }
                                        }
                                        Controls.Button {
                                            text: qsTr("Eliminar definitivamente…")
                                            icon.name: "edit-delete"
                                            onClicked: deleteEntryDialog.open()
                                        }
                                    }
                                }
                            }
                        }
                        Kirigami.PlaceholderMessage {
                            anchors.centerIn: parent
                            width: parent.width - Kirigami.Units.largeSpacing * 4
                            visible: !store.selected
                            icon.name: "document-encrypt"
                            text: qsTr("Ningún elemento seleccionado")
                            explanation: qsTr("Selecciona un elemento de la lista para ver su contenido.")
                        }
                    }
                }
                PanelFooter {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.Label {
                            text: qsTr("%1 elementos · %2").arg(store.visibleEntries.length).arg(root.scopeTitle)
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            horizontalAlignment: Text.AlignRight
                            elide: Text.ElideRight
                            text: qsTr("Demostración en memoria · sin persistencia")
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                }
            }
        }
    }
    MarkdownDemo {
        id: markdownDemo
        anchors.centerIn: parent
    }
    Controls.Dialog {
        id: editor
        objectName: "entryEditor"
        property string entryId: ""
        anchors.centerIn: parent
        width: Math.min(600, root.width - 60)
        modal: true
        title: entryId ? qsTr("Editar elemento") : qsTr("Nuevo elemento")
        standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
        function openEntry(entry) {
            entryId = entry ? entry.id : "";
            titleField.text = entry ? entry.title : "";
            valueField.text = entry ? entry.value : "";
            notesField.text = entry ? entry.notes : "";
            formatField.currentIndex = entry?.format === "Binario" ? 1 : 0;
            folderField.currentIndex = Math.max(0, folderField.model.findIndex(f => f.id === (entry ? entry.folder : store.scope)));
            const recipients = entry ? entry.recipients : "Llave A";
            keyA.checked = recipients.includes("Llave A");
            keyB.checked = recipients.includes("Llave B");
            keyC.checked = recipients.includes("Llave C");
            open();
            titleField.forceActiveFocus();
        }
        onOpened: standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => titleField.text.trim().length > 0 && (keyA.checked || keyB.checked || keyC.checked))
        onAccepted: store.saveEntry(entryId, folderField.currentValue, titleField.text, formatField.currentText, valueField.text, notesField.text, [keyA.checked ? "Llave A" : "", keyB.checked ? "Llave B" : "", keyC.checked ? "Llave C" : ""].filter(Boolean).join(", "))
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                text: qsTr("Título:")
            }
            Controls.TextField {
                id: titleField
                Layout.fillWidth: true
                placeholderText: qsTr("Nombre del elemento")
            }
            RowLayout {
                Controls.Label {
                    text: qsTr("Carpeta:")
                }
                Controls.ComboBox {
                    id: folderField
                    Layout.fillWidth: true
                    model: [
                        {
                            id: "",
                            name: qsTr("Sin carpeta")
                        }
                    ].concat(store.folderRows)
                    textRole: "name"
                    valueRole: "id"
                }
                Controls.ComboBox {
                    id: formatField
                    model: [qsTr("Texto"), qsTr("Binario")]
                }
            }
            Controls.Label {
                text: formatField.currentIndex === 1 ? qsTr("Bytes de demostración (hexadecimal):") : qsTr("Contenido de demostración:")
            }
            Controls.ScrollView {
                Layout.fillWidth: true
                Layout.preferredHeight: 160
                Controls.TextArea {
                    id: valueField
                    wrapMode: TextEdit.Wrap
                    selectByMouse: true
                }
            }
            Controls.Label {
                text: qsTr("Notas:")
            }
            Controls.TextField {
                id: notesField
                Layout.fillWidth: true
            }
            Controls.GroupBox {
                title: qsTr("Llaves ficticias autorizadas (cualquiera de ellas)")
                Layout.fillWidth: true
                RowLayout {
                    Controls.CheckBox {
                        id: keyA
                        text: qsTr("Llave A")
                    }
                    Controls.CheckBox {
                        id: keyB
                        text: qsTr("Llave B")
                    }
                    Controls.CheckBox {
                        id: keyC
                        text: qsTr("Llave C")
                    }
                }
            }
        }
    }
    Controls.Dialog {
        id: folderEditor
        objectName: "folderEditor"
        property string folderId: ""
        anchors.centerIn: parent
        width: 420
        modal: true
        title: folderId ? qsTr("Renombrar carpeta") : qsTr("Nueva carpeta")
        standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
        function openFolder(rename) {
            folderId = rename ? store.scope : "";
            folderNameField.text = rename ? store.folderName(store.scope) : "";
            parentField.currentIndex = Math.max(0, parentField.model.findIndex(f => f.id === (rename ? store.folders.find(f => f.id === folderId).parent : store.scope)));
            open();
            folderNameField.forceActiveFocus();
        }
        onOpened: standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => folderNameField.text.trim().length > 0)
        onAccepted: store.saveFolder(folderId, folderNameField.text, parentField.currentValue)
        contentItem: ColumnLayout {
            Controls.Label {
                text: qsTr("Nombre:")
            }
            Controls.TextField {
                id: folderNameField
                Layout.fillWidth: true
            }
            Controls.Label {
                text: qsTr("Dentro de:")
                visible: !folderEditor.folderId
            }
            Controls.ComboBox {
                id: parentField
                Layout.fillWidth: true
                visible: !folderEditor.folderId
                model: [
                    {
                        id: "",
                        name: qsTr("Carpetas principales")
                    }
                ].concat(store.folderRows)
                textRole: "name"
                valueRole: "id"
            }
        }
    }
    Controls.Dialog {
        id: moveDialog
        anchors.centerIn: parent
        width: 420
        modal: true
        title: qsTr("Mover elemento")
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        function openMove() {
            destination.currentIndex = Math.max(0, destination.model.findIndex(f => f.id === store.selected.folder));
            open();
        }
        onAccepted: store.updateEntry(store.selectedId, {
            folder: destination.currentValue
        })
        contentItem: ColumnLayout {
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Mover solo cambia la carpeta. Los destinatarios se conservan.")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: destination
                Layout.fillWidth: true
                model: [
                    {
                        id: "",
                        name: qsTr("Sin carpeta")
                    }
                ].concat(store.folderRows)
                textRole: "name"
                valueRole: "id"
            }
        }
    }
    Controls.Dialog {
        id: deleteFolderDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Eliminar carpeta")
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onAccepted: store.removeFolder(store.scope)
        Controls.Label {
            text: qsTr("Se eliminará la carpeta. Sus elementos y subcarpetas pasarán a la carpeta superior.")
            width: 380
            wrapMode: Text.Wrap
        }
    }
    Controls.Dialog {
        id: deleteEntryDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Eliminar definitivamente")
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onAccepted: store.removeEntry(store.selectedId)
        Controls.Label {
            text: qsTr("¿Eliminar este elemento de la demostración?")
        }
    }
    Controls.Dialog {
        id: aboutDialog
        anchors.centerIn: parent
        modal: true
        title: qsTr("Acerca de KryptoPass")
        standardButtons: Controls.Dialog.Close
        Controls.Label {
            width: 400
            wrapMode: Text.Wrap
            text: qsTr("Prototipo de interfaz KDE para organizar secretos en carpetas.\n\nTodos los datos y llaves son ficticios. El contenido vive en memoria, no se cifra ni se guarda en disco.")
        }
    }
}
