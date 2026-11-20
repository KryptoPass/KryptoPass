pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.ki18n

EntryPickerDialog {
    id: dialog
    required property string requestJson
    required property bool vaultLocked
    property var entryMetadata: []
    property var folders: []
    readonly property var request: requestJson ? JSON.parse(requestJson) : null
    readonly property var candidates: request ? request.candidates : []
    readonly property string selectedEntryId: pendingId
    property bool decisionMade: false
    property string displayedToken: ""
    readonly property var selectedCandidate: candidates.find(candidate => candidate.id === pendingId)

    signal decisionRequested(string token, string entryId, bool approve)
    signal usernameRequested(string token, string entryId)

    title: KI18n.i18nc("@title:window", "Fill in the Browser")
    confirmText: KI18n.i18nc("@action:button", "Allow Filling")
    cancelText: KI18n.i18nc("@action:button", "Reject")
    listAccessibleName: KI18n.i18n("Available logins")
    emptyText: KI18n.i18n("No logins are available.")
    noMatchesText: KI18n.i18n("No matches. Try another name or folder.")
    initialFocusReject: true
    preserveSelectionOnModelChange: true
    selectionAllowed: !!selectedCandidate && (!selectedCandidate.usernameState || selectedCandidate.usernameState === "ready")
    options: candidates.map(candidate => {
        const metadata = entryMetadata.find(entry => entry.id === candidate.id);
        const folder = metadata ? metadata.folder || "" : "";
        return {
            id: candidate.id,
            title: candidate.name,
            kind: "login",
            folder: folder,
            icon: metadata ? metadata.icon || null : null,
            iconSource: metadata ? metadata.iconSource || "" : "",
            subtitle: candidate.usernameState === "unloaded" ? KI18n.i18n("Select to see the username") : candidate.usernameState === "loading" ? KI18n.i18nc("@info:progress", "Loading username…") : candidate.usernameState === "error" ? KI18n.i18n("The username could not be read") : candidate.username || KI18n.i18n("No username")
        };
    })

    function decide(entryId, approve) {
        if (request && !decisionMade) {
            decisionMade = true;
            decisionRequested(request.token, entryId, approve && !vaultLocked);
        }
    }

    onRequestChanged: {
        const token = request ? request.token : "";
        if (displayedToken !== token) {
            displayedToken = token;
            decisionMade = false;
            resetSelection();
        }
        if (requestJson && !vaultLocked)
            open();
        else
            close();
    }
    onCandidateHighlighted: function (entryId) {
        const candidate = candidates.find(item => item.id === entryId);
        if (request && candidate && candidate.usernameState === "unloaded")
            usernameRequested(request.token, entryId);
    }
    onVaultLockedChanged: {
        if (vaultLocked && opened)
            close();
    }
    onSelectionRequested: function (entryId) {
        decide(entryId, true);
    }
    onClosed: decide("", false)

    introduction: Component {
        ColumnLayout {
            spacing: 8
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("The browser is requesting a username and password for:")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: dialog.request ? dialog.request.origin : ""
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
                font.bold: true
                Accessible.name: KI18n.i18n("Requested site: %1", text)
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("Choose a login. It is sent only if it has this same site saved. The authorization covers this request.")
                wrapMode: Text.Wrap
            }
        }
    }
}
