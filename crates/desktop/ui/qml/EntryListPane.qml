pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property var activeStore
    required property SecretStore storeAdapter
    required property KeyStore keyStore
    required property string scopeTitle
    required property real headerHeight
    property bool entryDraggingEnabled: false

    // The shell owns adapter mutations and the editor dialog; this pane reports user intent.
    signal selectionRequested(string entryId)
    signal entryOpenRequested(var entry)

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
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    text: root.scopeTitle
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                }
                Controls.ToolButton {
                    visible: !root.keysMode
                    checkable: true
                    checked: root.storeAdapter.sortDescending
                    text: checked ? qsTr("Z a A") : qsTr("A a Z")
                    icon.name: checked ? "view-sort-descending" : "view-sort-ascending"
                    Accessible.name: checked ? qsTr("Orden descendente") : qsTr("Orden ascendente")
                    onToggled: root.storeAdapter.sortDescending = checked
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: qsTr("Cambiar orden por nombre")
                }
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
                        root.selectionRequested(root.activeStore.visibleEntries[currentIndex].id);
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
                            highlighted: root.keyStore.selectedId === entry.id
                            onClicked: {
                                root.selectionRequested(entry.id);
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
                            icon.name: root.storeAdapter.kindIcon(modelData.kind)
                            icon.width: Kirigami.Units.iconSizes.smallMedium
                            icon.height: Kirigami.Units.iconSizes.smallMedium
                            highlighted: root.activeStore.selectedId === modelData.id
                            topPadding: Kirigami.Units.largeSpacing
                            bottomPadding: Kirigami.Units.largeSpacing
                            leftPadding: Kirigami.Units.largeSpacing
                            rightPadding: Kirigami.Units.largeSpacing
                            onClicked: {
                                root.selectionRequested(modelData.id);
                                entryList.forceActiveFocus();
                            }
                            onDoubleClicked: {
                                if (!root.keysMode && !modelData.trashed && !root.storeAdapter.controller.busy)
                                    root.entryOpenRequested(modelData);
                            }
                            DragHandler {
                                id: entryDrag
                                target: null
                                acceptedButtons: Qt.LeftButton
                                enabled: root.entryDraggingEnabled && !entryDelegate.modelData.trashed
                                function positionPreview() {
                                    const point = entryDelegate.mapToItem(dragPreview.parent, centroid.position.x, centroid.position.y);
                                    dragPreview.x = point.x;
                                    dragPreview.y = point.y;
                                }
                                onActiveTranslationChanged: {
                                    if (active)
                                        positionPreview();
                                }
                                onActiveChanged: {
                                    if (active) {
                                        positionPreview();
                                        dragPreview.Drag.active = true;
                                    } else if (dragPreview.Drag.active) {
                                        dragPreview.Drag.drop();
                                        dragPreview.Drag.active = false;
                                    }
                                }
                            }
                            Shortcut {
                                sequence: "Esc"
                                enabled: dragPreview.Drag.active
                                onActivated: dragPreview.Drag.cancel()
                            }
                            Rectangle {
                                id: dragPreview
                                parent: Controls.Overlay.overlay
                                readonly property string entryId: entryDelegate.modelData.id
                                readonly property var storeAdapter: root.storeAdapter
                                visible: Drag.active
                                z: 1000
                                width: dragLabel.implicitWidth + 24
                                height: dragLabel.implicitHeight + 16
                                radius: 6
                                color: Kirigami.Theme.backgroundColor
                                border.color: Kirigami.Theme.highlightColor
                                Drag.source: dragPreview
                                Drag.keys: ["org.kryptopass.entry"]
                                Drag.supportedActions: Qt.MoveAction
                                Drag.proposedAction: Qt.MoveAction
                                Controls.Label {
                                    id: dragLabel
                                    anchors.centerIn: parent
                                    text: entryDelegate.text
                                    textFormat: Text.PlainText
                                    elide: Text.ElideRight
                                    width: Math.min(implicitWidth, 240)
                                }
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
                                    subtitle: qsTranslate("Main", "%1 · %2").arg(root.storeAdapter.folderName(entryDelegate.modelData.folder)).arg(root.storeAdapter.kindName(entryDelegate.modelData.kind))
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
        icon.name: root.keysMode && root.keyStore.controller.loadError ? "dialog-error" : root.activeStore.query ? "edit-find" : root.keysMode ? "view-certificate" : root.storeAdapter.controller.errorMessage ? "dialog-error" : "document-new"
        text: root.keysMode ? (root.keyStore.controller.busy ? qsTranslate("Main", "Cargando claves…") : root.keyStore.controller.loadError ? qsTranslate("Main", "Claves no disponibles") : root.keyStore.query ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "No hay claves en esta vista")) : root.storeAdapter.controller.busy ? qsTranslate("Main", "Actualizando elementos…") : root.storeAdapter.controller.errorMessage ? qsTranslate("Main", "No se pudo cargar la bóveda") : root.storeAdapter.query ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "Carpeta vacía")
        explanation: root.keysMode ? (root.keyStore.controller.loadError ? qsTranslate("Main", "Reintenta la carga o consulta los detalles del error.") : root.keyStore.controller.busy ? qsTranslate("Main", "Consultando el llavero local de GnuPG.") : root.keyStore.query ? qsTranslate("Main", "Prueba con otro nombre, correo o huella.") : qsTranslate("Main", "Las claves del llavero local de GnuPG aparecerán aquí.")) : root.storeAdapter.controller.errorMessage ? root.storeAdapter.controller.errorMessage : root.storeAdapter.controller.busy ? qsTranslate("Main", "Consultando el almacenamiento local cifrado.") : root.storeAdapter.query ? qsTranslate("Main", "Prueba con otro título, carpeta o tipo.") : qsTranslate("Main", "Crea un elemento para empezar.")
    }
    Controls.Button {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.verticalCenter
        anchors.topMargin: Kirigami.Units.gridUnit * 5
        visible: entryList.count === 0 && (root.keysMode ? !!root.keyStore.controller.loadError : !!root.storeAdapter.controller.errorMessage)
        text: qsTr("Reintentar")
        icon.name: "view-refresh"
        enabled: root.keysMode ? !root.keyStore.controller.busy : !root.storeAdapter.controller.busy
        onClicked: root.keysMode ? root.keyStore.controller.refreshKeys() : root.storeAdapter.controller.loadStore()
    }
}
