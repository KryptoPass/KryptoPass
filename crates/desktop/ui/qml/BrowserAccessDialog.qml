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

    signal decisionRequested(string token, string entryId, bool approve)

    title: qsTr("Rellenar en el navegador")
    confirmText: qsTr("Permitir relleno")
    cancelText: qsTr("Rechazar")
    listAccessibleName: qsTr("Credenciales disponibles")
    emptyText: qsTr("No hay credenciales disponibles.")
    noMatchesText: qsTr("No hay coincidencias. Prueba con otro nombre o carpeta.")
    initialFocusReject: true
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
            subtitle: qsTr("Credencial · %1").arg(folderPath(folder))
        };
    })

    function folderPath(folderId) {
        const names = [];
        const visited = new Set();
        let id = folderId;
        while (id && !visited.has(id)) {
            visited.add(id);
            const folder = folders.find(candidate => candidate.id === id);
            if (!folder)
                break;
            names.unshift(folder.name);
            id = folder.parent || "";
        }
        return names.length ? names.join(" / ") : qsTr("Sin carpeta");
    }

    function decide(entryId, approve) {
        if (request && !decisionMade) {
            decisionMade = true;
            decisionRequested(request.token, entryId, approve && !vaultLocked);
        }
    }

    onRequestJsonChanged: {
        decisionMade = false;
        resetSelection();
        if (requestJson && !vaultLocked)
            open();
        else
            close();
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
