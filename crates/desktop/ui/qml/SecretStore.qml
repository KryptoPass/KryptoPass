pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop

QtObject {
    id: store

    readonly property SecretsController controller: SecretsController {}
    property string scope: "all"
    property string query: ""
    property string selectedId: ""
    property bool sortDescending: false
    readonly property string status: controller.status
    readonly property string errorMessage: controller.errorMessage
    readonly property bool busy: controller.busy || controller.detailBusy || controller.editorBusy || controller.saving
    readonly property var kindOptions: [
        {
            id: "login",
            name: qsTr("Inicio de sesión"),
            icon: "network-server"
        },
        {
            id: "note",
            name: qsTr("Nota"),
            icon: "document-edit"
        }
    ]
    readonly property var entries: parseRecords(controller.entryRecords)
    readonly property var folders: parseRecords(controller.folderRecords)
    readonly property var selected: visibleEntries.find(entry => entry.id === selectedId) || null
    readonly property var detail: parseRecord(controller.detailJson)
    readonly property var historyVersion: parseRecord(controller.versionJson)
    readonly property var visibleEntries: {
        const needle = query.trim().toLowerCase();
        const filtered = entries.filter(entry => {
            if (scope === "trash" ? !entry.trashed : entry.trashed)
                return false;
            if (scope === "favorites" && !entry.favorite)
                return false;
            if (!["all", "favorites", "trash"].includes(scope) && entry.folder !== scope)
                return false;
            if (!needle)
                return true;
            const searchText = [entry.title, folderName(entry.folder), kindName(entry.kind)].join(" ").toLowerCase();
            return searchText.includes(needle);
        });
        return filtered.sort((left, right) => {
            const result = left.title.localeCompare(right.title, undefined, {
                sensitivity: "base",
                numeric: true
            }) || left.id.localeCompare(right.id);
            return sortDescending ? -result : result;
        });
    }
    readonly property var folderRows: {
        const rows = [];
        const visited = Object.create(null);
        const orderedFolders = folders.slice().sort((left, right) => (left.position - right.position) || left.name.localeCompare(right.name, undefined, {
            sensitivity: "base",
            numeric: true
        }) || left.id.localeCompare(right.id));
        const walk = (parentId, depth) => {
            if (depth > orderedFolders.length)
                return;
            for (const folder of orderedFolders) {
                if (folder.parent !== parentId || visited[folder.id])
                    continue;
                visited[folder.id] = true;
                rows.push({
                    id: folder.id,
                    parent: folder.parent || "",
                    name: folder.name,
                    depth: depth,
                    icon: folder.icon || "folder",
                    color: folder.color || "transparent"
                });
                walk(folder.id, depth + 1);
            }
        };
        walk("", 0);
        return rows;
    }

    function canMoveFolder(id, parentId) {
        if (!folders.some(folder => folder.id === id))
            return false;
        let current = parentId;
        const visited = Object.create(null);
        while (current) {
            if (current === id || visited[current])
                return false;
            visited[current] = true;
            const folder = folders.find(candidate => candidate.id === current);
            if (!folder)
                return false;
            current = folder.parent || "";
        }
        return true;
    }

    function parseRecords(records) {
        const parsed = [];
        for (const record of Array.from(records || [])) {
            try {
                parsed.push(JSON.parse(record));
            } catch (error) {
                console.warn("Ignoring an invalid metadata record from SecretsController");
            }
        }
        return parsed;
    }

    function parseRecord(record) {
        if (!record)
            return null;
        try {
            return JSON.parse(record);
        } catch (error) {
            return null;
        }
    }

    function folderName(id) {
        const folder = folders.find(candidate => candidate.id === id);
        return folder ? folder.name : qsTr("Sin carpeta");
    }

    function folderIcon(id) {
        const folder = folders.find(candidate => candidate.id === id);
        return folder ? (folder.icon || "folder") : "folder";
    }

    function folderColor(id) {
        const folder = folders.find(candidate => candidate.id === id);
        return folder ? (folder.color || "transparent") : "transparent";
    }

    function kindName(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option ? option.name : qsTr("Tipo desconocido");
    }

    function kindIcon(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option ? option.icon : "dialog-warning";
    }

    function count(id) {
        return entries.filter(entry => {
            if (id === "trash")
                return entry.trashed;
            if (entry.trashed)
                return false;
            if (id === "all")
                return true;
            if (id === "favorites")
                return entry.favorite;
            return entry.folder === id;
        }).length;
    }

    function selectScope(id) {
        scope = id;
        selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }

    function updateEntry(id, changes) {
        if (Object.prototype.hasOwnProperty.call(changes, "favorite"))
            controller.setFavorite(id, changes.favorite);
        if (Object.prototype.hasOwnProperty.call(changes, "trashed"))
            controller.setTrashed(id, changes.trashed);
        if (Object.prototype.hasOwnProperty.call(changes, "folder"))
            controller.moveEntry(id, changes.folder);
    }

    function saveFolder(id, name, parentId, icon, color) {
        controller.saveFolder(id, name, parentId, icon, color);
    }

    function removeFolder(id) {
        controller.removeFolder(id);
    }

    function removeEntry(id) {
        controller.removeEntry(id);
    }

    onSelectedIdChanged: controller.selectEntry(selectedId)
    onVisibleEntriesChanged: {
        if (!visibleEntries.some(entry => entry.id === selectedId))
            selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    onFoldersChanged: {
        if (!["all", "favorites", "trash"].includes(scope)
                && !folders.some(folder => folder.id === scope))
            selectScope("all");
    }

    readonly property Connections controllerConnections: Connections {
        target: store.controller

        function onEntrySaved(id) {
            const saved = store.entries.find(entry => entry.id === id);
            store.scope = saved && saved.folder ? saved.folder : "all";
            store.query = "";
            if (store.selectedId !== id)
                store.selectedId = id;
            else
                store.controller.selectEntry(id);
        }

        function onFolderSaved(id) {
            store.scope = id;
            store.query = "";
        }
    }
}
