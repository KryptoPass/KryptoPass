pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property var activeStore
    required property MockStore storeAdapter
    required property KeyStore keyStore
    required property string scopeTitle
    required property real headerHeight

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
                                if (!root.keysMode && !modelData.trashed)
                                    root.entryOpenRequested(modelData);
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
        icon.name: root.keysMode && root.keyStore.controller.loadError ? "dialog-error" : root.activeStore.query ? "edit-find" : root.keysMode ? "view-certificate" : "document-new"
        text: root.keysMode ? (root.keyStore.controller.busy ? qsTranslate("Main", "Cargando claves…") : root.keyStore.controller.loadError ? qsTranslate("Main", "Claves no disponibles") : root.keyStore.query ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "No hay claves en esta vista")) : root.storeAdapter.query ? qsTranslate("Main", "Sin coincidencias") : qsTranslate("Main", "Carpeta vacía")
        explanation: root.keysMode ? (root.keyStore.controller.loadError ? qsTranslate("Main", "Reintenta la carga o consulta los detalles del error.") : root.keyStore.controller.busy ? qsTranslate("Main", "Consultando el llavero local de GnuPG.") : root.keyStore.query ? qsTranslate("Main", "Prueba con otro nombre, correo o huella.") : qsTranslate("Main", "Las claves del llavero local de GnuPG aparecerán aquí.")) : root.storeAdapter.query ? qsTranslate("Main", "Prueba con otro título o nota.") : qsTranslate("Main", "Crea un elemento para empezar.")
    }
}
