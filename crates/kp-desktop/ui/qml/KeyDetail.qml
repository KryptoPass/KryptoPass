pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: root
    required property var selectedKey
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    signal notificationRequested(string message)
    property string displayedId: ""
    readonly property bool available: !keyStore.controller.busy
    spacing: Kirigami.Units.smallSpacing
    readonly property IdentityWording wording: keyStore.wording
    readonly property bool own: !!selectedKey?.own
    readonly property bool compact: width < Kirigami.Units.gridUnit * 34
    readonly property var sections: [KI18n.i18nc("@title:tab", "General and Access"), KI18n.i18nc("@title:tab", "Security and Recovery"), KI18n.i18nc("@title:tab", "Servers and Signatures"), KI18n.i18nc("@title:tab", "KDE Contacts")]
    onSelectedKeyChanged: {
        if (displayedId !== (selectedKey?.id || "")) {
            tabs.currentIndex = 0;
            displayedId = selectedKey?.id || "";
        }
    }
    function copyFingerprint() {
        if (!selectedKey?.id)
            return;
        clipboardSource.selectAll();
        clipboardSource.copy();
        clipboardSource.deselect();
        notificationRequested(KI18n.i18n("Copied to clipboard"));
    }
    TextEdit {
        id: clipboardSource
        visible: false
        readOnly: true
        textFormat: TextEdit.PlainText
        text: root.selectedKey?.id || ""
    }
    IdentityEditor {
        id: identityEditor
        keyStore: root.keyStore
    }
    RowLayout {
        Layout.fillWidth: true
        Layout.margins: Kirigami.Units.largeSpacing
        Layout.bottomMargin: 0
        spacing: Kirigami.Units.largeSpacing
        KeyAvatar {
            source: root.selectedKey?.photoSource || ""
            Layout.preferredWidth: Kirigami.Units.iconSizes.huge
            Layout.preferredHeight: Kirigami.Units.iconSizes.huge
            Layout.alignment: Qt.AlignTop
            size: Kirigami.Units.iconSizes.huge
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            spacing: 0
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Kirigami.Heading {
                    Layout.maximumWidth: parent.width - editButton.width - Kirigami.Units.smallSpacing
                    level: 2
                    text: root.selectedKey?.title || ""
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.ToolButton {
                    id: editButton
                    icon.name: "document-edit"
                    display: Controls.AbstractButton.IconOnly
                    text: KI18n.i18nc("@action:button", "Edit Alias in KryptoPass")
                    enabled: root.available
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.ToolTip.text: text
                    onClicked: identityEditor.edit(root.selectedKey)
                }
                Item {
                    Layout.fillWidth: true
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                text: (root.selectedKey?.pgpTitle || "") + (root.selectedKey?.email ? " <" + root.selectedKey.email + ">" : "")
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Kirigami.Icon {
                    visible: root.wording.needsAttention(root.selectedKey)
                    source: "emblem-warning"
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                    Layout.preferredHeight: Kirigami.Units.iconSizes.small
                    Accessible.ignored: true
                }
                Controls.Label {
                    text: (root.selectedKey?.status || "") + " · " + root.wording.expiry(root.selectedKey?.subkeys[0]?.expiresAt)
                    textFormat: Text.PlainText
                    opacity: 0.75
                }
                Kirigami.LinkButton {
                    visible: !!root.selectedKey?.hasSecret
                    text: KI18n.i18nc("@action:button", "Change")
                    enabled: root.available
                    onClicked: root.operationDialog.openFor("expiry", root.selectedKey.id)
                }
                Item {
                    Layout.fillWidth: true
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 0
                Controls.Label {
                    Layout.maximumWidth: parent.width - copyFingerprintButton.width
                    text: root.wording.groupedFingerprint(root.selectedKey?.id)
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    opacity: 0.75
                    Accessible.name: KI18n.i18n("Key fingerprint")
                }
                Controls.ToolButton {
                    id: copyFingerprintButton
                    icon.name: "edit-copy"
                    display: Controls.AbstractButton.IconOnly
                    text: KI18n.i18nc("@action:button", "Copy Fingerprint")
                    implicitWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
                    implicitHeight: implicitWidth
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.ToolTip.text: text + ". " + root.wording.tips.fingerprint
                    onClicked: root.copyFingerprint()
                }
                Item {
                    Layout.fillWidth: true
                }
            }
        }
        RowLayout {
            Layout.alignment: Qt.AlignTop
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                text: KI18n.i18nc("@action", "Copy Public Key")
                icon.name: "edit-copy"
                display: root.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                enabled: root.available
                Controls.ToolTip.visible: root.compact && (hovered || visualFocus)
                Controls.ToolTip.text: text
                onClicked: root.operationDialog.openFor("copyPublic", root.selectedKey.id)
            }
            Controls.Button {
                id: exportButton
                text: KI18n.i18nc("@action:button", "Export…")
                icon.name: "document-save-as"
                display: root.compact ? Controls.AbstractButton.IconOnly : Controls.AbstractButton.TextBesideIcon
                enabled: root.available
                Controls.ToolTip.visible: root.compact && (hovered || visualFocus)
                Controls.ToolTip.text: text
                onClicked: exportMenu.popup(exportButton, 0, exportButton.height)
                Controls.Menu {
                    id: exportMenu
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Export Public Key (.asc)…")
                        icon.name: "document-save-as"
                        onTriggered: root.operationDialog.openFor("exportPublic", root.selectedKey.id)
                    }
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Back Up Private Key (.asc)…")
                        icon.name: "document-encrypted"
                        enabled: !!root.selectedKey?.hasSecret
                        onTriggered: root.operationDialog.openFor("backupPrivate", root.selectedKey.id)
                    }
                }
            }
        }
    }
    Controls.ComboBox {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.largeSpacing
        Layout.rightMargin: Kirigami.Units.largeSpacing
        visible: !!root.keyStore.selectedRow?.isPerson && root.keyStore.selectedRow.identities.length > 1
        model: root.keyStore.selectedRow?.isPerson ? root.keyStore.selectedRow.identities : []
        textRole: "title"
        Accessible.name: KI18n.i18n("This person's keys")
        currentIndex: root.keyStore.selectedRow?.isPerson ? root.keyStore.selectedRow.identities.findIndex(key => key.id === root.selectedKey?.id) : -1
        onActivated: index => root.keyStore.selectedIdentityId = root.keyStore.selectedRow.identities[index].id
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.largeSpacing
        Layout.rightMargin: Kirigami.Units.largeSpacing
        visible: !!root.keyStore.controller.catalogError || !!root.keyStore.controller.operationError
        type: Kirigami.MessageType.Error
        text: root.keyStore.controller.catalogError || root.keyStore.controller.operationError
    }
    Controls.ComboBox {
        Layout.fillWidth: true
        Layout.leftMargin: Kirigami.Units.largeSpacing
        Layout.rightMargin: Kirigami.Units.largeSpacing
        visible: root.width < Kirigami.Units.gridUnit * 30
        model: root.sections
        currentIndex: tabs.currentIndex
        Accessible.name: KI18n.i18n("Key section")
        onActivated: index => tabs.currentIndex = index
    }
    Controls.TabBar {
        id: tabs
        visible: root.width >= Kirigami.Units.gridUnit * 30
        Layout.fillWidth: true
        Repeater {
            model: root.sections
            delegate: Controls.TabButton {
                required property string modelData
                text: modelData
            }
        }
    }
    IdentityKeys {
        visible: tabs.currentIndex === 0
        Layout.fillWidth: true
        Layout.fillHeight: true
        selectedKey: root.selectedKey
        keyStore: root.keyStore
        operationDialog: root.operationDialog
        onOrganizeRequested: identityEditor.edit(root.selectedKey)
        onNotificationRequested: message => root.notificationRequested(message)
    }
    IdentitySecurity {
        visible: tabs.currentIndex === 1
        Layout.fillWidth: true
        Layout.fillHeight: true
        selectedKey: root.selectedKey
        keyStore: root.keyStore
        operationDialog: root.operationDialog
    }
    IdentityServers {
        visible: tabs.currentIndex === 2
        Layout.fillWidth: true
        Layout.fillHeight: true
        selectedKey: root.selectedKey
        keyStore: root.keyStore
        operationDialog: root.operationDialog
    }
    Controls.ScrollView {
        id: contactsScroll
        visible: tabs.currentIndex === 3
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true
        KeyContactPanel {
            width: Math.min(contactsScroll.availableWidth - 48, 820)
            x: 24
            y: 20
            selectedKey: root.selectedKey
            keyStore: root.keyStore
        }
    }
}
