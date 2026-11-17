pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: root

    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    readonly property var contactsController: keyStore.contactsController
    readonly property var contactEntries: {
        const rows = [];
        for (const contact of keyStore.contactRows) {
            const fingerprints = keyStore.contactFingerprints(contact);
            if (!fingerprints.length) {
                continue;
            }
            for (const fingerprint of fingerprints) {
                const key = keyStore.entries.find(entry => entry.id.toUpperCase() === fingerprint) || null;
                rows.push({
                    contact: contact,
                    fingerprint: fingerprint,
                    key: key
                });
            }
        }
        return rows;
    }

    spacing: Kirigami.Units.largeSpacing

    OnlineServicesDialog {
        id: onlineDialog
        keyStore: root.keyStore
    }

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 2
        text: KI18n.i18nc("@title", "Contacts with an OpenPGP fingerprint")
    }
    Controls.Label {
        Layout.fillWidth: true
        text: KI18n.i18n("Link a key to a contact from its details. You can choose anyone in your KDE address book.")
        wrapMode: Text.Wrap
    }
    Flow {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Refresh Address Book")
            icon.name: "view-refresh"
            enabled: !root.contactsController.busy
            onClicked: root.contactsController.refreshContacts()
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Import Key…")
            icon.name: "document-import"
            enabled: !root.keyStore.controller.busy && !root.keyStore.controller.operationBusy
            onClicked: root.operationDialog.openFor("import")
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Fetch from Server…")
            icon.name: "download"
            enabled: !root.keyStore.controller.busy && !root.keyStore.controller.operationBusy
            onClicked: root.operationDialog.openFor("refreshExternal")
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Configure Online Services…")
            icon.name: "configure"
            onClicked: onlineDialog.edit()
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Open KDE Contacts")
            icon.name: "view-pim-contacts"
            enabled: !root.contactsController.busy
            onClicked: root.contactsController.openContacts()
        }
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.contactsController.errorMessage
        type: Kirigami.MessageType.Error
        text: root.contactsController.errorMessage
    }
    Controls.ScrollView {
        id: contactsScroll
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: contactsScroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                visible: root.contactsController.busy
                text: KI18n.i18n("Reading KDE Contacts…")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: root.contactEntries.length === 0 && !root.contactsController.busy
                text: KI18n.i18n("No contacts have a linked OpenPGP fingerprint. Add the fingerprint in the KDE address book or link a key from its details.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            Repeater {
                model: root.contactEntries
                delegate: Controls.Frame {
                    id: contactEntry
                    required property var modelData
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            Layout.fillWidth: true
                            text: contactEntry.modelData.contact.name || contactEntry.modelData.contact.email || contactEntry.modelData.contact.uid || KI18n.i18n("Unnamed contact")
                            textFormat: Text.PlainText
                            font.weight: Font.DemiBold
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!contactEntry.modelData.contact.email
                            text: contactEntry.modelData.contact.email || ""
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: contactEntry.modelData.fingerprint || KI18n.i18n("No full OpenPGP fingerprint linked")
                            textFormat: Text.PlainText
                            font: Kirigami.Theme.fixedWidthFont
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: contactEntry.modelData.key ? KI18n.i18n("Key available in the local keyring: %1", contactEntry.modelData.key.title || contactEntry.modelData.key.id) : KI18n.i18n("Key not imported yet")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Button {
                                visible: !!contactEntry.modelData.key
                                text: KI18n.i18nc("@action:button", "Open Key")
                                icon.name: "view-certificate"
                                onClicked: {
                                    root.keyStore.scope = "all";
                                    root.keyStore.query = "";
                                    root.keyStore.selectedId = contactEntry.modelData.key.id;
                                }
                            }
                            Controls.Button {
                                visible: !contactEntry.modelData.key
                                enabled: !root.keyStore.controller.busy && !root.keyStore.controller.operationBusy
                                text: KI18n.i18nc("@action:button", "Fetch Key by Fingerprint…")
                                icon.name: "download"
                                onClicked: root.operationDialog.openFor("refreshExternal", contactEntry.modelData.fingerprint)
                            }
                            Controls.Button {
                                enabled: !root.contactsController.busy
                                text: KI18n.i18nc("@action:button", "Edit Contact")
                                icon.name: "document-edit"
                                onClicked: root.contactsController.editContact(contactEntry.modelData.contact.uid)
                            }
                        }
                    }
                }
            }
        }
    }
}
