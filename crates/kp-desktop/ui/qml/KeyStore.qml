import QtQuick
import org.kryptopass.desktop as Backend

QtObject {
    id: store
    readonly property Backend.KeyringController controller: Backend.KeyringController {}
    readonly property Backend.ContactsController contactsController: Backend.ContactsController {}
    readonly property var onlineSettings: JSON.parse(controller.onlineSettings)
    property string scope: "own"
    readonly property var availability: JSON.parse(controller.keyAvailability)
    readonly property var catalog: JSON.parse(controller.identityCatalog)
    readonly property var people: Object.entries(catalog.people || {}).map(([id, person]) => ({
                id,
                name: person.name
            })).sort((a, b) => a.name.localeCompare(b.name))
    property string selectedIdentityId: ""
    function editCatalog(request) {
        controller.editCatalog(JSON.stringify(request));
    }
    readonly property IdentityWording wording: IdentityWording {}
    readonly property var health: JSON.parse(controller.identityHealth)
    function dateText(seconds) {
        return wording.dateText(seconds) || qsTr("Sin registro");
    }
    function purposeText(purpose) {
        return wording.roleTitle(purpose);
    }
    property string query: ""
    property string selectedId: ""
    property bool contactsOverviewVisible: false
    /// The vault's primary key; the vault controller owns it.
    property string primaryKey: ""
    signal primaryKeyRequested(string fingerprint)
    /// Whether locking the vault makes gpg-agent forget the primary key's passphrase.
    property bool forgetOnLock: false
    signal forgetOnLockRequested(bool enabled)
    function isPrimary(fingerprint) {
        return !!primaryKey && (fingerprint || "").toUpperCase() === primaryKey.toUpperCase();
    }
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
        const contactNames = linkedContacts.map(contact => contact.name || contact.email || contact.uid || "").filter(name => !!name);
        const meta = catalog.identities?.[fingerprint] || {};
        const person = catalog.people?.[meta.person];
        const own = meta.own === undefined || meta.own === null ? key.hasSecret : meta.own;
        const verified = !!meta.verifiedAt && JSON.stringify(meta.verifiedUserIds) === JSON.stringify(key.identities.map(uid => uid.id));
        return Object.assign({}, key, {
            pgpTitle: key.title,
            subkeys: key.subkeys.map(component => Object.assign({}, component, {
                    location: availability[component.fingerprint]?.location || "unknown",
                    label: (meta.uses || []).filter(usage => usage.fingerprint === component.fingerprint).map(usage => usage.label).join(", ")
                })),
            title: meta.label || key.title,
            own: own,
            meta: meta,
            health: health[fingerprint] || null,
            personName: person?.name || "",
            verified: verified,
            verificationChanged: !!meta.verifiedAt && !verified,
            contactName: contactNames.join(", "),
            contactUid: linkedContacts.length === 1 ? (linkedContacts[0].uid || "") : "",
            contactUids: linkedContacts.map(contact => contact.uid || "")
        });
    })
    readonly property var personEntries: people.map(person => {
        const identities = entries.filter(key => !key.own && key.meta.person === person.id);
        return {
            id: "person:" + person.id,
            personId: person.id,
            isPerson: true,
            title: person.name,
            email: qsTr("%n identidad", "", identities.length),
            identities: identities,
            status: "",
            statusCode: "",
            own: false
        };
    }).concat(entries.filter(key => !key.own && !catalog.people?.[key.meta.person]))
    readonly property var visibleEntries: (scope === "contacts" ? personEntries : entries.filter(key => scope === "all" || key.own)).filter(key => {
        const searchable = key.isPerson ? key.identities.map(identity => [identity.title, identity.email, identity.id].join(" ")).join(" ") : [key.email, key.contactName, key.id, ...(key.meta.uses || []).map(usage => usage.label)].join(" ");
        return controller.matchesSearch(query, key.title + " " + searchable);
    }).sort((a, b) => a.title.localeCompare(b.title, undefined, {
            sensitivity: "base",
            numeric: true
        }))
    readonly property var selectedRow: visibleEntries.find(key => key.id === selectedId) || null
    readonly property var selected: selectedRow?.isPerson ? (selectedRow.identities.find(key => key.id === selectedIdentityId) || selectedRow.identities[0] || null) : selectedRow
    onVisibleEntriesChanged: {
        if (!visibleEntries.some(key => key.id === selectedId))
            selectedId = visibleEntries.length ? visibleEntries[0].id : "";
    }
    function count(id) {
        return id === "contacts" ? personEntries.length : entries.filter(key => id === "all" || key.own).length;
    }
    function selectScope(id) {
        scope = id;
    }
    function selectAfterRefresh(fingerprint) {
        const normalized = (fingerprint || "").toUpperCase();
        if (!normalized)
            return;
        query = "";
        const match = entries.find(key => key.id.toUpperCase() === normalized);
        if (match) {
            scope = match.own ? "own" : "contacts";
            selectedIdentityId = match.id;
            selectedId = !match.own && catalog.people?.[match.meta.person] ? "person:" + match.meta.person : match.id;
            contactsOverviewVisible = false;
        }
    }
}
