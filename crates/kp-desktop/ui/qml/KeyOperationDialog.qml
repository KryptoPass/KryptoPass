pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: dialog

    signal notificationRequested(string message)

    required property KeyStore keyStore
    property string action: ""
    property string fingerprint: ""
    property string userId: ""
    property string subkeyFingerprint: ""
    property var subkeys: []
    property var userIds: []
    property bool privateMaterialAvailable: false
    property string filePath: ""
    property bool submitted: false
    property bool privateBackupConfirmed: false
    property bool consequenceConfirmed: false
    property Item dialogParent: Controls.Overlay.overlay
    property string operationError: ""
    property string operationWarning: ""
    readonly property var expiryOptions: [
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "30 days"),
            days: 30
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "90 days"),
            days: 90
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "180 days"),
            days: 180
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "1 year"),
            days: 365
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "2 years"),
            days: 730
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "5 years"),
            days: 1825
        },
        {
            label: KI18n.i18nc("@item:inlistbox key expiry", "Never expires"),
            days: 0
        }
    ]
    readonly property var usageOptions: [
        {
            label: KI18n.i18nc("@item:inlistbox subkey usage", "Data encryption"),
            value: "encrypt"
        },
        {
            label: KI18n.i18nc("@item:inlistbox subkey usage", "Signing"),
            value: "sign"
        },
        {
            label: KI18n.i18nc("@item:inlistbox subkey usage", "SSH access (authentication)"),
            value: "authenticate"
        }
    ]
    readonly property var materialOptions: privateMaterialAvailable ? [
        {
            label: KI18n.i18nc("@item:inlistbox key material", "Public and private"),
            value: "both"
        },
        {
            label: KI18n.i18nc("@item:inlistbox key material", "Private only"),
            value: "private"
        }
    ] : [
        {
            label: KI18n.i18nc("@item:inlistbox key material", "Public only"),
            value: "public"
        }
    ]
    readonly property var certifierOptions: keyStore.entries.filter(key => key.hasSecret && key.canCertify && key.id.toUpperCase() !== normalizedFingerprint(fingerprint)).map(key => ({
                label: (key.title || KI18n.i18n("No name")) + " · " + key.id,
                fingerprint: key.id
            }))
    readonly property bool controllerBusy: keyStore.controller.busy || keyStore.controller.operationBusy
    readonly property bool destructive: action === "revokeKey" || action === "revokeIdentity" || action === "revokeSubkey" || action === "delete"
    readonly property bool needsFile: action === "import" || action === "exportPublic" || action === "backupPrivate" || action === "prepareRevocation"
    readonly property bool privateBackupReady: action !== "backupPrivate" || privateBackupConfirmed
    readonly property bool consequenceReady: !destructive || consequenceConfirmed
    readonly property bool canSubmit: !controllerBusy && !submitted && !completionMessage && fingerprintReady() && privateBackupReady && consequenceReady && operationInputsReady()
    property string completionMessage: ""
    property string completedFingerprint: ""
    /// Use preselected the next time "addSubkey" opens; consumed by `openFor`.
    property string presetUsage: ""
    /// KryptoPass label typed for a new subkey; the caller stores it once the subkey exists.
    property string subkeyLabel: ""

    parent: dialogParent
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 36, parent ? parent.width - Kirigami.Units.gridUnit * 4 : 720)
    height: Math.min(implicitHeight, parent ? parent.height - Kirigami.Units.gridUnit * 4 : 640)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: titleForAction()
    Controls.Overlay.modal: Rectangle {
        color: "#80000000"
    }
    header: Controls.Pane {
        padding: Kirigami.Units.largeSpacing
        background: null
        contentItem: Controls.Label {
            text: dialog.title
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
    }

    function normalizedFingerprint(value) {
        return (value || "").replace(/\s+/g, "").toUpperCase();
    }

    function isFullFingerprint(value) {
        return /^(?:[A-F0-9]{40}|[A-F0-9]{64})$/.test(normalizedFingerprint(value));
    }

    function displayPath(path) {
        return decodeURIComponent(String(path || "").replace(/^file:\/\//, ""));
    }

    function fileName(path) {
        const parts = displayPath(path).split("/");
        return parts[parts.length - 1];
    }

    function droppedFileUrl(event) {
        const isFile = value => /^file:\/\//i.test(value);
        const fromList = text => (text || "").split(/\r?\n/).map(line => line.replace(/\0/g, "").trim()).find(line => isFile(line)) || "";
        for (const url of event.urls || []) {
            if (isFile(url.toString()))
                return url.toString();
        }
        for (const format of ["application/x-kde4-urilist", "text/x-moz-url", "text/uri-list"]) {
            if (!event.formats.includes(format))
                continue;
            const found = fromList(event.getDataAsString(format));
            if (found)
                return found;
        }
        return fromList(event.text);
    }

    function requestFingerprint() {
        return normalizedFingerprint(fingerprint || fingerprintField.text);
    }

    function fingerprintReady() {
        if (action === "create" || action === "import")
            return true;
        return isFullFingerprint(requestFingerprint());
    }

    function selectedExpiryDays() {
        return expiryCombo.currentIndex >= 0 ? expiryOptions[expiryCombo.currentIndex].days : 0;
    }

    function operationInputsReady() {
        if (action === "create")
            return !!userIdField.text.trim();
        if (needsFile)
            return !!filePath;
        if (action === "addIdentity")
            return !!userIdField.text.trim();
        if (action === "primaryIdentity" || action === "revokeIdentity")
            return !!userId;
        if (action === "revokeSubkey")
            return isFullFingerprint(subkeyFingerprint);
        if (action === "addSubkey")
            return usageCombo.currentIndex >= 0;
        if (action === "certify")
            return userIds.length > 0 && certifierCombo.currentIndex >= 0 && certifierOptions.length > 0;
        if (action === "refreshExternal")
            return keyStore.onlineSettings.enabled;
        return true;
    }

    function titleForAction() {
        const titles = {
            create: KI18n.i18nc("@title:window", "Create OpenPGP Key"),
            import: KI18n.i18nc("@title:window", "Import OpenPGP Key"),
            exportPublic: KI18n.i18nc("@title:window", "Export Public Key"),
            copyPublic: KI18n.i18nc("@action", "Copy Public Key"),
            backupPrivate: KI18n.i18nc("@title:window", "Back Up Private Key"),
            passphrase: KI18n.i18nc("@title:window", "Change Passphrase"),
            expiry: KI18n.i18nc("@title:window", "Change Expiry"),
            addIdentity: KI18n.i18nc("@action", "Add Identity"),
            primaryIdentity: KI18n.i18nc("@title:window", "Choose Primary Identity"),
            revokeIdentity: KI18n.i18nc("@action", "Revoke Identity"),
            addSubkey: KI18n.i18nc("@action", "Add Subkey"),
            revokeSubkey: KI18n.i18nc("@action", "Revoke Subkey"),
            prepareRevocation: KI18n.i18nc("@title:window", "Create Revocation Certificate"),
            revokeKey: KI18n.i18nc("@action", "Revoke Key"),
            certify: KI18n.i18nc("@action", "Certify Identity"),
            refreshExternal: KI18n.i18nc("@title:window", "Fetch or Refresh Key"),
            delete: KI18n.i18nc("@title:window", "Delete Key from Keyring"),
            markAsOwn: KI18n.i18nc("@title:window", "Confirm Trust in GnuPG")
        };
        return titles[action] || KI18n.i18nc("@title:window", "OpenPGP Operation");
    }

    function operationButtonText() {
        const labels = {
            create: KI18n.i18nc("@action:button", "Create Key"),
            import: KI18n.i18nc("@action:button", "Import Key"),
            exportPublic: KI18n.i18nc("@action:button", "Export Public"),
            copyPublic: KI18n.i18nc("@action:button", "Copy Public"),
            backupPrivate: KI18n.i18nc("@action:button", "Save Private Backup"),
            passphrase: KI18n.i18nc("@action:button", "Open Secure Dialog"),
            expiry: KI18n.i18nc("@action:button", "Save Expiry"),
            addIdentity: KI18n.i18nc("@action", "Add Identity"),
            primaryIdentity: KI18n.i18nc("@action:button", "Make Primary"),
            revokeIdentity: KI18n.i18nc("@action", "Revoke Identity"),
            addSubkey: KI18n.i18nc("@action", "Add Subkey"),
            revokeSubkey: KI18n.i18nc("@action", "Revoke Subkey"),
            prepareRevocation: KI18n.i18nc("@action:button", "Save Certificate"),
            revokeKey: KI18n.i18nc("@action", "Revoke Key"),
            certify: KI18n.i18nc("@action", "Certify Identity"),
            refreshExternal: KI18n.i18nc("@action:button", "Fetch Key"),
            delete: KI18n.i18nc("@action:button", "Delete from Keyring"),
            markAsOwn: KI18n.i18nc("@action:button", "Mark as Mine")
        };
        return labels[action] || KI18n.i18nc("@action:button", "Continue");
    }

    function requestObject() {
        const request = {
            action: action
        };
        if (action !== "create" && action !== "import")
            request.fingerprint = requestFingerprint();
        if (action === "create") {
            request.userid = userIdField.text.trim();
            request.expiresDays = selectedExpiryDays();
        } else if (action === "import" || action === "exportPublic" || action === "backupPrivate" || action === "prepareRevocation") {
            request.path = filePath;
        } else if (action === "expiry") {
            request.subkeys = subkeys.slice();
            request.expiresDays = selectedExpiryDays();
        } else if (action === "addIdentity") {
            request.userid = userIdField.text.trim();
        } else if (action === "primaryIdentity" || action === "revokeIdentity") {
            request.userid = userId;
        } else if (action === "addSubkey") {
            request.usage = usageOptions[usageCombo.currentIndex].value;
            request.expiresDays = selectedExpiryDays();
        } else if (action === "revokeSubkey") {
            request.subkey = subkeyFingerprint;
        } else if (action === "certify") {
            request.signerFingerprint = certifierOptions[certifierCombo.currentIndex].fingerprint;
            request.userids = userIds.slice();
            request.local = localCertificationCheck.checked;
        } else if (action === "refreshExternal") {
            request.keyserver = keyStore.onlineSettings.keyservers[serverCombo.currentIndex] || "";
        } else if (action === "delete") {
            request.material = materialOptions[materialCombo.currentIndex].value;
        }
        return request;
    }

    function openFor(actionName, sourceFingerprint, sourceUserId, sourceSubkey, sourceUserIds) {
        action = actionName;
        fingerprint = normalizedFingerprint(sourceFingerprint || "");
        userId = sourceUserId || "";
        subkeyFingerprint = normalizedFingerprint(sourceSubkey || "");
        subkeys = subkeyFingerprint ? [subkeyFingerprint] : [];
        userIds = sourceUserIds ? sourceUserIds.slice() : (userId ? [userId] : []);
        const sourceKey = keyStore.entries.find(key => key.id.toUpperCase() === normalizedFingerprint(fingerprint));
        privateMaterialAvailable = !!sourceKey?.hasSecret;
        filePath = "";
        submitted = false;
        privateBackupConfirmed = false;
        consequenceConfirmed = false;
        completionMessage = "";
        completedFingerprint = "";
        operationError = "";
        operationWarning = "";
        userIdField.text = "";
        subkeyLabelField.text = "";
        fingerprintField.text = "";
        keyStore.controller.clearOperationError();
        Qt.callLater(function () {
            serverCombo.currentIndex = 0;
            expiryCombo.currentIndex = 3;
            usageCombo.currentIndex = Math.max(0, usageOptions.findIndex(option => option.value === presetUsage));
            presetUsage = "";
            certifierCombo.currentIndex = certifierOptions.length ? 0 : -1;
            materialCombo.currentIndex = 0;
            localCertificationCheck.checked = true;
            if (action === "create" || action === "addIdentity")
                userIdField.forceActiveFocus();
            else if (action === "refreshExternal" && !fingerprint)
                fingerprintField.forceActiveFocus();
        });
        if (action === "import") {
            fileDialog.open();
            return;
        }
        open();
    }

    function submit() {
        if (!canSubmit)
            return;
        keyStore.controller.clearOperationError();
        operationError = "";
        operationWarning = "";
        submitted = true;
        completionMessage = "";
        keyStore.controller.runOperation(JSON.stringify(requestObject()));
    }

    function successMessage(message) {
        if (message)
            return message;
        return KI18n.i18n("Operation complete.");
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.opened && !dialog.controllerBusy
        onActivated: dialog.reject()
    }

    Connections {
        target: dialog.keyStore.controller
        function onOperationFinished(finishedFingerprint, message) {
            if (!dialog.submitted)
                return;
            dialog.submitted = false;
            dialog.completedFingerprint = finishedFingerprint || "";
            if (finishedFingerprint)
                dialog.keyStore.selectAfterRefresh(finishedFingerprint);
            dialog.operationWarning = String(dialog.keyStore.controller.operationError || "");
            dialog.completionMessage = dialog.successMessage(message);
        }
        function onOperationFailed(message) {
            if (!dialog.submitted)
                return;
            dialog.submitted = false;
            dialog.operationWarning = "";
            dialog.operationError = message || dialog.keyStore.controller.operationError;
        }
        function onPublicKeyReady(armored) {
            if (!dialog.submitted || dialog.action !== "copyPublic" || !armored)
                return;
            publicKeyClipboardSource.text = armored;
            publicKeyClipboardSource.selectAll();
            publicKeyClipboardSource.copy();
            publicKeyClipboardSource.deselect();
            publicKeyClipboardSource.text = "";
            dialog.notificationRequested(KI18n.i18n("Copied to clipboard"));
        }
    }

    ToastHost {
        id: operationToast
        overlayItem: dialog.parent
    }

    Connections {
        target: dialog
        function onNotificationRequested(message) {
            operationToast.show(message, "success");
        }
    }

    TextEdit {
        id: publicKeyClipboardSource
        visible: false
        readOnly: true
        textFormat: TextEdit.PlainText
    }

    contentItem: Controls.ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        implicitHeight: Math.min(form.implicitHeight + Kirigami.Units.largeSpacing, dialog.parent ? dialog.parent.height - Kirigami.Units.gridUnit * 10 : 480)

        ColumnLayout {
            id: form
            width: scroll.availableWidth
            spacing: Kirigami.Units.largeSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.operationError
                type: Kirigami.MessageType.Error
                text: dialog.operationError
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.operationWarning
                type: Kirigami.MessageType.Warning
                text: dialog.operationWarning
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.completionMessage && !(dialog.action === "import" && !!dialog.completedFingerprint)
                type: Kirigami.MessageType.Positive
                text: dialog.completionMessage
            }

            KeyImportReview {
                id: importReview
                Layout.fillWidth: true
                visible: dialog.action === "import" && !!dialog.completionMessage && !!dialog.completedFingerprint
                selectedKey: dialog.keyStore.entries.find(key => key.id.toUpperCase() === (dialog.completedFingerprint || "").toUpperCase()) || null
                keyStore: dialog.keyStore
            }

            Controls.Button {
                visible: !!dialog.completionMessage && !!dialog.completedFingerprint && (dialog.action === "import" || dialog.action === "refreshExternal")
                enabled: !dialog.keyStore.contactsController.busy
                text: KI18n.i18nc("@action:button", "Link to KDE Contact…")
                icon.name: "link"
                onClicked: dialog.keyStore.contactsController.chooseContactForKey(dialog.completedFingerprint)
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.completedFingerprint && !!dialog.keyStore.contactsController.errorMessage
                type: Kirigami.MessageType.Error
                text: dialog.keyStore.contactsController.errorMessage
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!(dialog.fingerprint || fingerprintField.text) && dialog.action !== "create" && dialog.action !== "import"
                text: KI18n.i18n("Full fingerprint: %1", dialog.requestFingerprint())
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!dialog.userId
                text: KI18n.i18n("Target identity: %1", dialog.userId)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!dialog.subkeyFingerprint
                text: KI18n.i18n("Target subkey: %1", dialog.subkeyFingerprint)
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "create"
                text: KI18n.i18n("GnuPG may ask for the passphrase in its secure dialog. The passphrase is never typed into or stored in KryptoPass.")
                wrapMode: Text.Wrap
            }
            Controls.TextField {
                id: userIdField
                Layout.fillWidth: true
                visible: dialog.action === "create" || dialog.action === "addIdentity"
                enabled: !dialog.controllerBusy
                placeholderText: KI18n.i18nc("@info:placeholder", "Name <email@example.org>")
                Accessible.name: KI18n.i18n("Name and email of the OpenPGP identity")
                onAccepted: if (dialog.canSubmit)
                    dialog.submit()
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "create"
                text: KI18n.i18nc("@label:listbox", "Initial key expiry")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: expiryCombo
                Layout.fillWidth: true
                visible: dialog.action === "create" || dialog.action === "expiry" || dialog.action === "addSubkey"
                enabled: !dialog.controllerBusy
                model: dialog.expiryOptions
                textRole: "label"
                currentIndex: 3
                Accessible.name: KI18n.i18n("Expiry")
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "import" && !dialog.completionMessage
                text: KI18n.i18n("Choose an OpenPGP file to import. The keyring changes only with the file's content; no identity is accepted because of its email.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "exportPublic"
                text: KI18n.i18n("The full public key will be exported, including its identities and public subkeys.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "backupPrivate"
                text: KI18n.i18n("The file will contain private key material. It keeps GnuPG's current protection; if the key has no passphrase, the backup will not be protected by one either. Store it somewhere private.")
                wrapMode: Text.Wrap
            }
            Controls.CheckBox {
                id: privateBackupCheck
                visible: dialog.action === "backupPrivate"
                enabled: !dialog.controllerBusy
                checked: dialog.privateBackupConfirmed
                text: KI18n.i18nc("@option:check", "I understand that the file contains private material and keeps the key's current protection")
                onToggled: dialog.privateBackupConfirmed = checked
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "passphrase"
                text: KI18n.i18n("GnuPG will open its secure pinentry dialog. Enter the passphrase there; KryptoPass neither asks for it nor receives it.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "expiry"
                text: dialog.subkeys.length ? KI18n.i18n("The new expiry will apply only to the selected subkey.") : KI18n.i18n("The new expiry will apply to the primary key. Choose “Never expires” to remove the expiration date.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "addIdentity"
                text: KI18n.i18n("Add an identity to this key of yours. Use a name and email you want to publish in OpenPGP.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "primaryIdentity"
                text: KI18n.i18n("The chosen identity will become the key's primary identity.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeIdentity"
                text: KI18n.i18n("This identity will be revoked permanently. It will no longer be valid for the key.")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: usageCombo
                Layout.fillWidth: true
                visible: dialog.action === "addSubkey"
                enabled: !dialog.controllerBusy
                model: dialog.usageOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: KI18n.i18n("Usage of the new subkey")
            }
            Controls.TextField {
                id: subkeyLabelField
                Layout.fillWidth: true
                visible: dialog.action === "addSubkey"
                enabled: !dialog.controllerBusy
                maximumLength: 160
                placeholderText: KI18n.i18nc("@info:placeholder", "Optional label, for example: GitHub SSH")
                Accessible.name: KI18n.i18n("Subkey label")
                onTextChanged: dialog.subkeyLabel = text
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeSubkey"
                text: KI18n.i18n("Only the subkey shown will be revoked. The primary key will not be revoked.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "prepareRevocation"
                text: KI18n.i18n("Store the certificate safely outside this computer: whoever has it can revoke the key. Creating the certificate does not revoke the key yet.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeKey"
                text: KI18n.i18n("Revoking the primary key is permanent and affects all its identities and subkeys.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                text: KI18n.i18n("Choose a key of yours that can certify and review the identity you will sign. A certification does not confirm that you verified its owner.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                text: dialog.userIds.join("\n")
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }
            Controls.ComboBox {
                id: certifierCombo
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                enabled: !dialog.controllerBusy
                model: dialog.certifierOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: KI18n.i18n("Your key that will certify this identity")
            }
            Controls.CheckBox {
                id: localCertificationCheck
                visible: dialog.action === "certify"
                enabled: !dialog.controllerBusy
                checked: true
                text: KI18n.i18nc("@option:check", "Keep the certification local, without exporting it")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal"
                text: KI18n.i18n("Fetch the key by its full fingerprint. Check the fingerprint with its owner before giving them access to an item.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                visible: dialog.action === "refreshExternal"
                text: KI18n.i18nc("@label:listbox", "Key server")
            }
            Controls.ComboBox {
                id: serverCombo
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal"
                enabled: !dialog.controllerBusy && dialog.keyStore.onlineSettings.enabled
                model: dialog.keyStore.onlineSettings.keyservers.length ? dialog.keyStore.onlineSettings.keyservers : [KI18n.i18nc("@item:inlistbox key server", "GnuPG configuration")]
                Accessible.name: KI18n.i18nc("@label:listbox", "Key server")
                onModelChanged: currentIndex = 0
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal" && !dialog.keyStore.onlineSettings.enabled
                text: KI18n.i18n("Online lookups are disabled. Enable them in the settings to fetch keys.")
                wrapMode: Text.Wrap
            }
            Controls.Button {
                visible: dialog.action === "refreshExternal"
                enabled: !dialog.controllerBusy
                text: KI18n.i18nc("@action:button", "Configure Online Services…")
                icon.name: "configure"
                onClicked: onlineDialog.edit()
            }
            Controls.Label {
                visible: dialog.action === "refreshExternal" && !dialog.fingerprint
                text: KI18n.i18nc("@label:textbox", "Full OpenPGP fingerprint")
            }
            Controls.TextField {
                id: fingerprintField
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal" && !dialog.fingerprint
                enabled: !dialog.controllerBusy
                placeholderText: KI18n.i18nc("@info:placeholder", "Full fingerprint of 40 or 64 hexadecimal characters")
                font: Kirigami.Theme.fixedWidthFont
                Accessible.name: KI18n.i18n("Full OpenPGP fingerprint to retrieve an external key")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "markAsOwn"
                text: KI18n.i18n("This key has its private part on this computer, but GnuPG does not know it is yours, so it refuses to encrypt with it. Marking it as yours gives it ultimate trust in this keyring. Only do this with keys that are really yours. Nothing is sent to other devices and nothing is revoked.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "delete"
                text: KI18n.i18n("Deletion affects only the local keyring; it does not revoke the key or withdraw copies other people already have. If you delete the private part without a backup, you will lose access to this key's secrets.")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: materialCombo
                Layout.fillWidth: true
                visible: dialog.action === "delete"
                enabled: !dialog.controllerBusy
                model: dialog.materialOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: KI18n.i18n("Material to delete from the keyring")
            }

            DropArea {
                id: importFileDrop
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 7
                visible: dialog.action === "import" && !dialog.completionMessage
                enabled: !dialog.controllerBusy
                onEntered: function (drag) {
                    drag.accepted = drag.hasUrls || drag.formats.includes("text/uri-list");
                    if (drag.accepted)
                        drag.acceptProposedAction();
                }
                onDropped: function (event) {
                    const url = dialog.droppedFileUrl(event);
                    if (!url)
                        return;
                    event.acceptProposedAction();
                    dialog.filePath = url;
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Kirigami.Units.cornerRadius
                    color: importFileDrop.containsDrag ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent"
                    border.width: 1
                    border.color: importFileDrop.containsDrag ? Kirigami.Theme.highlightColor : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
                }
                ColumnLayout {
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    visible: !dialog.filePath
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignHCenter
                        source: "view-certificate-import"
                        implicitWidth: Kirigami.Units.iconSizes.large
                        implicitHeight: Kirigami.Units.iconSizes.large
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: KI18n.i18n("Drop your OpenPGP key here")
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: KI18n.i18n("Supported files: .asc, .gpg, .pgp")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                    Controls.Button {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        text: KI18n.i18nc("@action:button", "Browse…")
                        icon.name: "document-open"
                        enabled: !dialog.controllerBusy
                        onClicked: fileDialog.open()
                    }
                }
                RowLayout {
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    visible: !!dialog.filePath
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: "view-certificate-import"
                        implicitWidth: Kirigami.Units.iconSizes.medium
                        implicitHeight: Kirigami.Units.iconSizes.medium
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        Controls.Label {
                            Layout.fillWidth: true
                            text: dialog.fileName(dialog.filePath)
                            textFormat: Text.PlainText
                            font.weight: Font.DemiBold
                            elide: Text.ElideMiddle
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: dialog.displayPath(dialog.filePath)
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            elide: Text.ElideMiddle
                        }
                    }
                    Controls.Button {
                        text: KI18n.i18nc("@action:button", "Change…")
                        icon.name: "document-open"
                        enabled: !dialog.controllerBusy
                        onClicked: fileDialog.open()
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.needsFile && dialog.action !== "import"
                text: dialog.filePath ? dialog.filePath : KI18n.i18n("No file chosen.")
                textFormat: Text.PlainText
                color: dialog.filePath ? Kirigami.Theme.textColor : Kirigami.Theme.disabledTextColor
                wrapMode: Text.WrapAnywhere
            }
            Controls.Button {
                Layout.alignment: Qt.AlignLeft
                visible: dialog.needsFile && dialog.action !== "import"
                enabled: !dialog.controllerBusy
                text: KI18n.i18nc("@action:button", "Choose Destination…")
                icon.name: "document-save-as"
                onClicked: fileDialog.open()
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.destructive
                text: dialog.destructiveDescription()
                wrapMode: Text.Wrap
            }
            Controls.CheckBox {
                id: consequenceCheck
                visible: dialog.destructive
                enabled: !dialog.controllerBusy
                checked: dialog.consequenceConfirmed
                text: dialog.action === "delete" ? KI18n.i18nc("@option:check", "I understand that this only deletes local material and does not revoke the key") : KI18n.i18nc("@option:check", "I understand that this revocation is permanent")
                onToggled: dialog.consequenceConfirmed = checked
            }
        }
    }

    function destructiveDescription() {
        if (action === "revokeKey")
            return KI18n.i18n("Permanent revocation of primary key %1 and all its identities and subkeys.", fingerprint);
        if (action === "revokeIdentity")
            return KI18n.i18n("The identity “%1” of key %2 will be revoked permanently.", userId, fingerprint);
        if (action === "revokeSubkey")
            return KI18n.i18n("Subkey %1 of key %2 will be revoked permanently.", subkeyFingerprint, fingerprint);
        const material = materialOptions[materialCombo.currentIndex]?.label || KI18n.i18n("the selected material");
        if (privateMaterialAvailable && materialOptions[materialCombo.currentIndex]?.value === "private")
            return KI18n.i18n("The private part of key %1 will be deleted and the public part kept. Without a backup you will not be able to use this key to decrypt or sign again. Local deletion does not revoke the key, and a backup can restore it.", fingerprint);
        return KI18n.i18n("%1 of key %2 will be deleted from the local keyring.", material, fingerprint);
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: dialog.action === "import" && dialog.completionMessage ? KI18n.i18nc("@action:button", "Continue") : dialog.completionMessage ? KI18n.i18nc("@action:button", "Close") : KI18n.i18nc("@action:button", "Cancel")
            highlighted: dialog.action === "import" && dialog.completionMessage
            enabled: !dialog.controllerBusy
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            onClicked: dialog.reject()
        }
        Controls.Button {
            visible: !dialog.completionMessage
            text: dialog.operationButtonText()
            icon.name: dialog.destructive ? "dialog-warning" : "dialog-ok"
            enabled: dialog.canSubmit
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
            onClicked: dialog.submit()
        }
    }

    OnlineServicesDialog {
        id: onlineDialog
        keyStore: dialog.keyStore
    }

    Dialogs.FileDialog {
        id: fileDialog
        title: dialog.action === "import" ? KI18n.i18nc("@title:window", "Select OpenPGP Key to Import") : KI18n.i18nc("@title:window", "Choose OpenPGP File Destination")
        fileMode: dialog.action === "import" ? Dialogs.FileDialog.OpenFile : Dialogs.FileDialog.SaveFile
        defaultSuffix: dialog.action === "backupPrivate" ? "gpg" : "asc"
        nameFilters: [KI18n.i18n("OpenPGP files (*.asc *.gpg *.pgp *.key)"), KI18n.i18n("All files (*)")]
        onAccepted: {
            dialog.filePath = selectedFile.toString();
            if (dialog.action === "import") {
                dialog.submit();
                dialog.open();
            }
        }
        onRejected: {
            if (dialog.action === "import" && !dialog.opened)
                dialog.open();
        }
    }

    onRejected: {
        if (!controllerBusy)
            close();
    }
    onClosed: {
        submitted = false;
        operationError = "";
        operationWarning = "";
        keyStore.controller.clearOperationError();
        publicKeyClipboardSource.text = "";
    }
}
