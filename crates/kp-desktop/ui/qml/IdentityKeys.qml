pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

/// KryptoPass settings of the identity, its real keys (primary and subkeys, once each)
/// and its user IDs.
Controls.ScrollView {
    id: root
    required property var selectedKey
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    signal organizeRequested
    signal notificationRequested(string message)
    readonly property IdentityWording wording: keyStore.wording
    readonly property bool available: !keyStore.controller.busy
    readonly property bool own: !!selectedKey?.own
    readonly property bool hasSecret: !!selectedKey?.hasSecret
    readonly property bool isVaultKey: keyStore.isPrimary(selectedKey?.id)
    readonly property var verification: wording.item(selectedKey, ["verified", "verificationChanged", "unverified"])
    readonly property var userIds: (selectedKey?.identities || []).slice().sort((a, b) => Number(b.primary) - Number(a.primary) || Number(a.revoked || a.invalid) - Number(b.revoked || b.invalid))
    readonly property bool hasCertifier: keyStore.entries.some(key => key.hasSecret && key.canCertify && key.id !== selectedKey?.id)
    /// Subkey fingerprints before "add subkey", to label the new one afterwards.
    property var knownSubkeys: null
    property string addingTo: ""
    /// Same width for the two "add" buttons so their sections line up.
    readonly property real addButtonWidth: Kirigami.Units.gridUnit * 11
    contentWidth: availableWidth
    clip: true

    function addSubkey() {
        knownSubkeys = selectedKey.subkeys.map(key => key.fingerprint);
        addingTo = selectedKey.id;
        operationDialog.openFor("addSubkey", selectedKey.id);
    }
    function setLabel(fingerprint, target, label) {
        keyStore.editCatalog({
            action: "setLabel",
            fingerprint,
            target,
            label
        });
    }
    Connections {
        target: root.operationDialog
        function onClosed() {
            if (!root.knownSubkeys)
                return;
            const known = root.knownSubkeys;
            const label = root.operationDialog.subkeyLabel.trim();
            root.knownSubkeys = null;
            const key = root.keyStore.entries.find(entry => entry.id === root.addingTo);
            const created = (key?.subkeys || []).find(subkey => !!subkey.fingerprint && !known.includes(subkey.fingerprint));
            if (created && label && root.operationDialog.completedFingerprint)
                root.setLabel(key.id, created.fingerprint, label);
        }
    }
    Connections {
        target: root.keyStore.controller
        function onSshPublicKeyReady(publicKey) {
            root.copyText(publicKey);
        }
        function onCatalogSaved() {
            if (labelDialog.saving) {
                labelDialog.saving = false;
                labelDialog.close();
            }
        }
        function onOperationFailed() {
            labelDialog.saving = false;
        }
    }
    function copyText(text) {
        clipboard.text = text;
        clipboard.selectAll();
        clipboard.copy();
        clipboard.deselect();
        clipboard.text = "";
        notificationRequested(KI18n.i18n("Copied to clipboard"));
    }
    TextEdit {
        id: clipboard
        visible: false
        textFormat: TextEdit.PlainText
    }
    Controls.Dialog {
        id: labelDialog
        property string target: ""
        property bool saving: false
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 24)
        modal: true
        title: KI18n.i18nc("@title:window", "Key Label")
        closePolicy: Controls.Popup.NoAutoClose
        footer: Controls.DialogButtonBox {
            standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
            onAccepted: labelDialog.save()
            onRejected: labelDialog.close()
        }
        onOpened: labelField.forceActiveFocus()
        function edit(component) {
            target = component.fingerprint;
            labelField.text = component.label;
            saving = false;
            root.keyStore.controller.clearOperationError();
            open();
        }
        function save() {
            if (root.keyStore.controller.busy)
                return;
            saving = true;
            root.setLabel(root.selectedKey.id, target, labelField.text);
        }
        contentItem: ColumnLayout {
            Controls.TextField {
                id: labelField
                Layout.fillWidth: true
                maximumLength: 160
                placeholderText: KI18n.i18nc("@info:placeholder", "For example: GitHub SSH")
                Accessible.name: KI18n.i18nc("@title:window", "Key Label")
                onAccepted: labelDialog.save()
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!root.keyStore.controller.operationError
                type: Kirigami.MessageType.Error
                text: root.keyStore.controller.operationError
            }
        }
    }
    Controls.Dialog {
        id: verifyDialog
        property string fingerprint: ""
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 26)
        modal: true
        title: KI18n.i18nc("@title:window", "Verify Fingerprint")
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onAccepted: root.keyStore.editCatalog({
            action: "verify",
            fingerprint,
            verified: true
        })
        contentItem: ColumnLayout {
            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WrapAnywhere
                font: Kirigami.Theme.fixedWidthFont
                text: root.wording.groupedFingerprint(verifyDialog.fingerprint)
            }
            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: KI18n.i18n("Accept only if it fully matches the one its owner gives you.")
            }
        }
    }

    component Chip: Rectangle {
        required property string text
        required property string role
        readonly property color tone: ({
                certify: Kirigami.Theme.neutralTextColor,
                sign: Kirigami.Theme.positiveTextColor,
                encrypt: Kirigami.Theme.linkColor,
                authenticate: Kirigami.Theme.visitedLinkColor,
                primary: Kirigami.Theme.highlightColor,
                muted: Kirigami.Theme.disabledTextColor
            })[role] || Kirigami.Theme.textColor
        implicitWidth: chipLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        implicitHeight: chipLabel.implicitHeight + 4
        radius: Kirigami.Units.cornerRadius
        color: Qt.rgba(tone.r, tone.g, tone.b, 0.15)
        border.color: Qt.rgba(tone.r, tone.g, tone.b, 0.5)
        Controls.Label {
            id: chipLabel
            anchors.centerIn: parent
            text: parent.text
            font: Kirigami.Theme.smallFont
        }
    }

    ColumnLayout {
        // Rows stay readable on wide windows: controls never drift far from their label.
        width: Math.min(root.availableWidth - 48, 820)
        x: 24
        spacing: 12
        Item {
            implicitHeight: 8
        }
        Kirigami.Heading {
            level: 4
            text: KI18n.i18nc("@title", "KryptoPass Settings")
        }
        IdentityRow {
            visible: root.own
            indicator: "folder-locked"
            title: KI18n.i18nc("@title review check", "Vault's main key")
            help: root.wording.tips.vaultKey
            Chip {
                visible: root.isVaultKey
                text: KI18n.i18n("Active as vault key")
                role: "sign"
            }
            Controls.Button {
                visible: !root.isVaultKey
                text: KI18n.i18nc("@action:button", "Use as Vault Key")
                enabled: root.available && root.hasSecret && !!root.selectedKey?.canEncrypt
                onClicked: root.keyStore.primaryKeyRequested(root.selectedKey.id)
            }
        }
        IdentityRow {
            visible: root.own
            indicator: "dialog-password"
            title: KI18n.i18n("Forget passphrase when the vault locks (gpg-agent)")
            help: root.wording.tips.forgetOnLock
            Controls.Switch {
                Accessible.name: KI18n.i18n("Forget passphrase when the vault locks")
                checked: !!root.selectedKey?.meta.forgetOnLock || (root.isVaultKey && root.keyStore.forgetOnLock)
                enabled: root.available && root.hasSecret
                onToggled: {
                    if (root.isVaultKey)
                        root.keyStore.forgetOnLockRequested(checked);
                    root.keyStore.editCatalog({
                        action: "forget",
                        fingerprint: root.selectedKey.id,
                        enabled: checked
                    });
                }
            }
        }
        IdentityRow {
            visible: !root.own
            indicator: "user-identity"
            title: KI18n.i18nc("@title", "Person")
            status: root.selectedKey?.personName || KI18n.i18n("Unassigned")
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Organize…")
                enabled: root.available
                onClicked: root.organizeRequested()
            }
        }
        IdentityRow {
            visible: !root.own
            indicator: root.verification?.code === "verified" ? "security-high" : "security-medium"
            level: root.verification?.level || "pending"
            title: KI18n.i18nc("@title", "Fingerprint verification")
            help: root.wording.tips.verification
            status: root.wording.health(root.verification)
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Verify…")
                visible: root.verification?.code !== "verified"
                enabled: root.available && root.selectedKey?.statusCode === "valid"
                onClicked: {
                    verifyDialog.fingerprint = root.selectedKey.id;
                    verifyDialog.open();
                }
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Remove Verification")
                visible: !!root.selectedKey?.meta.verifiedAt
                enabled: root.available
                onClicked: root.keyStore.editCatalog({
                    action: "verify",
                    fingerprint: root.selectedKey.id,
                    verified: false
                })
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: 8
            Layout.bottomMargin: 8
        }
        RowLayout {
            Layout.fillWidth: true
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: KI18n.i18nc("@title:group", "Keys and subkeys")
            }
            Controls.Button {
                visible: root.hasSecret
                Layout.preferredWidth: root.addButtonWidth
                text: KI18n.i18nc("@action:button", "Add Subkey…")
                icon.name: "list-add"
                enabled: root.available && root.selectedKey?.statusCode === "valid"
                onClicked: root.addSubkey()
            }
        }
        Repeater {
            model: root.selectedKey?.subkeys || []
            delegate: RowLayout {
                id: keyRow
                required property var modelData
                required property int index
                readonly property bool primary: modelData.isPrimary || index === 0
                readonly property bool usable: root.selectedKey?.statusCode === "valid" && modelData.statusCode === "valid"
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.bottomMargin: 4
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: keyRow.primary ? "view-certificate" : "dialog-password"
                    Layout.preferredWidth: 20
                    Layout.preferredHeight: 20
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 2
                    Accessible.ignored: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            text: keyRow.modelData.label || (keyRow.primary ? KI18n.i18n("Primary key") : KI18n.i18n("Subkey #%1", keyRow.index))
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            font.weight: Font.DemiBold
                            Layout.maximumWidth: keyRow.width / 3
                            Layout.rightMargin: Kirigami.Units.smallSpacing
                        }
                        Repeater {
                            model: root.wording.chips(keyRow.modelData)
                            delegate: Chip {
                                required property var modelData
                                text: modelData.text
                                role: modelData.role
                            }
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: [keyRow.modelData.algorithm, root.wording.groupedFingerprint(keyRow.modelData.fingerprint.slice(-16)), keyRow.modelData.status, root.wording.expiry(keyRow.modelData.expiresAt), root.wording.location(keyRow.modelData.location)].filter(part => !!part).join(" · ")
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        color: keyRow.modelData.statusCode === "valid" ? Kirigami.Theme.textColor : Kirigami.Theme.neutralTextColor
                        opacity: keyRow.modelData.statusCode === "valid" ? 0.75 : 1
                    }
                }
                Controls.ToolButton {
                    id: moreButton
                    Layout.alignment: Qt.AlignVCenter
                    icon.name: "overflow-menu"
                    display: Controls.AbstractButton.IconOnly
                    text: KI18n.i18nc("@action:button", "Key Actions")
                    enabled: root.available && !!keyRow.modelData.fingerprint
                    onClicked: moreMenu.popup(moreButton, 0, moreButton.height)
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.ToolTip.text: text
                    Controls.Menu {
                        id: moreMenu
                        Controls.MenuItem {
                            visible: !!keyRow.modelData.canAuthenticate
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Copy SSH Public Key")
                            icon.name: "edit-copy"
                            enabled: keyRow.usable
                            onTriggered: root.keyStore.controller.copySshPublicKey(root.selectedKey.id, keyRow.modelData.fingerprint)
                        }
                        Controls.MenuItem {
                            text: keyRow.primary ? KI18n.i18nc("@action:button", "Copy Fingerprint") : KI18n.i18nc("@action:inmenu", "Copy Subkey Fingerprint")
                            icon.name: "edit-copy"
                            onTriggered: root.copyText(keyRow.modelData.fingerprint)
                        }
                        Controls.MenuItem {
                            text: KI18n.i18nc("@action:inmenu", "Change Label…")
                            icon.name: "tag"
                            onTriggered: labelDialog.edit(keyRow.modelData)
                        }
                        Controls.MenuItem {
                            visible: root.hasSecret
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Change Expiry…")
                            icon.name: "appointment-new"
                            onTriggered: root.operationDialog.openFor("expiry", root.selectedKey.id, "", keyRow.primary ? "" : keyRow.modelData.fingerprint)
                        }
                        Controls.MenuSeparator {
                            visible: revokeItem.visible
                            height: visible ? implicitHeight : 0
                        }
                        Controls.MenuItem {
                            id: revokeItem
                            visible: root.hasSecret && !keyRow.primary
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Revoke Subkey…")
                            icon.name: "dialog-cancel"
                            enabled: keyRow.modelData.statusCode !== "revoked"
                            onTriggered: root.operationDialog.openFor("revokeSubkey", root.selectedKey.id, "", keyRow.modelData.fingerprint)
                        }
                    }
                }
            }
        }

        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: 8
            Layout.bottomMargin: 8
        }
        RowLayout {
            Layout.fillWidth: true
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: KI18n.i18nc("@title:group", "Identities (UID)")
            }
            Controls.Button {
                visible: root.hasSecret
                Layout.preferredWidth: root.addButtonWidth
                text: KI18n.i18nc("@action:button", "Add Identity…")
                icon.name: "list-add"
                enabled: root.available
                onClicked: root.operationDialog.openFor("addIdentity", root.selectedKey.id)
            }
        }
        Repeater {
            model: root.userIds
            delegate: RowLayout {
                id: uidRow
                required property var modelData
                readonly property bool usable: !modelData.revoked && !modelData.invalid
                readonly property bool trusted: ["ultimate", "full"].includes(modelData.validityCode)
                readonly property bool canPromote: root.hasSecret && !modelData.primary && usable
                readonly property bool canRevoke: root.hasSecret && !modelData.revoked
                readonly property bool canCertify: !root.hasSecret && root.hasCertifier && usable
                // Deferred like IdentityRow.titleCap to avoid a recursive rearrange.
                property real idCap: Number.POSITIVE_INFINITY
                onWidthChanged: Qt.callLater(uidRow.updateIdCap)
                function updateIdCap() {
                    uidRow.idCap = uidRow.width > 0 ? uidRow.width * 0.7 : Number.POSITIVE_INFINITY;
                }
                Layout.fillWidth: true
                Layout.topMargin: 4
                Layout.bottomMargin: 4
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "user-identity"
                    Layout.preferredWidth: 20
                    Layout.preferredHeight: 20
                    Layout.alignment: Qt.AlignTop
                    Layout.topMargin: 2
                    Accessible.ignored: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 4
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            Layout.maximumWidth: uidRow.idCap
                            Layout.rightMargin: Kirigami.Units.smallSpacing
                            text: uidRow.modelData.id
                            textFormat: Text.PlainText
                            elide: Text.ElideMiddle
                            font.weight: Font.DemiBold
                            opacity: uidRow.usable ? 1 : 0.6
                        }
                        Chip {
                            visible: !!uidRow.modelData.primary
                            text: KI18n.i18nc("@label badge identity", "Primary")
                            role: "primary"
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Chip {
                            text: "● " + (uidRow.modelData.validityCode === "ultimate" && root.selectedKey?.ownerTrust === "ultimate" ? KI18n.i18n("Validity and trust: Ultimate (own key)") : KI18n.i18n("GnuPG validity: %1", root.wording.validity(uidRow.modelData.validityCode)))
                            role: uidRow.trusted ? "sign" : uidRow.modelData.validityCode === "marginal" ? "certify" : "muted"
                        }
                        HelpButton {
                            help: root.wording.tips.validity
                        }
                        Item {
                            Layout.fillWidth: true
                        }
                    }
                }
                Controls.ToolButton {
                    id: uidButton
                    Layout.alignment: Qt.AlignVCenter
                    visible: uidRow.canPromote || uidRow.canRevoke || uidRow.canCertify
                    icon.name: "overflow-menu"
                    display: Controls.AbstractButton.IconOnly
                    text: KI18n.i18nc("@action:button", "Identity Actions")
                    enabled: root.available
                    onClicked: uidMenu.popup(uidButton, 0, uidButton.height)
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.ToolTip.text: text
                    Controls.Menu {
                        id: uidMenu
                        Controls.MenuItem {
                            visible: root.hasSecret
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Use as Primary Identity")
                            icon.name: "favorite"
                            enabled: uidRow.canPromote
                            onTriggered: root.operationDialog.openFor("primaryIdentity", root.selectedKey.id, uidRow.modelData.id)
                        }
                        Controls.MenuItem {
                            visible: uidRow.canCertify
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Certify…")
                            icon.name: "dialog-ok"
                            onTriggered: root.operationDialog.openFor("certify", root.selectedKey.id, uidRow.modelData.id, "", [uidRow.modelData.id])
                        }
                        Controls.MenuItem {
                            visible: root.hasSecret
                            height: visible ? implicitHeight : 0
                            text: KI18n.i18nc("@action:inmenu", "Revoke Identity…")
                            icon.name: "dialog-cancel"
                            enabled: uidRow.canRevoke
                            onTriggered: root.operationDialog.openFor("revokeIdentity", root.selectedKey.id, uidRow.modelData.id)
                        }
                    }
                }
            }
        }
        Item {
            implicitHeight: 8
        }
    }
}
