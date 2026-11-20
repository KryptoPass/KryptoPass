pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

/// Backup, passphrase and revocation as a one-line checklist, then the destructive actions.
Controls.ScrollView {
    id: root
    required property var selectedKey
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    readonly property IdentityWording wording: keyStore.wording
    readonly property bool available: !keyStore.controller.busy
    readonly property bool hasSecret: !!selectedKey?.hasSecret
    readonly property var backup: wording.item(selectedKey, ["backupSaved", "noBackup"])
    readonly property var revocation: wording.item(selectedKey, ["emergencySaved", "noEmergency"])
    /// Shared width of the action column, so the states line up.
    readonly property real buttonWidth: Kirigami.Units.gridUnit * 13
    contentWidth: availableWidth
    clip: true

    ColumnLayout {
        // Rows stay readable on wide windows: controls never drift far from their label.
        width: Math.min(root.availableWidth - 48, 820)
        x: 24
        spacing: 12
        Item {
            implicitHeight: 8
        }
        IdentityRow {
            visible: !!root.selectedKey?.needsOwnTrust
            level: "warning"
            indicator: "security-low"
            title: KI18n.i18nc("@title review check", "Trust in GnuPG")
            help: root.wording.tips.ownerTrust
            status: KI18n.i18nc("@info review status", "No ultimate trust")
            Controls.Button {
                Layout.preferredWidth: root.buttonWidth
                text: KI18n.i18nc("@action:button", "Confirm Trust…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("markAsOwn", root.selectedKey.id)
            }
        }
        IdentityRow {
            visible: root.hasSecret
            level: root.backup?.level || "pending"
            indicator: "document-export"
            title: KI18n.i18nc("@title", "Private key backup")
            help: root.wording.tips.backup
            status: root.wording.health(root.backup)
            Controls.Button {
                Layout.preferredWidth: root.buttonWidth
                text: KI18n.i18nc("@action:button", "Export Private Copy…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("backupPrivate", root.selectedKey.id)
            }
        }
        IdentityRow {
            visible: root.hasSecret
            indicator: "object-locked"
            title: KI18n.i18nc("@title", "Key passphrase")
            Controls.Button {
                Layout.preferredWidth: root.buttonWidth
                text: KI18n.i18nc("@action:button", "Change Passphrase…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("passphrase", root.selectedKey.id)
            }
        }
        IdentityRow {
            level: root.hasSecret ? root.revocation?.level || "pending" : ""
            indicator: "security-medium"
            title: KI18n.i18nc("@title", "Revocation certificate (emergency plan)")
            help: root.wording.tips.revocation
            status: root.hasSecret ? root.wording.health(root.revocation) : ""
            Controls.Button {
                Layout.preferredWidth: (root.buttonWidth - Kirigami.Units.smallSpacing) / 2
                visible: root.hasSecret && root.selectedKey?.statusCode !== "revoked"
                text: KI18n.i18nc("@action:button", "Create…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("prepareRevocation", root.selectedKey.id)
            }
            Controls.Button {
                Layout.preferredWidth: root.hasSecret && root.selectedKey?.statusCode !== "revoked" ? (root.buttonWidth - Kirigami.Units.smallSpacing) / 2 : root.buttonWidth
                text: KI18n.i18nc("@action:button", "Apply Existing…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("import")
            }
        }
        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: 8
            Layout.bottomMargin: 8
        }
        Kirigami.Heading {
            level: 4
            text: KI18n.i18nc("@title:group", "Danger zone")
        }
        IdentityRow {
            visible: root.hasSecret && root.selectedKey?.statusCode !== "revoked"
            indicator: "dialog-cancel"
            title: KI18n.i18nc("@title", "Revoke this key permanently")
            Controls.Button {
                Layout.preferredWidth: root.buttonWidth
                text: KI18n.i18nc("@action:button", "Revoke Key…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("revokeKey", root.selectedKey.id)
            }
        }
        IdentityRow {
            indicator: "edit-delete"
            title: KI18n.i18nc("@title", "Delete key from this computer")
            Controls.Button {
                Layout.preferredWidth: root.buttonWidth
                text: KI18n.i18nc("@action:button", "Delete from Keyring…")
                enabled: root.available
                onClicked: root.operationDialog.openFor("delete", root.selectedKey.id)
            }
        }
        Item {
            implicitHeight: 8
        }
    }
}
