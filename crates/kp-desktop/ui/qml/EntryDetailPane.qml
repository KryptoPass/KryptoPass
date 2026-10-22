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
    required property KeyOperationDialog keyOperationDialog
    required property bool revealed
    required property real headerHeight
    required property Controls.Action editAction
    required property Controls.Action trashAction
    required property Controls.Action favoriteAction
    required property Controls.Action restoreAction
    required property Controls.Action deleteAction
    required property Controls.Action copyFingerprintAction

    property bool editing: false
    property alias editorHost: editorHost

    readonly property var selected: storeAdapter.selected
    readonly property var selectedDetail: storeAdapter.detail && selected && storeAdapter.detail.id === selected.id ? storeAdapter.detail : null
    readonly property var relatedEntry: storeAdapter.relatedEntryFor(selected)
    readonly property bool hasRelatedEntryReference: !!selected && (!!selected.relatedEntryId || storeAdapter.entries.some(entry => entry.id !== selected.id && entry.relatedEntryId === selected.id))

    signal favoriteToggleRequested
    signal restoreRequested
    signal permanentDeleteRequested
    signal revealToggleRequested
    signal copyUsernameRequested
    signal copyPasswordRequested
    signal copyTotpRequested(bool next)
    signal retryDetailRequested
    signal notificationRequested(string message)
    signal relatedEntryOpenRequested(var entry)
    signal relatedEntryTrashRequested(var entry)
    signal createSecurityRequested(var entry)

    function copySelectedFingerprint() {
        keyDetail.copyFingerprint();
    }

    Controls.SplitView.fillWidth: true
    Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 21
    padding: 0

    Timer {
        interval: 1000
        repeat: true
        triggeredOnStart: true
        running: root.visible && !root.editing && !root.keysMode && !root.storeAdapter.controller.locked && !!root.selectedDetail && !!root.selectedDetail.content?.fields?.totp && Application.state === Qt.ApplicationActive
        onTriggered: root.storeAdapter.controller.refreshTotp()
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        PanelHeader {
            visible: !root.editing
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            headerHeight: root.headerHeight
            contentItem: RowLayout {
                Kirigami.Heading {
                    objectName: "detailHeading"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    level: 3
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                    text: root.keysMode ? (root.keyStore.selected && !(root.keyStore.scope === "contacts" && root.keyStore.contactsOverviewVisible) ? qsTr("Propiedades de la clave") : root.keyStore.scope === "contacts" ? qsTr("Agenda de contactos") : qsTr("Contactos de KDE Contacts")) : qsTr("Detalle")
                }
                Controls.ToolButton {
                    visible: root.keysMode && root.keyStore.scope === "contacts"
                    text: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? qsTr("Volver a la clave") : qsTr("Ver agenda")
                    icon.name: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? "view-certificate" : "view-pim-contacts"
                    Accessible.name: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? qsTr("Volver a los detalles de la clave") : qsTr("Ver la agenda de contactos")
                    onClicked: root.keyStore.contactsOverviewVisible = root.keyStore.selected ? !root.keyStore.contactsOverviewVisible : true
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    action: root.favoriteAction
                    visible: !root.keysMode && !!root.selected && !root.selected.trashed
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
                    action: root.restoreAction
                    visible: !root.keysMode && !!root.selected && root.selected.trashed
                    display: Controls.AbstractButton.IconOnly
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
                Controls.ToolButton {
                    action: root.deleteAction
                    visible: !root.keysMode && !!root.selected && root.selected.trashed
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
            visible: root.keysMode && !root.editing && !!root.keyStore.selected && !(root.keyStore.scope === "contacts" && root.keyStore.contactsOverviewVisible)
            selectedKey: root.keyStore.selected
            keyStore: root.keyStore
            operationDialog: root.keyOperationDialog
            onNotificationRequested: function (message) {
                root.notificationRequested(message);
            }
        }

        KeyContactsOverview {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: root.keysMode && !root.editing && (!root.keyStore.selected || (root.keyStore.scope === "contacts" && root.keyStore.contactsOverviewVisible))
            keyStore: root.keyStore
            operationDialog: root.keyOperationDialog
        }

        Controls.ScrollView {
            id: detailScroll
            objectName: "entryDetailScroll"
            visible: !root.editing && !root.keysMode && !!root.selected
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true

            Item {
                id: detailSurface
                width: detailScroll.availableWidth
                implicitHeight: detailContent.implicitHeight

                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.LeftButton
                    Accessible.ignored: true
                    onClicked: detailSurface.forceActiveFocus(Qt.MouseFocusReason)
                }

                ColumnLayout {
                    id: detailContent
                    width: detailSurface.width
                    spacing: Kirigami.Units.largeSpacing

                    Kirigami.Separator {
                        Layout.fillWidth: true
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        spacing: Kirigami.Units.largeSpacing
                        EntryIcon {
                            entryIcon: root.selected?.icon || null
                            imageSource: root.selected?.iconSource || ""
                            fallbackIcon: root.storeAdapter.kindIcon(root.selected?.kind || "note")
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
                        visible: !!root.selected && root.storeAdapter.controller.locked
                        type: Kirigami.MessageType.Information
                        text: qsTr("La bóveda está bloqueada. Desbloquéala para abrir este elemento.")
                    }

                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: !root.selectedDetail && root.storeAdapter.controller.detailBusy
                        type: Kirigami.MessageType.Information
                        text: qsTr("Descifrando el contenido del elemento…")
                    }

                    SecretDetail {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: !!root.selectedDetail
                        detail: root.selectedDetail
                        recipients: root.selected?.recipients || []
                        recipientKeys: root.keyStore.entries
                        revealed: root.revealed
                        versionDetail: root.storeAdapter.historyVersion
                        onVersionOpenRequested: function (index) {
                            root.storeAdapter.controller.openHistoryVersion(index);
                        }
                        onVersionCloseRequested: root.storeAdapter.controller.closeHistoryVersion()
                        onVersionCopyRequested: function (field) {
                            root.storeAdapter.controller.copyHistoryValue(field);
                        }
                        onActivityExportRequested: root.storeAdapter.controller.exportActivity()
                        onCopyFieldRequested: function (field) {
                            root.storeAdapter.controller.copyField(field);
                        }
                        onRevealFieldRequested: function (field) {
                            root.storeAdapter.controller.toggleField(field);
                        }
                        onQrRequested: root.storeAdapter.controller.showQr()
                        onRevealToggled: root.revealToggleRequested()
                        onCopyUsernameRequested: root.copyUsernameRequested()
                        onCopyPasswordRequested: root.copyPasswordRequested()
                        onCopyTotpRequested: function (next) {
                            root.copyTotpRequested(next);
                        }
                    }

                    Controls.Frame {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: root.hasRelatedEntryReference || (!!root.selected && !root.selected.trashed)
                        padding: Kirigami.Units.largeSpacing
                        contentItem: ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Controls.ItemDelegate {
                                id: relatedEntryCard
                                objectName: "relatedEntryCard"
                                Layout.fillWidth: true
                                visible: !!root.relatedEntry
                                enabled: !root.relatedEntry?.trashed && !root.storeAdapter.controller.busy
                                text: ""
                                Accessible.name: root.relatedEntry?.trashed ? qsTr("Elemento vinculado %1, en la papelera").arg(root.relatedEntry.title) : qsTr("Abrir elemento vinculado %1").arg(root.relatedEntry?.title || "")
                                Accessible.description: root.relatedEntry ? qsTr("Elemento vinculado · %1").arg(root.storeAdapter.kindName(root.relatedEntry.kind)) : ""
                                contentItem: RowLayout {
                                    spacing: Kirigami.Units.largeSpacing
                                    EntryIcon {
                                        entryIcon: root.relatedEntry?.icon || null
                                        imageSource: root.relatedEntry?.iconSource || ""
                                        fallbackIcon: root.storeAdapter.kindIcon(root.relatedEntry?.kind || "custom")
                                        selected: relatedEntryCard.highlighted || relatedEntryCard.down
                                        Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                                        Layout.preferredHeight: width
                                    }
                                    Kirigami.TitleSubtitle {
                                        Layout.fillWidth: true
                                        Layout.minimumWidth: 0
                                        textFormat: Text.PlainText
                                        title: root.relatedEntry?.title || ""
                                        selected: relatedEntryCard.highlighted || relatedEntryCard.down
                                        font: relatedEntryCard.font
                                        subtitleFont: relatedEntryCard.font
                                        subtitle: root.relatedEntry?.trashed ? qsTr("Elemento vinculado · %1 · En la papelera").arg(root.storeAdapter.kindName(root.relatedEntry.kind)) : qsTr("Elemento vinculado · %1").arg(root.storeAdapter.kindName(root.relatedEntry?.kind || "custom"))
                                    }
                                    Kirigami.Icon {
                                        visible: !root.relatedEntry?.trashed
                                        source: "go-next"
                                        Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                                        Layout.preferredHeight: width
                                    }
                                }
                                onClicked: {
                                    if (root.relatedEntry && !root.relatedEntry.trashed)
                                        root.relatedEntryOpenRequested(root.relatedEntry);
                                }
                            }

                            Controls.Button {
                                Layout.alignment: Qt.AlignRight
                                visible: !!root.relatedEntry?.trashed
                                text: qsTr("Ver en la papelera")
                                icon.name: "user-trash"
                                onClicked: root.relatedEntryTrashRequested(root.relatedEntry)
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                visible: !!root.selected && !root.selected.trashed && !root.hasRelatedEntryReference
                                spacing: Kirigami.Units.largeSpacing
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Layout.minimumWidth: 0
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: root.storeAdapter.controller.locked ? qsTr("Desbloquea la bóveda para crear un elemento vinculado.") : qsTr("Crea un secreto personalizado para acompañar este elemento.")
                                        textFormat: Text.PlainText
                                        wrapMode: Text.Wrap
                                        color: Kirigami.Theme.disabledTextColor
                                    }
                                }
                                Controls.Button {
                                    objectName: "createSecurityButton"
                                    text: qsTr("Crear seguridad")
                                    icon.name: "list-add"
                                    enabled: !root.storeAdapter.controller.locked && !root.storeAdapter.controller.busy && !root.storeAdapter.controller.editorBusy
                                    onClicked: root.createSecurityRequested(root.selected)
                                }
                            }

                            Kirigami.InlineMessage {
                                Layout.fillWidth: true
                                visible: root.hasRelatedEntryReference && !root.relatedEntry
                                type: Kirigami.MessageType.Warning
                                text: qsTr("El elemento vinculado no está disponible. Puedes cambiar o quitar el vínculo al editar este elemento.")
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
        anchors.verticalCenterOffset: root.headerHeight / 2
        visible: !root.editing && !root.keysMode && !root.selected
        icon.name: "document-encrypt"
        text: qsTr("Ningún elemento seleccionado")
        explanation: qsTr("Selecciona un elemento de la lista para ver su contenido.")
    }
}
