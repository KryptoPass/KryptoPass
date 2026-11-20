pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Pane {
    id: root

    required property bool keysMode
    required property SecretStore storeAdapter
    required property KeyStore keyStore
    required property KeyOperationDialog keyOperationDialog
    required property bool revealed
    required property real headerHeight
    required property Controls.Action editAction
    required property Controls.Action moveAction
    required property Controls.Action trashAction
    required property Controls.Action favoriteAction
    required property Controls.Action restoreAction
    required property Controls.Action deleteAction
    required property Controls.Action copyFingerprintAction

    property bool editing: false
    property bool listEmpty: false
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
    signal createRelatedEntryRequested(var entry)

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
                    text: root.keysMode ? (root.keyStore.selected && !(root.keyStore.scope === "contacts" && root.keyStore.contactsOverviewVisible) ? KI18n.i18nc("@title", "Identity") : root.keyStore.scope === "contacts" ? KI18n.i18nc("@title key scope", "People") : KI18n.i18nc("@title", "Identities")) : KI18n.i18nc("@title", "Details")
                }
                Controls.ToolButton {
                    visible: root.keysMode && root.keyStore.scope === "contacts"
                    text: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? KI18n.i18nc("@action:button", "Back to Key") : KI18n.i18nc("@action:button", "Manage People")
                    icon.name: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? "view-certificate" : "view-pim-contacts"
                    Accessible.name: root.keyStore.selected && root.keyStore.contactsOverviewVisible ? KI18n.i18n("Back to the key details") : KI18n.i18n("View the address book")
                    onClicked: root.keyStore.contactsOverviewVisible = root.keyStore.selected ? !root.keyStore.contactsOverviewVisible : true
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                }
                DetailActionBar {
                    visible: !root.keysMode && !!root.selected
                    editAction: root.editAction
                    favoriteAction: root.favoriteAction
                    moveAction: root.moveAction
                    trashAction: root.trashAction
                    restoreAction: root.restoreAction
                    deleteAction: root.deleteAction
                    trashed: !!root.selected?.trashed
                    hasRelatedEntry: root.hasRelatedEntryReference
                    locked: root.storeAdapter.controller.locked
                    busy: root.storeAdapter.controller.busy
                    editorBusy: root.storeAdapter.controller.editorBusy
                    onCreateRelatedEntryRequested: root.createRelatedEntryRequested(root.selected)
                }
                Controls.ToolButton {
                    action: root.copyFingerprintAction
                    visible: false
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

        ColumnLayout {
            visible: root.keysMode && !root.editing && !root.keyStore.selected && root.keyStore.scope !== "contacts"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Controls.Button {
                Layout.margins: Kirigami.Units.largeSpacing
                text: KI18n.i18nc("@action:button", "Add Identity…")
                enabled: !root.keyStore.controller.busy
                onClicked: emptyIdentityDialog.start()
            }
            Item {
                Layout.fillHeight: true
            }
        }
        IdentityAddDialog {
            id: emptyIdentityDialog
            keyStore: root.keyStore
            operationDialog: root.keyOperationDialog
        }

        KeyContactsOverview {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: Kirigami.Units.largeSpacing
            visible: root.keysMode && !root.editing && root.keyStore.scope === "contacts" && (!root.keyStore.selected || root.keyStore.contactsOverviewVisible)
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
                                text: KI18n.i18nc("@label two parts of a subtitle", "%1 · %2", root.storeAdapter.folderName(root.selected?.folder || ""), root.storeAdapter.kindName(root.selected?.kind || "note"))
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
                        text: KI18n.i18n("The vault is locked. Unlock it to open this item.")
                    }

                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: !root.selectedDetail && root.storeAdapter.controller.detailBusy
                        type: Kirigami.MessageType.Information
                        text: KI18n.i18nc("@info:progress", "Decrypting the item's content…")
                    }

                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: !!root.selected && root.selected.conflicted === true
                        type: Kirigami.MessageType.Warning
                        text: KI18n.i18n("This item was edited on two devices at the same time. Until you choose what to keep, this computer's version is shown.")
                        actions: [
                            Kirigami.Action {
                                text: KI18n.i18nc("@action:button", "Review Differences…")
                                icon.name: "vcs-merge"
                                enabled: !root.storeAdapter.controller.busy && !root.storeAdapter.controller.locked
                                onTriggered: root.storeAdapter.controller.openConflict(root.selected.id)
                            }
                        ]
                    }

                    // Why an item cannot be opened comes from its recipients: "foreign" only
                    // when none of its keys is on this device.
                    Kirigami.InlineMessage {
                        Layout.fillWidth: true
                        Layout.margins: Kirigami.Units.largeSpacing
                        visible: !!root.selected && root.selected.readable === false
                        type: root.selected?.unreadableReason === "foreign" ? Kirigami.MessageType.Information : Kirigami.MessageType.Warning
                        text: root.selected?.unreadableReason === "foreign" ? KI18n.i18n("This item is encrypted for a key you do not have on this device. It is still stored and synced, and you can open it if you import that key.") : KI18n.i18n("This item is encrypted for a key on this device, but it could not be opened yet. It is retried when the key is unlocked; if it stays like this, it is a bug and is recorded in the log.")
                    }

                    ConflictDialog {
                        storeAdapter: root.storeAdapter
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
                        onVersionCopyUrlRequested: function (index) {
                            root.storeAdapter.controller.copyHistoryUrl(index);
                        }
                        onVersionRevealRequested: root.storeAdapter.controller.toggleHistoryPassword()
                        onActivityExportRequested: root.storeAdapter.controller.exportActivity()
                        onCopyFieldRequested: function (field) {
                            root.storeAdapter.controller.copyField(field);
                        }
                        onCopyUrlRequested: function (index) {
                            root.storeAdapter.controller.copyUrl(index);
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
                        visible: root.hasRelatedEntryReference
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
                                Accessible.name: root.relatedEntry?.trashed ? KI18n.i18n("Linked item %1, in the trash", root.relatedEntry.title) : KI18n.i18n("Open linked item %1", root.relatedEntry?.title || "")
                                Accessible.description: root.relatedEntry ? KI18n.i18nc("@label %1 is the item type", "Linked item · %1", root.storeAdapter.kindName(root.relatedEntry.kind)) : ""
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
                                        subtitle: root.relatedEntry?.trashed ? KI18n.i18nc("@label %1 is the item type", "Linked item · %1 · In the trash", root.storeAdapter.kindName(root.relatedEntry.kind)) : KI18n.i18nc("@label %1 is the item type", "Linked item · %1", root.storeAdapter.kindName(root.relatedEntry?.kind || "custom"))
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
                                text: KI18n.i18nc("@action:button", "View in Trash")
                                icon.name: "user-trash"
                                onClicked: root.relatedEntryTrashRequested(root.relatedEntry)
                            }

                            Kirigami.InlineMessage {
                                Layout.fillWidth: true
                                visible: root.hasRelatedEntryReference && !root.relatedEntry
                                type: Kirigami.MessageType.Warning
                                text: KI18n.i18n("The linked item is not available. You can change or remove the link when you edit this item.")
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
        visible: !root.editing && !root.keysMode && !root.selected && !root.listEmpty
        icon.name: "view-list-details"
        text: KI18n.i18nc("@title placeholder", "No item selected")
        explanation: KI18n.i18n("Select an item from the list to see its content.")
    }
}
