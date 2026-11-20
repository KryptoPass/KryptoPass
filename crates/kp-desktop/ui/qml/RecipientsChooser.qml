pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend
import org.kde.ki18n

ColumnLayout {
    id: chooser

    Accessible.role: Accessible.Grouping
    Accessible.name: KI18n.i18nc("@title:group", "Who has access")

    required property KeyStore keyStore
    readonly property Backend.ContactsController contactsController: keyStore.contactsController
    property var selectedFingerprints: []
    readonly property var encryptableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property var matchingKeys: chooser.encryptableKeys.filter(key => (key.title + " " + key.email + " " + key.contactName + " " + key.id).toLowerCase().includes(recipientSearch.text.trim().toLowerCase()) || key.id.toLowerCase().includes(recipientSearch.text.replace(/\s/g, "").toLowerCase()))
    readonly property var unavailableFingerprints: selectedFingerprints.filter(fingerprint => !encryptableKeys.some(key => key.id.toUpperCase() === fingerprint.toUpperCase()))
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && unavailableFingerprints.length === 0
    readonly property var contactRows: {
        const parsed = [];
        for (const record of contactsController.contactRecords) {
            try {
                parsed.push(JSON.parse(record));
            } catch (error) {
                // Ignore malformed contact rows from the native projection.
            }
        }
        return parsed;
    }
    readonly property var linkedRecipientRows: {
        const rows = [];
        for (const contact of contactRows) {
            for (const fingerprint of keyStore.contactFingerprints(contact)) {
                if (Array.from(contactsController.recipientCandidates).includes(fingerprint) && encryptableKeys.some(key => key.id.toUpperCase() === fingerprint))
                    rows.push(Object.assign({}, contact, {
                        openPgpFingerprint: fingerprint
                    }));
            }
        }
        return rows;
    }
    readonly property QtObject recipientDialogOwner: QtObject {
        readonly property Controls.Dialog dialog: Controls.Dialog {
            id: contactRecipientDialog
            parent: Controls.Overlay.overlay
            title: KI18n.i18nc("@title:window", "Choose a Linked Contact")
            modal: true
            standardButtons: Controls.Dialog.Cancel
            implicitWidth: Math.min(Kirigami.Units.gridUnit * 28, chooser.width)
            implicitHeight: Math.min(contentItem.implicitHeight + topPadding + bottomPadding + header.implicitHeight + footer.implicitHeight, parent ? parent.height - Kirigami.Units.gridUnit * 4 : 480)
            x: parent ? (parent.width - width) / 2 : 0
            y: parent ? (parent.height - height) / 2 : 0
            onRejected: chooser.contactsController.clearRecipientSelection()

            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("Choose a contact already linked to one of these keys. Matching uses the full fingerprint, never the email.")
                    wrapMode: Text.Wrap
                }
                Controls.ScrollView {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.min(contentHeight, Kirigami.Units.gridUnit * 16)
                    clip: true
                    ColumnLayout {
                        width: contactRecipientDialog.availableWidth
                        Repeater {
                            model: chooser.linkedRecipientRows
                            delegate: Controls.ItemDelegate {
                                id: contactChoice
                                required property var modelData
                                Layout.fillWidth: true
                                text: contactChoice.modelData.name || contactChoice.modelData.email || contactChoice.modelData.uid
                                Accessible.name: KI18n.i18n("Choose contact %1, fingerprint %2", text, contactChoice.modelData.openPgpFingerprint)
                                onClicked: {
                                    chooser.contactsController.selectRecipient(contactChoice.modelData.openPgpFingerprint);
                                    contactRecipientDialog.close();
                                }
                                contentItem: ColumnLayout {
                                    spacing: 2
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: contactChoice.modelData.name || contactChoice.modelData.email || contactChoice.modelData.uid
                                        textFormat: Text.PlainText
                                        wrapMode: Text.Wrap
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        text: contactChoice.modelData.email || contactChoice.modelData.openPgpFingerprint
                                        textFormat: Text.PlainText
                                        color: Kirigami.Theme.disabledTextColor
                                        wrapMode: Text.WrapAnywhere
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    signal selectionChanged(var fingerprints)

    spacing: 8

    KeyOperationDialog {
        id: recoveryDialog
        keyStore: chooser.keyStore
    }

    function setChecked(fingerprint, checked) {
        const next = selectedFingerprints.filter(candidate => candidate !== fingerprint);
        if (checked)
            next.push(fingerprint);
        selectedFingerprints = next;
        selectionChanged(next);
    }

    function contactsForFingerprint(fingerprint) {
        const normalized = fingerprint.toUpperCase();
        return contactRows.filter(contact => keyStore.contactFingerprints(contact).includes(normalized));
    }

    Connections {
        target: chooser.contactsController
        function onRecipientSelected(fingerprint) {
            chooser.setChecked(fingerprint, true);
        }
    }

    Controls.Label {
        text: KI18n.i18nc("@title:group", "Who has access")
        font.weight: Font.DemiBold
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8

        Controls.Label {
            Layout.fillWidth: true
            text: KI18n.i18n("Choose who can open this item with their OpenPGP private key.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Kirigami.SearchField {
            id: recipientSearch
            Layout.fillWidth: true
            placeholderText: KI18n.i18nc("@info:placeholder recipients", "Search name, email or fingerprint…")
            Accessible.name: KI18n.i18n("Search people with an OpenPGP key")
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                id: keyActionsButton
                text: KI18n.i18nc("@action:button", "Add or Import Key")
                icon.name: "list-add"
                Accessible.name: KI18n.i18n("Add or import an access key")
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: KI18n.i18nc("@info:tooltip", "Refresh contacts, import a key or look it up on a server")
                onClicked: keyActionsMenu.open()
                Controls.Menu {
                    id: keyActionsMenu
                    y: keyActionsButton.height
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Refresh KDE Contacts")
                        icon.name: "view-refresh"
                        enabled: !chooser.contactsController.busy
                        onTriggered: chooser.contactsController.refreshContacts()
                    }
                    Controls.MenuSeparator {}
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Import Key from File…")
                        icon.name: "document-import"
                        enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                        onTriggered: recoveryDialog.openFor("import")
                    }
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Fetch Key from Server…")
                        icon.name: "download"
                        enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                        onTriggered: recoveryDialog.openFor("refreshExternal")
                    }
                    Controls.MenuSeparator {}
                    Controls.MenuItem {
                        text: KI18n.i18nc("@action:inmenu", "Choose from Contacts")
                        icon.name: "contact-new"
                        enabled: !chooser.contactsController.busy && chooser.contactRows.length > 0 && chooser.encryptableKeys.length > 0
                        onTriggered: {
                            chooser.contactsController.clearRecipientSelection();
                            chooser.contactsController.chooseRecipient(JSON.stringify(chooser.encryptableKeys.map(key => key.id)));
                            if (Array.from(chooser.contactsController.recipientCandidates).length > 0)
                                contactRecipientDialog.open();
                        }
                    }
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.contactsController.busy
            text: KI18n.i18nc("@info:progress", "Reading or updating KDE Contacts…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !!chooser.contactsController.errorMessage
            text: chooser.contactsController.errorMessage
            color: Kirigami.Theme.negativeTextColor
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !!recipientSearch.text && chooser.matchingKeys.length === 0 && chooser.encryptableKeys.length > 0
            text: KI18n.i18n("No keys match the search.")
            wrapMode: Text.Wrap
        }
        Repeater {
            model: chooser.matchingKeys

            delegate: ColumnLayout {
                id: keyRow
                required property var modelData
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                spacing: 4
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    Controls.CheckBox {
                        id: keyCheck
                        checked: chooser.selectedFingerprints.includes(keyRow.modelData.id)
                        Accessible.name: KI18n.i18n("Give access to %1, %2, fingerprint %3", keyRow.modelData.title, keyRow.modelData.email || "", keyRow.modelData.id)
                        onToggled: chooser.setChecked(keyRow.modelData.id, checked)
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 2
                        Controls.Label {
                            Layout.fillWidth: true
                            text: keyRow.modelData.contactName || keyRow.modelData.title
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: !!keyRow.modelData.email
                            text: keyRow.modelData.email || ""
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.WrapAnywhere
                        }
                    }
                    Controls.ToolButton {
                        id: keyDetails
                        text: checked ? KI18n.i18nc("@action:button", "Hide Key Details") : KI18n.i18nc("@action:button", "View Key Details")
                        icon.name: checked ? "arrow-up" : "arrow-down"
                        display: Controls.AbstractButton.IconOnly
                        checkable: true
                        Accessible.name: KI18n.i18nc("@label label and value", "%1: %2", text, keyRow.modelData.title)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    visible: keyDetails.checked
                    text: KI18n.i18n("OpenPGP fingerprint: %1", keyRow.modelData.id)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Button {
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    visible: keyDetails.checked
                    enabled: !chooser.keyStore.controller.busy && !chooser.keyStore.controller.operationBusy
                    text: KI18n.i18nc("@action", "Copy Public Key")
                    icon.name: "edit-copy"
                    onClicked: recoveryDialog.openFor("copyPublic", keyRow.modelData.id)
                }
                ColumnLayout {
                    id: contactLinkArea
                    visible: keyDetails.checked
                    Layout.fillWidth: true
                    Layout.leftMargin: keyCheck.implicitWidth + 8
                    spacing: Kirigami.Units.smallSpacing
                    readonly property var linkedContacts: chooser.contactsForFingerprint(keyRow.modelData.id)
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: contactLinkArea.linkedContacts.length > 0
                        text: KI18n.i18n("Linked contacts: %1", contactLinkArea.linkedContacts.map(contact => contact.name || contact.email || contact.uid).join(", "))
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: contactLinkArea.linkedContacts
                            delegate: Controls.Button {
                                id: linkedContactEdit
                                required property var modelData
                                text: KI18n.i18nc("@action:button %1 is a contact", "Edit %1", linkedContactEdit.modelData.name || linkedContactEdit.modelData.email || linkedContactEdit.modelData.uid)
                                icon.name: "document-edit"
                                enabled: !chooser.contactsController.busy
                                Accessible.name: KI18n.i18n("Edit contact %1 linked to %2", linkedContactEdit.modelData.name || linkedContactEdit.modelData.email || linkedContactEdit.modelData.uid, keyRow.modelData.title)
                                onClicked: chooser.contactsController.editContact(linkedContactEdit.modelData.uid)
                            }
                        }
                    }
                    Controls.Button {
                        text: KI18n.i18nc("@action:button", "Link to KDE Contact…")
                        icon.name: "link"
                        enabled: !chooser.contactsController.busy
                        Accessible.name: KI18n.i18n("Add a contact link for key %1", keyRow.modelData.title)
                        onClicked: chooser.contactsController.chooseContactForKey(keyRow.modelData.id)
                    }
                }
            }
        }

        Repeater {
            model: chooser.unavailableFingerprints

            delegate: RowLayout {
                id: unavailableRow
                required property string modelData
                Layout.fillWidth: true
                spacing: 8
                Controls.CheckBox {
                    checked: chooser.selectedFingerprints.includes(unavailableRow.modelData)
                    Accessible.name: KI18n.i18n("Original access key unavailable, fingerprint %1", unavailableRow.modelData)
                    onToggled: chooser.setChecked(unavailableRow.modelData, checked)
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: KI18n.i18n("%1 · no valid key to encrypt with", unavailableRow.modelData)
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0
            text: KI18n.i18nc("@info:progress", "Reading OpenPGP keys…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: !chooser.keyStore.controller.busy && chooser.encryptableKeys.length === 0 && chooser.unavailableFingerprints.length === 0
            text: KI18n.i18n("There are no valid keys that can encrypt. Refresh the keyring or check the keys.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: chooser.unavailableFingerprints.length > 0
            text: KI18n.i18n("Each original fingerprint is kept. Uncheck the unavailable keys or choose a valid key before saving.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }
}
