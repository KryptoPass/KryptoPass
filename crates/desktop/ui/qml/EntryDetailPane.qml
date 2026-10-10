pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property MockStore storeAdapter
    required property KeyStore keyStore
    required property bool revealed
    required property real headerHeight
    required property Controls.Action editAction
    required property Controls.Action trashAction
    required property Controls.Action copyFingerprintAction

    signal favoriteToggleRequested()
    signal restoreRequested()
    signal permanentDeleteRequested()
    signal revealToggleRequested()

    function copySelectedFingerprint() {
        keyDetail.copyFingerprint();
    }

    Controls.SplitView.fillWidth: true
    Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 21
    padding: 0
    ColumnLayout {
        anchors.fill: parent
        spacing: 0
        PanelHeader {
            Layout.fillWidth: true
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Kirigami.Heading {
                    objectName: "detailHeading"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    level: 3
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: root.keysMode ? qsTranslate("Main", "Propiedades de la clave") : qsTranslate("Main", "Detalle")
                }
                Controls.ToolButton {
                    visible: !root.keysMode
                    text: qsTranslate("Main", "Favorito")
                    icon.name: "rating"
                    checkable: true
                    checked: root.storeAdapter.selected?.favorite || false
                    enabled: root.editAction.enabled
                    onClicked: root.favoriteToggleRequested()
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    action: root.editAction
                    visible: !root.keysMode
                }
                Controls.ToolButton {
                    action: root.trashAction
                    visible: !root.keysMode
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.visible: hovered
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    action: root.copyFingerprintAction
                    visible: root.keysMode
                    display: root.width < Kirigami.Units.gridUnit * 26 ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
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
            selectedKey: root.keyStore.selected
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
                visible: !!root.storeAdapter.selected
                Kirigami.Separator {
                    Layout.fillWidth: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: root.storeAdapter.kindIcon(root.storeAdapter.selected?.kind || "note")
                        Layout.preferredWidth: Kirigami.Units.iconSizes.large
                        Layout.preferredHeight: width
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 0
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 2
                            text: root.storeAdapter.selected?.title || ""
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTranslate("Main", "%1 · %2 · %3").arg(root.storeAdapter.folderName(root.storeAdapter.selected?.folder || "")).arg(root.storeAdapter.kindName(root.storeAdapter.selected?.kind || "note")).arg(root.storeAdapter.selected?.format || "")
                            color: Kirigami.Theme.disabledTextColor
                            elide: Text.ElideRight
                        }
                    }
                }
                SecretDetail {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    entry: root.storeAdapter.selected
                    revealed: root.revealed
                    onRevealToggled: root.revealToggleRequested()
                    onCopyRequested: function(value) {
                        copyBuffer.text = value;
                        copyBuffer.selectAll();
                        copyBuffer.copy();
                        copyBuffer.deselect();
                    }
                }
                Controls.TextArea {
                    id: copyBuffer
                    visible: false
                    textFormat: TextEdit.PlainText
                }
                Kirigami.Separator {
                    Layout.fillWidth: true
                }
                Kirigami.Heading {
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    level: 3
                    text: qsTranslate("Main", "Información del elemento")
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    Controls.Label {
                        Kirigami.FormData.label: qsTranslate("Main", "Destinatarios:")
                        text: root.storeAdapter.selected?.recipients || ""
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                    Controls.Label {
                        Kirigami.FormData.label: qsTranslate("Main", "Notas:")
                        text: root.storeAdapter.selected?.notes || qsTranslate("Main", "Sin notas")
                        wrapMode: Text.Wrap
                        Layout.fillWidth: true
                    }
                }
                ColumnLayout {
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: root.storeAdapter.selected?.trashed || false
                    Controls.Button {
                        text: qsTranslate("Main", "Restaurar")
                        icon.name: "edit-undo"
                        onClicked: root.restoreRequested()
                    }
                    Controls.Button {
                        text: qsTranslate("Main", "Eliminar definitivamente…")
                        icon.name: "edit-delete"
                        onClicked: root.permanentDeleteRequested()
                    }
                }
            }
        }
    }
    Kirigami.PlaceholderMessage {
        anchors.centerIn: parent
        width: parent.width - Kirigami.Units.largeSpacing * 4
        anchors.verticalCenterOffset: root.headerHeight / 2
        visible: root.keysMode ? !root.keyStore.selected : !root.storeAdapter.selected
        icon.name: root.keysMode ? "view-certificate" : "document-encrypt"
        text: root.keysMode ? qsTranslate("Main", "Ninguna clave seleccionada") : qsTranslate("Main", "Ningún elemento seleccionado")
        explanation: root.keysMode ? qsTranslate("Main", "Selecciona una clave para ver sus identidades, subclaves y firmas.") : qsTranslate("Main", "Selecciona un elemento de la lista para ver su contenido.")
    }
}
