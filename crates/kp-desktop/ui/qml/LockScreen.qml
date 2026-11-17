pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kde.ki18n

// Covers the vault while it is locked so names, folders and counts stay hidden.
Rectangle {
    id: root

    required property KeyStore keyStore

    property bool unlockEnabled: true
    property bool unlocking: false
    property bool canReviewKey: false
    property string primaryFingerprint: ""
    property string errorMessage: ""
    property string localError: ""
    property bool importing: false
    property string reviewFingerprint: ""
    property bool reviewImportedFromFile: false

    readonly property bool keyBusy: importing || keyStore.controller.busy || keyStore.controller.operationBusy
    readonly property var reviewKey: reviewFingerprint ? (keyStore.entries.find(key => key.id.toUpperCase() === reviewFingerprint.toUpperCase()) || null) : null

    signal unlockRequested
    signal restoreRequested
    signal quitRequested

    function openReview(fingerprint, imported) {
        if (!fingerprint)
            return;
        localError = "";
        reviewImportedFromFile = imported;
        reviewFingerprint = fingerprint;
    }

    function closeReview() {
        reviewFingerprint = "";
        reviewImportedFromFile = false;
        localError = "";
    }

    function importKey(url) {
        if (keyBusy)
            return;
        localError = "";
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
            if (!root.importing)
                return;
            root.importing = false;
            if (fingerprint)
                root.openReview(fingerprint, true);
            else
                root.localError = KI18n.i18n("The import returned no key.");
        }
        function onOperationFailed(message) {
            if (!root.importing)
                return;
            root.importing = false;
            root.keyStore.controller.clearOperationError();
            if (!message.toLowerCase().includes("cancel"))
                root.localError = message;
        }
    }

    color: Kirigami.Theme.backgroundColor
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    onVisibleChanged: {
        if (visible)
            Qt.callLater(unlockButton.forceActiveFocus);
    }

    // Swallows pointer input so nothing behind the cover reacts.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    ColumnLayout {
        visible: root.reviewFingerprint.length === 0
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 26)
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.alignment: Qt.AlignHCenter
            source: "object-locked"
            implicitWidth: Kirigami.Units.iconSizes.enormous
            implicitHeight: Kirigami.Units.iconSizes.enormous
            color: Kirigami.Theme.disabledTextColor
        }
        Kirigami.Heading {
            Layout.fillWidth: true
            level: 1
            text: KI18n.i18n("%1 is locked", Application.displayName)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            type: Kirigami.MessageType.Warning
            text: root.localError || root.errorMessage
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            visible: root.importing
            spacing: Kirigami.Units.smallSpacing
            Controls.BusyIndicator {
                running: root.importing
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: implicitWidth
                Accessible.name: KI18n.i18n("Importing key")
            }
            Controls.Label {
                text: KI18n.i18n("Importing key…")
                color: Kirigami.Theme.disabledTextColor
            }
        }
        Controls.Button {
            id: unlockButton
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: root.unlocking ? KI18n.i18n("Unlocking…") : KI18n.i18nc("@action:button", "Unlock")
            icon.name: "object-unlocked"
            highlighted: true
            enabled: root.unlockEnabled && !root.unlocking && !root.importing
            onClicked: {
                root.localError = "";
                root.unlockRequested();
            }
            Keys.onReturnPressed: {
                root.localError = "";
                root.unlockRequested();
            }
            Keys.onEnterPressed: {
                root.localError = "";
                root.unlockRequested();
            }
        }
        Flow {
            Layout.alignment: Qt.AlignHCenter
            Layout.fillWidth: true
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                visible: root.canReviewKey
                text: KI18n.i18nc("@action:button", "Review Key…")
                icon.name: "view-certificate"
                enabled: !root.keyBusy
                onClicked: root.openReview(root.primaryFingerprint, false)
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Import Private Key…")
                icon.name: "document-import"
                enabled: !root.keyBusy
                onClicked: importDialog.open()
            }
            Controls.Button {
                text: KI18n.i18nc("@action:button", "Restore Backup…")
                icon.name: "document-revert"
                enabled: !root.keyBusy
                onClicked: root.restoreRequested()
            }
        }
        Controls.ToolButton {
            Layout.alignment: Qt.AlignHCenter
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            text: KI18n.i18nc("@action:button", "Quit")
            icon.name: "application-exit"
            enabled: !root.keyBusy
            onClicked: root.quitRequested()
        }
    }

    KeyReviewPage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 29)
        visible: root.reviewFingerprint.length > 0
        title: root.reviewImportedFromFile ? KI18n.i18nc("@title", "Review the imported key") : KI18n.i18nc("@title", "Review the vault key")
        subtitle: root.reviewImportedFromFile ? KI18n.i18n("Verify the fingerprint and mark the key as your own to unlock the vault.") : KI18n.i18n("Mark the key as your own to unlock the vault.")
        selectedKey: root.reviewKey
        keyStore: root.keyStore
        importedFromFile: root.reviewImportedFromFile
        onBackRequested: root.closeReview()
        onAccepted: {
            root.closeReview();
            root.unlockRequested();
        }
    }

    Dialogs.FileDialog {
        id: importDialog
        title: KI18n.i18nc("@title:window", "Select OpenPGP Key to Import")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [KI18n.i18n("OpenPGP files (*.asc *.gpg *.pgp *.key)"), KI18n.i18n("All files (*)")]
        onAccepted: root.importKey(selectedFile)
    }
}
