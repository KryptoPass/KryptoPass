pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls

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

    title: qsTr("Rellenar en el navegador")
    confirmText: qsTr("Permitir relleno")
    cancelText: qsTr("Rechazar")
    listAccessibleName: qsTr("Credenciales disponibles")
    emptyText: qsTr("No hay credenciales disponibles.")
    noMatchesText: qsTr("No hay coincidencias. Prueba con otro nombre o carpeta.")
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
            subtitle: candidate.usernameState === "unloaded" ? qsTr("Selecciona para ver el usuario")
                : candidate.usernameState === "loading" ? qsTr("Cargando usuario…")
                : candidate.usernameState === "error" ? qsTr("No se pudo leer el usuario")
                : candidate.username || qsTr("Sin usuario")
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
    onCandidateHighlighted: function(entryId) {
        const candidate = candidates.find(item => item.id === entryId);
        if (request && candidate && candidate.usernameState === "unloaded")
            usernameRequested(request.token, entryId);
    }
    onVaultLockedChanged: {
        if (vaultLocked && opened)
            close();
    }
    onSelectionRequested: function(entryId) { decide(entryId, true); }
    onClosed: decide("", false)

    introduction: Component {
        ColumnLayout {
            spacing: 8
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("El navegador solicita usuario y contraseña para:")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: dialog.request ? dialog.request.origin : ""
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
                font.bold: true
                Accessible.name: qsTr("Sitio solicitado: %1").arg(text)
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Elige una credencial. Solo se enviará si tiene guardado este mismo sitio. La autorización sirve para esta solicitud.")
                wrapMode: Text.Wrap
            }
        }
    }
}
