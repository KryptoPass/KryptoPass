import QtQuick

QtObject {
    id: store
    // UI-only fixtures. Every value is invented and resets on restart.
    property var folders: [
        {
            id: "personal",
            name: "Personal",
            parent: ""
        },
        {
            id: "finance",
            name: "Finanzas",
            parent: "personal"
        },
        {
            id: "work",
            name: "Trabajo",
            parent: ""
        },
        {
            id: "projects",
            name: "Proyectos",
            parent: "work"
        }
    ]
    property var entries: [
        {
            id: "1",
            folder: "personal",
            title: "Correo personal",
            format: "Texto",
            value: "usuario: ana@example.test\ncontraseña: ejemplo-correo-123",
            notes: "Cuenta de demostración",
            recipients: "Llave A",
            favorite: true,
            trashed: false
        },
        {
            id: "2",
            folder: "personal",
            title: "Nota privada",
            format: "Texto",
            value: "Esta nota es ficticia. Aquí puedes probar el editor de KryptoPass.",
            notes: "",
            recipients: "Llave A, Llave B",
            favorite: false,
            trashed: false
        },
        {
            id: "3",
            folder: "finance",
            title: "Código de recuperación",
            format: "Texto",
            value: "DEMO-0000-1111-2222",
            notes: "Ejemplo sin valor real",
            recipients: "Llave B",
            favorite: true,
            trashed: false
        },
        {
            id: "4",
            folder: "work",
            title: "VPN de pruebas",
            format: "Texto",
            value: "servidor: vpn.example.test\nusuario: demo\ncontraseña: solo-demostracion",
            notes: "Entorno de pruebas",
            recipients: "Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "5",
            folder: "projects",
            title: "Token de desarrollo",
            format: "Texto",
            value: "demo_token_abcdef123456",
            notes: "API ficticia",
            recipients: "Llave A, Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "6",
            folder: "work",
            title: "Certificado de prueba",
            format: "Binario",
            value: "00 01 02 03 AB CD EF",
            notes: "Bytes ficticios en hexadecimal",
            recipients: "Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "7",
            folder: "personal",
            title: "Nota antigua",
            format: "Texto",
            value: "Ejemplo en la papelera",
            notes: "",
            recipients: "Llave A",
            favorite: false,
            trashed: true
        }
    ]
    property int nextId: 10
    property string scope: "all"
    property string query: ""
    property string selectedId: "1"
    readonly property var selected: entries.find(e => e.id === selectedId) || null
    onVisibleEntriesChanged: {
        if (!visibleEntries.some(e => e.id === selectedId))
            selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    readonly property var visibleEntries: entries.filter(e => {
        if (scope === "trash" ? !e.trashed : e.trashed)
            return false;
        if (scope === "favorites" && !e.favorite)
            return false;
        if (!["all", "favorites", "trash"].includes(scope) && e.folder !== scope)
            return false;
        return (e.title + " " + e.notes).toLowerCase().includes(query.toLowerCase());
    }).sort((a, b) => a.title.localeCompare(b.title))
    readonly property var folderRows: {
        const result = [];
        const walk = (parent, depth) => {
            for (const folder of folders.filter(f => f.parent === parent).sort((a, b) => a.name.localeCompare(b.name))) {
                result.push({
                    id: folder.id,
                    name: folder.name,
                    depth: depth
                });
                walk(folder.id, depth + 1);
            }
        };
        walk("", 0);
        return result;
    }
    function folderName(id) {
        return folders.find(f => f.id === id)?.name || qsTr("Sin carpeta");
    }
    function count(id) {
        return entries.filter(e => id === "trash" ? e.trashed : !e.trashed && (id === "all" || (id === "favorites" ? e.favorite : e.folder === id))).length;
    }
    function selectScope(id) {
        scope = id;
        selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    function saveEntry(id, folder, title, format, value, notes, recipients) {
        if (!title.trim() || !recipients.trim())
            return false;
        const old = entries.find(e => e.id === id);
        const entry = {
            id: id || "e" + nextId++,
            folder: folder,
            title: title.trim(),
            format: format,
            value: value,
            notes: notes,
            recipients: recipients,
            favorite: old ? old.favorite : false,
            trashed: false
        };
        entries = old ? entries.map(e => e.id === id ? entry : e) : entries.concat([entry]);
        scope = folder || "all";
        query = "";
        selectedId = entry.id;
        return true;
    }
    function updateEntry(id, changes) {
        entries = entries.map(e => e.id === id ? Object.assign({}, e, changes) : e);
    }
    function removeEntry(id) {
        entries = entries.filter(e => e.id !== id);
        selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    function saveFolder(id, name, parent) {
        if (!name.trim())
            return false;
        const folder = {
            id: id || "f" + nextId++,
            name: name.trim(),
            parent: parent
        };
        folders = id ? folders.map(f => f.id === id ? folder : f) : folders.concat([folder]);
        selectScope(folder.id);
        return true;
    }
    function removeFolder(id) {
        const folder = folders.find(f => f.id === id);
        if (!folder)
            return;
        // Preserve contents and reparent them when their folder is removed.
        folders = folders.filter(f => f.id !== id).map(f => f.parent === id ? Object.assign({}, f, {
                parent: folder.parent
            }) : f);
        entries = entries.map(e => e.folder === id ? Object.assign({}, e, {
                folder: folder.parent
            }) : e);
        selectScope("all");
    }
}
