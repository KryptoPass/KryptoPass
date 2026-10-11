import QtQuick
import org.kryptopass.desktop as Backend

QtObject {
    id: store
    readonly property Backend.KeyringController controller: Backend.KeyringController {}
    readonly property Backend.ContactsController contactsController: Backend.ContactsController {}
    readonly property var onlineSettings: JSON.parse(controller.onlineSettings)
    property string scope: "all"
    property string query: ""
    property string selectedId: ""
    property bool contactsOverviewVisible: false
    onScopeChanged: {
        if (scope !== "contacts")
            contactsOverviewVisible = false;
    }
    readonly property var contactRows: {
        const parsed = [];
        for (const record of contactsController.contactRecords) {
            try {
                parsed.push(JSON.parse(record));
            } catch (error) {
                // Ignore malformed rows from the native address-book projection.
            }
        }
        return parsed;
    }
    function contactFingerprints(contact) {
        const values = Array.isArray(contact.openPgpFingerprints) ? contact.openPgpFingerprints.slice() : [];
        if (contact.openPgpFingerprint)
            values.unshift(contact.openPgpFingerprint);
        const fingerprints = [];
        for (const value of values) {
            const normalized = (value || "").replace(/\s+/g, "").toUpperCase();
            if (/^(?:[A-F0-9]{40}|[A-F0-9]{64})$/.test(normalized) && !fingerprints.includes(normalized))
                fingerprints.push(normalized);
        }
        return fingerprints;
    }
    readonly property var entries: Array.from(controller.keyRecords, record => {
        const key = JSON.parse(record);
        const fingerprint = (key.id || "").toUpperCase();
        const linkedContacts = contactRows.filter(row => store.contactFingerprints(row).includes(fingerprint));
        const contactNames = linkedContacts.map(contact => contact.name || contact.email || contact.uid || "")
            .filter(name => !!name);
        return Object.assign({}, key, {
            contactName: contactNames.join(", "),
            contactUid: linkedContacts.length === 1 ? (linkedContacts[0].uid || "") : "",
            contactUids: linkedContacts.map(contact => contact.uid || "")
        });
    })
    readonly property var visibleEntries: entries.filter(key => {
        if (scope === "own" && !key.hasSecret)
            return false;
        if (scope === "contacts" && key.hasSecret)
            return false;
        return (key.title + " " + key.email + " " + key.contactName + " " + key.id).toLowerCase().includes(query.replace(/\s+/g, " ").trim().toLowerCase()) || key.id.toLowerCase().includes(query.replace(/\s/g, "").toLowerCase());
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
    function selectAfterRefresh(fingerprint) {
        const normalized = (fingerprint || "").toUpperCase();
        if (!normalized)
            return;
        scope = "all";
        query = "";
        const match = entries.find(key => key.id.toUpperCase() === normalized);
        if (match)
            selectedId = match.id;
    }
}
