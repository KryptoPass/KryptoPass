pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var entries: []
    property var kindOptions: []
    property string entryId: ""
    property string relatedEntryId: ""

    readonly property var options: {
        const choices = [{ id: "", label: qsTr("Sin vínculo"), unavailable: false }];
        const eligible = entries.filter(candidate => !candidate.trashed
            && candidate.id !== entryId
            && (!candidate.relatedEntryId || candidate.relatedEntryId === entryId));
        eligible.sort((left, right) => left.title.localeCompare(right.title, undefined, {
            sensitivity: "base",
            numeric: true
        }) || left.id.localeCompare(right.id));
        for (const candidate of eligible) {
            choices.push({
                id: candidate.id,
                label: qsTr("%1 · %2").arg(candidate.title).arg(kindName(candidate.kind)),
                unavailable: false
            });
        }

        const current = relatedEntryId ? entries.find(candidate => candidate.id === relatedEntryId) : null;
        if (relatedEntryId && (!current || current.trashed
                || !eligible.some(candidate => candidate.id === relatedEntryId))) {
            choices.push({
                id: relatedEntryId,
                label: !current
                    ? qsTr("Elemento vinculado no disponible")
                    : current.trashed
                      ? qsTr("%1 · %2 — En la papelera").arg(current.title).arg(kindName(current.kind))
                      : qsTr("%1 · %2 — No disponible").arg(current.title).arg(kindName(current.kind)),
                unavailable: true
            });
        }
        return choices;
    }

    signal selectionChanged(string relatedEntryId)

    function kindName(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option ? option.name : qsTr("Tipo desconocido");
    }

    function syncSelection() {
        const index = relatedEntryField.indexOfValue(relatedEntryId);
        relatedEntryField.currentIndex = index >= 0 ? index : 0;
    }

    function selectRelatedEntry(id) {
        const index = relatedEntryField.indexOfValue(id || "");
        if (index < 0 || options[index].unavailable)
            return false;
        selectionChanged(options[index].id);
        return true;
    }

    onRelatedEntryIdChanged: Qt.callLater(syncSelection)
    onOptionsChanged: Qt.callLater(syncSelection)

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        Layout.fillWidth: true
        text: qsTr("Elemento vinculado")
    }

    Controls.ComboBox {
        id: relatedEntryField
        objectName: "relatedEntryField"
        Layout.fillWidth: true
        model: root.options
        textRole: "label"
        valueRole: "id"
        Accessible.name: qsTr("Elemento vinculado")
        Accessible.description: qsTr("Vincula un elemento complementario. Ambos elementos mostrarán esta relación.")
        delegate: Controls.ItemDelegate {
            required property var modelData
            width: relatedEntryField.width
            text: modelData.label
            enabled: !modelData.unavailable
        }
        onActivated: {
            if (!root.selectRelatedEntry(currentValue || ""))
                root.syncSelection();
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        text: qsTr("Vincula un elemento complementario. Ambos elementos mostrarán esta relación.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
    }
}
