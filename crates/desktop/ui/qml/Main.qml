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
    KeyStore {
        id: keys
        objectName: "keyStore"
    }
    property bool keysMode: false
    readonly property var activeStore: keysMode ? keys : store
    function selectSection(showKeys) {
        if (keysMode === showKeys)
            return;
        revealed = false;
        keysMode = showKeys;
        search.text = activeStore.query;
        if (showKeys)
            keys.controller.refreshKeys();
    }
    property bool revealed: false
    readonly property string scopeTitle: keysMode ? (keys.scope === "own" ? qsTr("Claves propias") : keys.scope === "contacts" ? qsTr("Destinatarios") : qsTr("Todas las claves")) : store.scope === "all" ? qsTr("Todos los elementos") : store.scope === "favorites" ? qsTr("Favoritos") : store.scope === "trash" ? qsTr("Papelera") : store.folderName(store.scope)
    Connections {
        target: store
        function onSelectedIdChanged() {
            root.revealed = false;
        }
        function onQueryChanged() {
            if (!root.keysMode && search.text !== store.query)
                search.text = store.query;
        }
    }
    Connections {
        target: keys
        function onQueryChanged() {
            if (root.keysMode && search.text !== keys.query)
                search.text = keys.query;
        }
    }

    Controls.Action {
        id: newEntry
        text: qsTr("Nuevo elemento…")
        icon.name: "document-new"
        enabled: !root.keysMode
        shortcut: StandardKey.New
        onTriggered: editor.openEntry(null)
    }
    Controls.Action {
        id: editEntry
        text: qsTr("Editar…")
        icon.name: "document-edit"
        enabled: !root.keysMode && !!store.selected && !store.selected.trashed
        shortcut: "F2"
        onTriggered: editor.openEntry(store.selected)
    }
    Controls.Action {
        id: trashEntry
        text: qsTr("Mover a la papelera")
        icon.name: "user-trash"
        enabled: !root.keysMode && !!store.selected && !store.selected.trashed
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
        enabled: !root.keysMode
        shortcut: "Ctrl+Shift+N"
        onTriggered: folderEditor.openFolder(false)
    }
    Controls.Action {
        id: refreshKeys
        text: qsTr("Actualizar claves")
        icon.name: "view-refresh"
        enabled: root.keysMode && !keys.controller.busy
        shortcut: "F5"
        onTriggered: keys.controller.refreshKeys()
    }
    Controls.Action {
        id: copyFingerprint
        text: qsTr("Copiar huella")
        icon.name: "edit-copy"
        enabled: root.keysMode && !!keys.selected
        onTriggered: keyDetail.copyFingerprint()
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
            Controls.MenuItem {
                action: refreshKeys
                visible: root.keysMode
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
            Controls.MenuItem {
                action: copyFingerprint
                visible: root.keysMode
            }
        }
        Controls.Menu {
            title: qsTr("&Ver")
            Controls.MenuItem {
                text: qsTr("Carpetas")
                checkable: true
                checked: !root.keysMode
                onTriggered: root.selectSection(false)
            }
            Controls.MenuItem {
                text: qsTr("Claves")
                checkable: true
                checked: root.keysMode
                onTriggered: root.selectSection(true)
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Markdown experimental…")
                onTriggered: markdownDemo.open()
            }
            Controls.MenuSeparator {}
            Controls.MenuItem {
                text: qsTr("Todos los elementos")
                onTriggered: {
                    root.selectSection(false);
                    store.selectScope("all");
                }
            }
            Controls.MenuItem {
                text: qsTr("Favoritos")
                onTriggered: {
                    root.selectSection(false);
                    store.selectScope("favorites");
                }
            }
            Controls.MenuItem {
                text: qsTr("Papelera")
                onTriggered: {
                    root.selectSection(false);
                    store.selectScope("trash");
                }
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
    readonly property real panelHeaderHeight: Math.max(Kirigami.Units.gridUnit * 2.5, toolbarSizeHint.implicitHeight + Kirigami.Units.smallSpacing * 2)
    Controls.ToolButton {
        id: toolbarSizeHint
        visible: false
        text: qsTr("Actualizar claves")
        icon.name: "view-refresh"
    }

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
        leftPadding: Kirigami.Units.largeSpacing + depth * Kirigami.Units.iconSizes.small
        rightPadding: Kirigami.Units.largeSpacing
        highlighted: root.activeStore.scope === scopeId
        onClicked: root.activeStore.selectScope(scopeId)
        contentItem: RowLayout {
            Kirigami.IconTitleSubtitle {
                Layout.fillWidth: true
                title: navigationItem.text
                icon: icon.fromControlsIcon(navigationItem.icon)
                selected: navigationItem.highlighted || navigationItem.down
                font: navigationItem.font
            }
            Controls.Label {
                text: root.activeStore.count(navigationItem.scopeId)
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
                        Controls.ToolButton {
                            id: sectionPicker
                            objectName: "sectionPicker"
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            leftPadding: Kirigami.Units.largeSpacing
                            rightPadding: Kirigami.Units.largeSpacing
                            topPadding: Kirigami.Units.smallSpacing
                            bottomPadding: Kirigami.Units.smallSpacing
                            readonly property string sectionTitle: root.keysMode ? qsTr("Claves") : qsTr("Carpetas")
                            Accessible.name: qsTr("Cambiar sección: %1").arg(sectionTitle)
                            onClicked: sectionMenu.popup(sectionPicker, 0, sectionPicker.height)
                            contentItem: RowLayout {
                                spacing: Kirigami.Units.largeSpacing
                                Kirigami.Heading {
                                    Layout.fillWidth: true
                                    level: 3
                                    text: sectionPicker.sectionTitle
                                    elide: Text.ElideRight
                                }
                                Kirigami.Icon {
                                    source: "arrow-down"
                                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                    Layout.preferredHeight: width
                                }
                            }
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: qsTr("Cambiar entre carpetas y claves")
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                            Controls.Menu {
                                id: sectionMenu
                                Controls.MenuItem {
                                    text: qsTr("Carpetas")
                                    icon.name: "folder"
                                    checkable: true
                                    checked: !root.keysMode
                                    onTriggered: root.selectSection(false)
                                }
                                Controls.MenuItem {
                                    text: qsTr("Claves")
                                    icon.name: "view-certificate"
                                    checkable: true
                                    checked: root.keysMode
                                    onTriggered: root.selectSection(true)
                                }
                            }
                        }
                        Controls.ToolButton {
                            action: newFolder
                            visible: !root.keysMode
                            display: Controls.AbstractButton.IconOnly
                            Controls.ToolTip.visible: hovered
                            Controls.ToolTip.text: text
                            Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                        }
                    }
                }
                Repeater {
                    model: root.keysMode ? [
                        {
                            id: "all",
                            name: qsTr("Todas las claves"),
                            icon: "view-certificate"
                        },
                        {
                            id: "own",
                            name: qsTr("Claves propias"),
                            icon: "user-identity"
                        },
                        {
                            id: "contacts",
                            name: qsTr("Destinatarios"),
                            icon: "view-pim-contacts"
                        }
                    ] : [
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
                    visible: !root.keysMode
                    text: qsTr("Mis carpetas")
                }
                Controls.ScrollView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    ListView {
                        id: folderList
                        model: root.keysMode ? [] : store.folderRows
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
                            visible: !root.keysMode
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
                            visible: !root.keysMode
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
                            visible: !root.keysMode
                        }
                        Controls.ToolButton {
                            action: refreshKeys
                            visible: root.keysMode
                        }
                        Controls.BusyIndicator {
                            visible: root.keysMode && keys.controller.busy
                            running: visible
                            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                            Layout.preferredHeight: width
                            Accessible.name: qsTr("Actualizando claves de GnuPG")
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Kirigami.SearchField {
                            id: search
                            Layout.fillWidth: true
                            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
                            placeholderText: root.keysMode ? qsTr("Buscar nombre, email o huella…") : qsTr("Buscar en esta vista…")
                            onTextChanged: root.activeStore.query = text
                        }
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: root.keysMode && !!keys.controller.loadError
                    position: Kirigami.InlineMessage.Position.Header
                    type: Kirigami.MessageType.Error
                    text: qsTr("No se pudieron cargar las claves de GnuPG.")
                    actions: [
                        Kirigami.Action {
                            text: qsTr("Reintentar")
                            icon.name: "view-refresh"
                            enabled: refreshKeys.enabled
                            onTriggered: keys.controller.refreshKeys()
                        },
                        Kirigami.Action {
                            text: qsTr("Ver detalles…")
                            icon.name: "dialog-information"
                            onTriggered: keyErrorDialog.open()
                        }
                    ]
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
                                    model: root.activeStore.visibleEntries
                                    currentIndex: root.activeStore.visibleEntries.findIndex(e => e.id === root.activeStore.selectedId)
                                    activeFocusOnTab: true
                                    keyNavigationEnabled: true
                                    Accessible.role: Accessible.List
                                    onCurrentIndexChanged: {
                                        if (activeFocus && currentIndex >= 0 && currentIndex < root.activeStore.visibleEntries.length)
                                            root.activeStore.selectedId = root.activeStore.visibleEntries[currentIndex].id;
                                    }
                                    delegate: Loader {
                                        id: rowLoader
                                        required property var modelData
                                        width: entryList.width
                                        sourceComponent: root.keysMode ? keyRow : secretRow
                                        Component {
                                            id: keyRow
                                            KeyListDelegate {
                                                focus: true
                                                entry: rowLoader.modelData
                                                highlighted: keys.selectedId === entry.id
                                                onClicked: {
                                                    keys.selectedId = entry.id;
                                                    entryList.forceActiveFocus();
                                                }
                                            }
                                        }
                                        Component {
                                            id: secretRow
                                            Controls.ItemDelegate {
                                                id: entryDelegate
                                                focus: true
                                                readonly property var modelData: rowLoader.modelData
                                                text: modelData.title
                                                icon.name: modelData.format === "Binario" ? "application-octet-stream" : "document-encrypt"
                                                icon.width: Kirigami.Units.iconSizes.smallMedium
                                                icon.height: Kirigami.Units.iconSizes.smallMedium
                                                highlighted: root.activeStore.selectedId === modelData.id
                                                topPadding: Kirigami.Units.largeSpacing
                                                bottomPadding: Kirigami.Units.largeSpacing
                                                leftPadding: Kirigami.Units.largeSpacing
                                                rightPadding: Kirigami.Units.largeSpacing
                                                onClicked: {
                                                    root.activeStore.selectedId = modelData.id;
                                                    entryList.forceActiveFocus();
                                                }
                                                onDoubleClicked: {
                                                    if (!root.keysMode && !modelData.trashed)
                                                        editor.openEntry(modelData);
                                                }
                                                Keys.onReturnPressed: clicked()
                                                Keys.onEnterPressed: clicked()
                                                contentItem: RowLayout {
                                                    Kirigami.Icon {
                                                        source: entryDelegate.icon.name
                                                        selected: entryDelegate.highlighted || entryDelegate.down
                                                        Layout.preferredWidth: entryDelegate.icon.width
                                                        Layout.preferredHeight: entryDelegate.icon.height
                                                    }
                                                    Kirigami.TitleSubtitle {
                                                        Layout.fillWidth: true
                                                        textFormat: Text.PlainText
                                                        title: entryDelegate.text
                                                        subtitle: store.folderName(entryDelegate.modelData.folder)
                                                        selected: entryDelegate.highlighted || entryDelegate.down
                                                        font: entryDelegate.font
                                                        subtitleFont: entryDelegate.font
                                                    }
                                                    Kirigami.Icon {
                                                        visible: !root.keysMode && !!entryDelegate.modelData.favorite
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
                            }
                        }
                        Kirigami.PlaceholderMessage {
                            anchors.centerIn: parent
                            width: parent.width - Kirigami.Units.largeSpacing * 4
                            visible: entryList.count === 0
                            icon.name: root.keysMode && keys.controller.loadError ? "dialog-error" : root.activeStore.query ? "edit-find" : root.keysMode ? "view-certificate" : "document-new"
                            text: root.keysMode ? (keys.controller.busy ? qsTr("Cargando claves…") : keys.controller.loadError ? qsTr("Claves no disponibles") : keys.query ? qsTr("Sin coincidencias") : qsTr("No hay claves en esta vista")) : store.query ? qsTr("Sin coincidencias") : qsTr("Carpeta vacía")
                            explanation: root.keysMode ? (keys.controller.loadError ? qsTr("Reintenta la carga o consulta los detalles del error.") : keys.controller.busy ? qsTr("Consultando el llavero local de GnuPG.") : keys.query ? qsTr("Prueba con otro nombre, correo o huella.") : qsTr("Las claves del llavero local de GnuPG aparecerán aquí.")) : store.query ? qsTr("Prueba con otro título o nota.") : qsTr("Crea un elemento para empezar.")
                        }
                    }
                    Controls.Pane {
                        id: detailPane
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
                                        objectName: "detailHeading"
                                        Layout.fillWidth: true
                                        Layout.minimumWidth: 0
                                        level: 3
                                        elide: Text.ElideRight
                                        textFormat: Text.PlainText
                                        text: root.keysMode ? qsTr("Propiedades de la clave") : qsTr("Detalle")
                                    }
                                    Controls.ToolButton {
                                        visible: !root.keysMode
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
                                        visible: !root.keysMode
                                    }
                                    Controls.ToolButton {
                                        action: trashEntry
                                        visible: !root.keysMode
                                        display: Controls.AbstractButton.IconOnly
                                        Controls.ToolTip.visible: hovered
                                        Controls.ToolTip.text: text
                                    }
                                    Controls.ToolButton {
                                        action: copyFingerprint
                                        visible: root.keysMode
                                        display: detailPane.width < Kirigami.Units.gridUnit * 26 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                                        Controls.ToolTip.visible: hovered || visualFocus
                                        Controls.ToolTip.text: text
                                        Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                                    }
                                }
                            }
                            KeyDetail {
                                id: keyDetail
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                visible: root.keysMode
                                selectedKey: keys.selected
                            }
                            Controls.ScrollView {
                                visible: !root.keysMode
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
                                    Kirigami.Heading {
                                        Layout.leftMargin: Kirigami.Units.largeSpacing
                                        level: 3
                                        text: qsTr("Información del elemento")
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
                            anchors.verticalCenterOffset: root.panelHeaderHeight / 2
                            visible: root.keysMode ? !keys.selected : !store.selected
                            icon.name: root.keysMode ? "view-certificate" : "document-encrypt"
                            text: root.keysMode ? qsTr("Ninguna clave seleccionada") : qsTr("Ningún elemento seleccionado")
                            explanation: root.keysMode ? qsTr("Selecciona una clave para ver sus identidades, subclaves y firmas.") : qsTr("Selecciona un elemento de la lista para ver su contenido.")
                        }
                    }
                }
                PanelFooter {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.Label {
                            Layout.maximumWidth: parent.width * 0.6
                            elide: Text.ElideRight
                            text: root.keysMode ? qsTr("%1 claves · %2").arg(keys.visibleEntries.length).arg(root.scopeTitle) : qsTr("%1 elementos · %2").arg(store.visibleEntries.length).arg(root.scopeTitle)
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            horizontalAlignment: Text.AlignRight
                            elide: Text.ElideRight
                            text: root.keysMode ? keys.controller.status : qsTr("Demostración en memoria · sin persistencia")
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
        height: Math.min(implicitHeight, parent.height - Kirigami.Units.largeSpacing * 4)
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
        function ensureFocusVisible() {
            let item = root.activeFocusItem;
            while (item && item.parent !== entryForm)
                item = item.parent;
            const flickable = editorScroll.contentItem as Flickable;
            if (!item || !flickable)
                return;
            const top = item.y;
            const bottom = top + item.height;
            if (top < flickable.contentY)
                flickable.contentY = top;
            else if (bottom > flickable.contentY + flickable.height)
                flickable.contentY = Math.max(0, bottom - flickable.height);
        }
        Connections {
            target: root
            function onActiveFocusItemChanged() {
                if (editor.opened)
                    Qt.callLater(editor.ensureFocusVisible);
            }
        }
        onOpened: {
            standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => titleField.text.trim().length > 0 && (keyA.checked || keyB.checked || keyC.checked));
            Qt.callLater(ensureFocusVisible);
        }
        onAccepted: store.saveEntry(entryId, folderField.currentValue, titleField.text, formatField.currentText, valueField.text, notesField.text, [keyA.checked ? "Llave A" : "", keyB.checked ? "Llave B" : "", keyC.checked ? "Llave C" : ""].filter(Boolean).join(", "))
        contentItem: Controls.ScrollView {
            id: editorScroll
            objectName: "entryEditorScroll"
            implicitHeight: entryForm.implicitHeight
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                id: entryForm
                width: editorScroll.availableWidth
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    text: qsTr("Título:")
                }
                Controls.TextField {
                    id: titleField
                    Layout.fillWidth: true
                    Accessible.name: qsTr("Título del elemento")
                    placeholderText: qsTr("Nombre del elemento")
                }
                RowLayout {
                    Controls.Label {
                        text: qsTr("Carpeta:")
                    }
                    Controls.ComboBox {
                        id: folderField
                        Layout.fillWidth: true
                        Accessible.name: qsTr("Carpeta del elemento")
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
                        Accessible.name: qsTr("Formato del contenido")
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
                        Accessible.name: qsTr("Contenido de demostración")
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
                    Accessible.name: qsTr("Notas del elemento")
                }
                Controls.GroupBox {
                    title: qsTr("Llaves de demostración")
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Cualquiera de las llaves seleccionadas podrá abrir el elemento.")
                            wrapMode: Text.Wrap
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
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
            text: qsTr("Prototipo de interfaz KDE para organizar secretos en carpetas.\n\nLos elementos de demostración y sus destinatarios son ficticios. Su contenido vive en memoria, no se cifra ni se guarda en disco.\n\nLa sección Claves consulta metadatos reales del llavero GnuPG del usuario. Esta vista no modifica las claves ni muestra material privado.")
        }
    }
    Controls.Dialog {
        id: keyErrorDialog
        objectName: "keyErrorDialog"
        anchors.centerIn: parent
        width: Math.min(640, parent.width - Kirigami.Units.largeSpacing * 4)
        height: Math.min(implicitHeight, parent.height - Kirigami.Units.largeSpacing * 4)
        title: qsTr("Error al cargar claves")
        modal: true
        standardButtons: Controls.Dialog.Close
        contentItem: Controls.ScrollView {
            implicitHeight: Math.min(errorText.implicitHeight, Kirigami.Units.gridUnit * 14)
            Controls.TextArea {
                id: errorText
                text: keys.controller.loadError
                readOnly: true
                selectByMouse: true
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.Wrap
                Accessible.name: qsTr("Detalles del error de GnuPG")
            }
        }
    }
}
