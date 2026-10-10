pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Pane {
    id: root

    required property bool keysMode
    required property SecretStore storeAdapter
    required property KeyStore keyStore
    required property bool revealed
    required property real headerHeight
    required property Controls.Action editAction
    required property Controls.Action trashAction
    required property Controls.Action copyFingerprintAction

    property bool editing: false
    property alias editorHost: editorHost

    readonly property var selected: storeAdapter.selected
    readonly property var selectedDetail: storeAdapter.detail && selected
            && storeAdapter.detail.id === selected.id ? storeAdapter.detail : null

    signal favoriteToggleRequested()
    signal restoreRequested()
    signal permanentDeleteRequested()
    signal revealToggleRequested()
    signal copyUsernameRequested()
    signal copyPasswordRequested()
    signal retryDetailRequested()

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
            visible: !root.editing
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
                    text: root.keysMode ? qsTr("Propiedades de la clave") : qsTr("Detalle")
                }
                Controls.ToolButton {
                    visible: !root.keysMode && !!root.selected && !root.selected.trashed
                    text: qsTr("Favorito")
                    icon.name: "rating"
                    checkable: true
                    checked: root.selected?.favorite || false
                    enabled: !root.storeAdapter.controller.busy
                    onClicked: root.favoriteToggleRequested()
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    action: root.editAction
                    visible: !root.keysMode
                }
                Controls.ToolButton {
                    action: root.trashAction
                    visible: !root.keysMode && !!root.selected && !root.selected.trashed
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.visible: hovered || visualFocus
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

        Item {
            id: editorHost
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.editing
        }

        KeyDetail {
            id: keyDetail
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.keysMode && !root.editing
            selectedKey: root.keyStore.selected
        }

        Controls.ScrollView {
            id: detailScroll
            visible: !root.editing && !root.keysMode && !!root.selected
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true

            ColumnLayout {
                width: detailScroll.availableWidth
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Separator {
                    Layout.fillWidth: true
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: root.storeAdapter.kindIcon(root.selected?.kind || "note")
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
                            text: root.selected?.title || ""
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("%1 · %2").arg(root.storeAdapter.folderName(root.selected?.folder || "")).arg(root.storeAdapter.kindName(root.selected?.kind || "note"))
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            elide: Text.ElideRight
                        }
                    }
                }

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: !root.selectedDetail && root.storeAdapter.controller.detailBusy
                    type: Kirigami.MessageType.Information
                    text: qsTr("Descifrando el contenido del elemento…")
                }

                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: !root.selectedDetail && !root.storeAdapter.controller.detailBusy && !!root.storeAdapter.controller.errorMessage
                    type: Kirigami.MessageType.Error
                    text: root.storeAdapter.controller.errorMessage
                    actions: [
                        Kirigami.Action {
                            text: qsTr("Reintentar")
                            icon.name: "view-refresh"
                            onTriggered: root.retryDetailRequested()
                        }
                    ]
                }

                SecretDetail {
                    Layout.fillWidth: true
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: !!root.selectedDetail
                    detail: root.selectedDetail
                    revealed: root.revealed
                    onRevealToggled: root.revealToggleRequested()
                    onCopyUsernameRequested: root.copyUsernameRequested()
                    onCopyPasswordRequested: root.copyPasswordRequested()
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
                        Layout.fillWidth: true
                        text: root.selected ? (root.selected.recipients || []).join(", ") : ""
                        textFormat: Text.PlainText
                        wrapMode: Text.WrapAnywhere
                    }
                }

                ColumnLayout {
                    Layout.margins: Kirigami.Units.largeSpacing
                    visible: root.selected?.trashed || false
                    Controls.Button {
                        text: qsTr("Restaurar")
                        icon.name: "edit-undo"
                        enabled: !root.storeAdapter.controller.busy
                        onClicked: root.restoreRequested()
                    }
                    Controls.Button {
                        text: qsTr("Eliminar definitivamente…")
                        icon.name: "edit-delete"
                        enabled: !root.storeAdapter.controller.busy
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
        visible: !root.editing && (root.keysMode ? !root.keyStore.selected : !root.selected)
        icon.name: root.keysMode ? "view-certificate" : "document-encrypt"
        text: root.keysMode ? qsTr("Ninguna clave seleccionada") : qsTr("Ningún elemento seleccionado")
        explanation: root.keysMode ? qsTr("Selecciona una clave para ver sus identidades, subclaves y firmas.") : qsTr("Selecciona un elemento de la lista para ver su contenido.")
    }
}
