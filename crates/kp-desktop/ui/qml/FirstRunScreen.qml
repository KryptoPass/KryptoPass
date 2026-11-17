pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n

// First run of this installation: choose or create the vault key, then back it up.
Rectangle {
    id: root

    required property KeyStore keyStore
    property url logo: ""
    signal restoreRequested
    signal handbookRequested(string section)
    // `dedicated` marks a key created here, which can be forgotten on lock safely.
    signal setupFinished(string vaultKey, bool dedicated)

    property int step: 0
    property int previousStep: 0
    property string chosenKey: ""
    property bool creating: false
    property bool importing: false
    property string importedFingerprint: ""
    property string reviewSource: "import"
    property bool backedUp: false
    property string errorText: ""
    readonly property var ownKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt && key.hasSecret)
    readonly property var importedKey: importedFingerprint ? (keyStore.entries.find(key => key.id.toUpperCase() === importedFingerprint.toUpperCase()) || null) : null
    readonly property bool importedKeyReady: !!importedKey && importedKey.hasSecret && importedKey.statusCode === "valid" && importedKey.canEncrypt && !importedKey.needsOwnTrust
    readonly property bool busy: creating || importing || keyStore.controller.operationBusy

    onStepChanged: {
        errorText = "";
        stepShift.x = root.step > root.previousStep ? 16 : -16;
        root.previousStep = root.step;
        setupHost.opacity = 0;
        stepFade.restart();
    }

    color: Kirigami.Theme.backgroundColor
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    function createVaultKey() {
        if (busy)
            return;
        errorText = "";
        creating = true;
        keyStore.controller.runOperation(JSON.stringify({
            action: "create",
            userid: Application.displayName,
            expiresDays: 0
        }));
    }

    function useExistingKey() {
        const key = ownKeys[existingKeys.currentIndex];
        if (!key)
            return;
        if (key.needsOwnTrust) {
            importedFingerprint = key.id;
            reviewSource = "existing";
            backedUp = (key.health?.items || []).some(item => item.code === "backupSaved");
            step = 2;
            return;
        }
        setupFinished(key.id, false);
    }

    function importKey(url) {
        if (busy)
            return;
        errorText = "";
        importing = true;
        keyStore.controller.clearOperationError();
        keyStore.controller.runOperation(JSON.stringify({
            action: "import",
            path: url.toString()
        }));
    }

    Connections {
        target: root.keyStore.controller
        function onOperationFinished(fingerprint, message) {
            if (root.importing) {
                root.importing = false;
                root.importedFingerprint = fingerprint || "";
                if (root.importedFingerprint) {
                    // The key came from a local file, which is the backup the user already has.
                    root.reviewSource = "import";
                    root.backedUp = true;
                    Qt.callLater(function () {
                        const index = root.ownKeys.findIndex(key => key.id.toUpperCase() === root.importedFingerprint.toUpperCase());
                        if (index >= 0) {
                            existingChoice.checked = true;
                            existingKeys.currentIndex = index;
                        }
                    });
                    root.step = 2;
                } else {
                    root.errorText = KI18n.i18n("The import returned no key.");
                }
                return;
            }
            if (!root.creating)
                return;
            root.creating = false;
            root.chosenKey = fingerprint;
            root.step = 3;
        }
        function onOperationFailed(message) {
            if (root.importing) {
                root.importing = false;
                root.keyStore.controller.clearOperationError();
                if (!message.toLowerCase().includes("cancel"))
                    root.errorText = message;
                return;
            }
            if (!root.creating)
                return;
            root.creating = false;
            // The error belongs to this screen; keep it out of the Keys view.
            root.keyStore.controller.clearOperationError();
            if (!message.toLowerCase().includes("cancel"))
                root.errorText = message;
        }
    }

    // Swallows pointer input so nothing behind the cover reacts.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    Item {
        id: setupHost
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 29)
        readonly property var currentPage: setupPages.children[root.step] || null
        height: Math.min(parent.height - Kirigami.Units.gridUnit * 2, currentPage && currentPage.implicitHeight > 0 ? currentPage.implicitHeight : setupPages.implicitHeight)
        opacity: 0
        transform: Translate {
            id: stepShift
            x: 0
        }
        Component.onCompleted: stepFade.start()

        StackLayout {
            id: setupPages
            anchors.fill: parent
            currentIndex: root.step

            // 0 — welcome
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    Layout.alignment: Qt.AlignHCenter
                    source: root.logo.toString() || "security-high"
                    fallback: "security-high"
                    implicitWidth: Kirigami.Units.iconSizes.enormous
                    implicitHeight: Kirigami.Units.iconSizes.enormous
                }
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 1
                    text: KI18n.i18nc("@title", "Welcome to %1", Application.displayName)
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    Controls.Label {
                        text: KI18n.i18n("Passwords and notes encrypted with OpenPGP, only on your computer.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                }
                Controls.Button {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    text: KI18n.i18nc("@action:button", "Create New Vault")
                    icon.name: "document-new"
                    highlighted: true
                    onClicked: root.step = 1
                }
                Controls.ToolButton {
                    Layout.alignment: Qt.AlignHCenter
                    text: KI18n.i18nc("@action:button", "Restore a Backup…")
                    icon.name: "document-revert"
                    onClicked: root.restoreRequested()
                }
            }

            // 1 — vault key: two clear alternatives, the dedicated key first.
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    Layout.alignment: Qt.AlignHCenter
                    source: "view-certificate"
                    implicitWidth: Kirigami.Units.iconSizes.huge
                    implicitHeight: Kirigami.Units.iconSizes.huge
                }
                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    Kirigami.Heading {
                        level: 1
                        text: KI18n.i18nc("@title", "Vault key")
                    }
                    HelpButton {
                        help: KI18n.i18n("Everything you save is encrypted with this key. Only someone who holds its private part and knows its passphrase can open the vault.")
                    }
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: root.errorText.length > 0
                    type: Kirigami.MessageType.Error
                    text: root.errorText
                }

                Controls.ButtonGroup {
                    id: keyChoice
                }
                Controls.ItemDelegate {
                    id: createChoice
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    checkable: true
                    checked: true
                    enabled: !root.busy
                    Controls.ButtonGroup.group: keyChoice
                    Accessible.name: KI18n.i18nc("@option:radio", "Create a key for %1", Application.displayName)
                    background: Rectangle {
                        radius: Kirigami.Units.cornerRadius
                        color: createChoice.checked ? Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0.10) : createChoice.hovered ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                        border.width: createChoice.checked ? 2 : 1
                        border.color: createChoice.checked ? Kirigami.Theme.highlightColor : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.RadioButton {
                            Layout.alignment: Qt.AlignTop
                            checked: createChoice.checked
                            focusPolicy: Qt.NoFocus
                            onClicked: createChoice.checked = true
                        }
                        Kirigami.Icon {
                            Layout.alignment: Qt.AlignTop
                            source: "view-certificate-add"
                            selected: createChoice.highlighted
                            implicitWidth: Kirigami.Units.iconSizes.medium
                            implicitHeight: Kirigami.Units.iconSizes.medium
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            RowLayout {
                                // A custom contentItem does not inherit the highlighted text color.
                                Controls.Label {
                                    text: KI18n.i18nc("@option:radio", "Create a key for %1", Application.displayName)
                                    font.weight: Font.DemiBold
                                    color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                                }
                                Controls.Label {
                                    text: KI18n.i18nc("@label key option", "Recommended")
                                    color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.positiveTextColor
                                    font: Kirigami.Theme.smallFont
                                }
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: KI18n.i18n("A key only for your passwords. When locking, %1 closes it completely without affecting anything else.", Application.displayName)
                                wrapMode: Text.Wrap
                                color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                                opacity: createChoice.highlighted ? 0.85 : 1
                            }
                        }
                    }
                }
                Controls.ItemDelegate {
                    id: existingChoice
                    Layout.fillWidth: true
                    checkable: true
                    enabled: !root.busy
                    Controls.ButtonGroup.group: keyChoice
                    Accessible.name: KI18n.i18nc("@option:radio", "Use one of my keys")
                    background: Rectangle {
                        radius: Kirigami.Units.cornerRadius
                        color: existingChoice.checked ? Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0.10) : existingChoice.hovered ? Kirigami.Theme.alternateBackgroundColor : "transparent"
                        border.width: existingChoice.checked ? 2 : 1
                        border.color: existingChoice.checked ? Kirigami.Theme.highlightColor : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Controls.RadioButton {
                            Layout.alignment: Qt.AlignTop
                            checked: existingChoice.checked
                            focusPolicy: Qt.NoFocus
                            onClicked: existingChoice.checked = true
                        }
                        Kirigami.Icon {
                            Layout.alignment: Qt.AlignTop
                            source: "view-certificate"
                            selected: existingChoice.highlighted
                            implicitWidth: Kirigami.Units.iconSizes.medium
                            implicitHeight: Kirigami.Units.iconSizes.medium
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                text: KI18n.i18nc("@option:radio", "Use one of my keys")
                                font.weight: Font.DemiBold
                                color: existingChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                visible: root.ownKeys.length === 0
                                text: KI18n.i18n("There are no private keys on this computer.")
                                wrapMode: Text.Wrap
                                color: existingChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                                opacity: existingChoice.highlighted ? 0.85 : 1
                            }
                            Controls.ComboBox {
                                id: existingKeys
                                Layout.fillWidth: true
                                visible: root.ownKeys.length > 0
                                enabled: existingChoice.checked && !root.busy
                                model: root.ownKeys.map(key => (key.title || key.email) + " · " + key.id.slice(-8))
                                Accessible.name: KI18n.i18nc("@label:listbox", "Existing key")
                            }
                            Controls.Button {
                                Layout.alignment: Qt.AlignLeft
                                text: KI18n.i18nc("@action:button", "Import a Private Key…")
                                icon.name: "document-import"
                                enabled: !root.busy
                                onClicked: {
                                    existingChoice.checked = true;
                                    importDialog.open();
                                }
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    Controls.Button {
                        text: KI18n.i18nc("@action:button", "Back")
                        icon.name: "go-previous"
                        enabled: !root.busy
                        onClicked: root.step = 0
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    Controls.Button {
                        text: root.creating ? KI18n.i18n("Creating the key…") : KI18n.i18nc("@action:button", "Continue")
                        icon.name: "go-next"
                        highlighted: true
                        enabled: !root.busy && (createChoice.checked || (existingChoice.checked && existingKeys.currentIndex >= 0 && root.ownKeys.length > 0))
                        onClicked: {
                            if (createChoice.checked)
                                root.createVaultKey();
                            else
                                root.useExistingKey();
                        }
                    }
                }
            }

            // 2 — review an imported or selected key before it becomes the vault key.
            KeyReviewPage {
                title: root.reviewSource === "import" ? KI18n.i18nc("@title", "Review the imported key") : KI18n.i18nc("@title", "Review the selected key")
                subtitle: KI18n.i18n("Verify your key's fingerprint and mark it as your own to use it in your vault.")
                selectedKey: root.importedKey
                keyStore: root.keyStore
                importedFromFile: root.reviewSource === "import"
                onBackRequested: root.step = 1
                onAccepted: {
                    root.chosenKey = root.importedFingerprint;
                    root.step = 3;
                }
            }

            // 3 — back up the new key
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    Layout.alignment: Qt.AlignHCenter
                    source: root.backedUp ? "dialog-ok-apply" : "document-save"
                    implicitWidth: Kirigami.Units.iconSizes.huge
                    implicitHeight: Kirigami.Units.iconSizes.huge
                }
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 1
                    text: root.backedUp ? KI18n.i18nc("@title", "Backup saved") : KI18n.i18nc("@title", "Save a backup of your key")
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    Layout.alignment: Qt.AlignHCenter
                    visible: !root.backedUp
                    Controls.Label {
                        text: KI18n.i18n("Without it you will not be able to open the vault if you lose this computer.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                }
                Controls.Button {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.topMargin: Kirigami.Units.largeSpacing
                    visible: !root.backedUp
                    text: KI18n.i18nc("@action:button", "Save Key Backup…")
                    icon.name: "document-save-as"
                    highlighted: true
                    enabled: !root.busy
                    onClicked: keyOperation.openFor("backupPrivate", root.chosenKey)
                }
                Controls.Button {
                    Layout.alignment: Qt.AlignHCenter
                    Layout.topMargin: root.backedUp ? Kirigami.Units.largeSpacing : 0
                    text: root.backedUp ? KI18n.i18nc("@action:button", "Open the Vault") : KI18n.i18nc("@action:button", "Later")
                    icon.name: root.backedUp ? "go-next" : ""
                    highlighted: root.backedUp
                    flat: !root.backedUp
                    enabled: !root.busy
                    onClicked: root.setupFinished(root.chosenKey, true)
                }
            }
        }
    }

    ParallelAnimation {
        id: stepFade
        NumberAnimation {
            target: setupHost
            property: "opacity"
            to: 1
            duration: 150
            easing.type: Easing.OutCubic
        }
        NumberAnimation {
            target: stepShift
            property: "x"
            to: 0
            duration: 150
            easing.type: Easing.OutCubic
        }
    }

    // One discreet way into the handbook instead of a "?" on every line.
    Controls.ToolButton {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Kirigami.Units.largeSpacing * 2
        text: KI18n.i18nc("@action:button", "Need help?")
        icon.name: "help-contents"
        onClicked: root.handbookRequested("first-run")
    }

    Dialogs.FileDialog {
        id: importDialog
        title: KI18n.i18nc("@title:window", "Select OpenPGP Key to Import")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [KI18n.i18n("OpenPGP files (*.asc *.gpg *.pgp *.key)"), KI18n.i18n("All files (*)")]
        onAccepted: root.importKey(selectedFile)
    }

    KeyOperationDialog {
        id: keyOperation
        keyStore: root.keyStore
        onClosed: {
            if (action === "backupPrivate" && completionMessage) {
                root.backedUp = true;
                return;
            }
        }
    }
}
