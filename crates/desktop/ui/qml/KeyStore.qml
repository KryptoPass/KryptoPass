import QtQuick
import org.kryptopass.desktop

QtObject {
    id: store
    readonly property KeyringController controller: KeyringController {}
    property string scope: "all"
    property string query: ""
    property string selectedId: ""
    readonly property var entries: Array.from(controller.keyRecords, record => JSON.parse(record))
    readonly property var visibleEntries: entries.filter(key => {
        if (scope === "own" && !key.hasSecret)
            return false;
        if (scope === "contacts" && key.hasSecret)
            return false;
        return (key.title + " " + key.email + " " + key.id).toLowerCase().includes(query.replace(/\s+/g, " ").trim().toLowerCase()) || key.id.toLowerCase().includes(query.replace(/\s/g, "").toLowerCase());
    }).sort((a, b) => a.title.localeCompare(b.title, undefined, {
            sensitivity: "base",
            numeric: true
        }) || a.email.localeCompare(b.email) || a.id.localeCompare(b.id))
    readonly property var selected: visibleEntries.find(key => key.id === selectedId) || null
    onVisibleEntriesChanged: {
        if (!visibleEntries.some(key => key.id === selectedId))
            selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    function count(id) {
        return entries.filter(key => id === "all" || (id === "own" ? key.hasSecret : !key.hasSecret)).length;
    }
    function selectScope(id) {
        scope = id;
    }
}
