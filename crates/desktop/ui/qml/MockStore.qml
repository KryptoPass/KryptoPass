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
            kind: "login",
            format: "Texto",
            value: "usuario: ana@example.test\ncontraseña: ejemplo-correo-123",
            details: {
                url: "https://correo.example.test",
                username: "ana@example.test",
                password: "ejemplo-correo-123"
            },
            notes: "Cuenta de demostración",
            recipients: "Llave A",
            favorite: true,
            trashed: false
        },
        {
            id: "2",
            folder: "personal",
            title: "Nota privada",
            kind: "note",
            format: "Texto",
            value: "Esta nota es ficticia. Aquí puedes probar el editor de KryptoPass.",
            details: {},
            notes: "",
            recipients: "Llave A, Llave B",
            favorite: false,
            trashed: false
        },
        {
            id: "3",
            folder: "finance",
            title: "Código de recuperación",
            kind: "note",
            format: "Texto",
            value: "DEMO-0000-1111-2222",
            details: {},
            notes: "Ejemplo sin valor real",
            recipients: "Llave B",
            favorite: true,
            trashed: false
        },
        {
            id: "4",
            folder: "work",
            title: "VPN de pruebas",
            kind: "login",
            format: "Texto",
            value: "servidor: vpn.example.test\nusuario: demo\ncontraseña: solo-demostracion",
            details: {
                url: "https://vpn.example.test",
                username: "demo",
                password: "solo-demostracion"
            },
            notes: "Entorno de pruebas",
            recipients: "Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "5",
            folder: "projects",
            title: "Token de desarrollo",
            kind: "note",
            format: "Texto",
            value: "demo_token_abcdef123456",
            details: {},
            notes: "API ficticia",
            recipients: "Llave A, Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "6",
            folder: "work",
            title: "Certificado de prueba",
            kind: "note",
            format: "Binario",
            value: "00 01 02 03 AB CD EF",
            details: {},
            notes: "Bytes ficticios en hexadecimal",
            recipients: "Llave C",
            favorite: false,
            trashed: false
        },
        {
            id: "7",
            folder: "personal",
            title: "Nota antigua",
            kind: "note",
            format: "Texto",
            value: "Ejemplo en la papelera",
            details: {},
            notes: "",
            recipients: "Llave A",
            favorite: false,
            trashed: true
        },
        {
            id: "8",
            folder: "finance",
            title: "Tarjeta de demostración",
            kind: "card",
            format: "Texto",
            value: "",
            details: {
                cardNumber: "4242 4242 4242 4242",
                cardHolder: "ANA DEMO",
                expiry: "12/30",
                cvv: "123"
            },
            notes: "Datos ficticios; no es una tarjeta real.",
            recipients: "Llave A, Llave B",
            favorite: false,
            trashed: false
        },
        {
            id: "9",
            folder: "personal",
            title: "Identidad de demostración",
            kind: "identity",
            format: "Texto",
            value: "",
            details: {
                fullName: "Ana Ejemplo",
                document: "DEMO-123456",
                birthDate: "1988-04-12",
                email: "ana@example.test",
                phone: "+00 555 0100"
            },
            notes: "Identidad ficticia para probar la vista.",
            recipients: "Llave A",
            favorite: false,
            trashed: false
        }
    ]
    readonly property var kindOptions: [
        {
            id: "card",
            name: qsTr("Tarjeta"),
            icon: "wallet-open"
        },
        {
            id: "identity",
            name: qsTr("Identidad"),
            icon: "user-identity"
        },
        {
            id: "login",
            name: qsTr("Inicio de sesión"),
            icon: "user-identity"
        },
        {
            id: "note",
            name: qsTr("Nota"),
            icon: "document-edit"
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
    function kindName(kind) {
        return kindOptions.find(k => k.id === kind)?.name || qsTr("Nota");
    }
    function kindIcon(kind) {
        return kindOptions.find(k => k.id === kind)?.icon || "document-edit";
    }
    function count(id) {
        return entries.filter(e => id === "trash" ? e.trashed : !e.trashed && (id === "all" || (id === "favorites" ? e.favorite : e.folder === id))).length;
    }
    function selectScope(id) {
        scope = id;
        selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    function saveEntry(id, folder, title, kind, format, value, notes, recipients, details) {
        if (!title.trim() || !recipients.trim())
            return false;
        const old = entries.find(e => e.id === id);
        const entry = {
            id: id || "e" + nextId++,
            folder: folder,
            title: title.trim(),
            kind: kind || "note",
            format: format,
            value: value,
            details: details || {},
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
