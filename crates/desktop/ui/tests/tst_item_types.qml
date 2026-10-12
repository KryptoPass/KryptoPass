pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "ItemTypes"
    width: 1200
    height: 900
    visible: true
    when: windowShown

    Component {
        id: detailComponent
        SecretDetail {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function typeRows() {
        return EntryTypes.options.map(option => ({ tag: option.id, option: option }));
    }

    function editorFields(kind) {
        switch (kind) {
        case "login":
            return {
                username: "user@example.org",
                password: "example password",
                url: "https://example.org/login",
                urls: ["https://example.org/login", "https://example.org/account"],
                totpInput: "JBSWY3DPEHPK3PXP",
                notes: "Recovery information"
            };
        case "note":
            return { value: "Private note content", notes: "Additional context" };
        case "custom":
            return {
                records: [{
                    id: "00000000-0000-4000-8000-000000000001",
                    key: "Recovery code",
                    value: "private recovery value",
                    multiline: true,
                    concealed: true
                }],
                notes: "Custom content notes"
            };
        case "card":
            return {
                holder: "Ada Example",
                number: "4111111111111111",
                expiryMonth: 12,
                expiryYear: 2032,
                cvv: "123",
                pin: "2468",
                notes: "Card notes"
            };
        case "totp":
            return {
                totpInput: "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&issuer=Example&algorithm=SHA256&digits=8&period=60",
                notes: "TOTP notes"
            };
        case "identity":
            return {
                firstName: "Ada",
                lastName: "Example",
                documents: [{
                    id: "00000000-0000-4000-8000-000000000002",
                    documentType: "Passport",
                    number: "P123456",
                    country: "GB",
                    expires: "2032-12-31"
                }],
                emails: ["ada@example.invalid"],
                phones: ["+1 555 0100"],
                address: "1 Example Street",
                notes: "Identity notes"
            };
        case "wifi":
            return {
                ssid: "Example network",
                security: "wpa3",
                password: "example-network-password",
                hidden: true,
                notes: "Wi-Fi notes"
            };
        case "license":
            return {
                product: "Example Product",
                licenseKey: "AAAA-BBBB-CCCC-DDDD",
                holder: "Ada Example",
                email: "ada@example.invalid",
                version: "2.1",
                purchased: "2025-01-02",
                expires: "2030-01-02",
                notes: "License notes"
            };
        case "ssh-key":
            return {
                importToken: "opaque-import-token",
                passphrase: "",
                publicKey: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEexample synthetic",
                fingerprint: "SHA256:synthetic-fingerprint",
                algorithm: "ssh-ed25519",
                user: "ada",
                hosts: ["example.invalid"],
                notes: "SSH notes"
            };
        default:
            fail("Add an editor contract fixture for " + kind);
        }
    }

    function emptyFields(kind) {
        switch (kind) {
        case "login":
            return { username: "", password: "", url: "", urls: [], totpInput: "", notes: "" };
        case "note":
            return { value: "", notes: "" };
        case "custom":
            return { records: [], notes: "" };
        case "card":
            return {
                holder: "", number: "", expiryMonth: 0, expiryYear: 0,
                cvv: "", pin: "", notes: ""
            };
        case "totp":
            return { totpInput: "", notes: "" };
        case "identity":
            return {
                firstName: "", lastName: "", documents: [], emails: [], phones: [],
                address: "", notes: ""
            };
        case "wifi":
            return { ssid: "", security: "wpa", password: "", hidden: false, notes: "" };
        case "license":
            return {
                product: "", licenseKey: "", holder: "", email: "", version: "",
                purchased: "", expires: "", notes: ""
            };
        case "ssh-key":
            return { importToken: "", passphrase: "", user: "", hosts: [], notes: "" };
        default:
            fail("Add an empty editor fixture for " + kind);
        }
    }

    function detailFields(kind) {
        const fields = editorFields(kind);
        if (kind === "login") {
            delete fields.totpInput;
            fields.totp = {
                currentCode: "123456",
                nextCode: "654321",
                period: 30,
                remainingSeconds: 15
            };
            fields.history = {
                events: [{ at: 1700000000, kind: "create" }],
                versions: [{ index: 0, at: 1700000000, passwordOnly: false }]
            };
        } else if (kind !== "note") {
            const structuredFields = {
                rows: [{
                    id: "field:test",
                    label: "Example field",
                    value: "Example value",
                    multiline: false,
                    concealed: false,
                    revealed: false,
                    hasValue: true
                }],
                notes: fields.notes,
                hasQr: false
            };
            if (kind === "totp") {
                structuredFields.totp = {
                    currentCode: "123456",
                    nextCode: "654321",
                    period: 30,
                    remainingSeconds: 15,
                    issuer: "Example",
                    account: "ada",
                    algorithm: "sha1",
                    digits: 6
                };
            }
            return structuredFields;
        }
        return fields;
    }

    function entryDetail(kind) {
        return {
            id: kind + "-entry",
            content: { kind: kind, fields: detailFields(kind) }
        };
    }

    function createDetail(kind) {
        const panel = createTemporaryObject(detailComponent, testCase, {
            width: 1000,
            detail: entryDetail(kind)
        });
        verify(panel !== null);
        tryVerify(() => panel.contentView !== null);
        return panel;
    }

    function assertSignalForwarded(panel, signalName, args) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: panel,
            signalName: signalName
        });
        verify(spy !== null && spy.valid, signalName);

        panel.contentView[signalName](...args);

        compare(spy.count, 1, signalName);
        compare(Array.from(spy.signalArguments[0]), args, signalName + " arguments");
    }

    function test_catalog_has_unique_supported_types() {
        verify(EntryTypes.options.length > 0);
        const seen = [];
        for (const option of EntryTypes.options) {
            verify(typeof option.id === "string" && option.id.length > 0);
            verify(seen.indexOf(option.id) === -1, "Duplicate type " + option.id);
            seen.push(option.id);
            verify(typeof option.name === "string" && option.name.length > 0);
            verify(typeof option.icon === "string" && option.icon.length > 0);
            compare(EntryTypes.find(option.id), option);
        }
        compare(EntryTypes.find("unsupported-test-type"), null);
    }

    function test_catalog_components_follow_common_contract_data() {
        return typeRows();
    }

    function test_catalog_components_follow_common_contract(row) {
        const editor = createTemporaryObject(row.option.editor, testCase, { width: 640 });
        verify(editor !== null, row.option.id + " editor");
        verify(editor instanceof SecretFields, row.option.id + " editor contract");
        compare(editor.currentFields(), emptyFields(row.option.id));

        const fields = editorFields(row.option.id);
        editor.loadFields(fields);
        const currentFields = Object.assign({}, fields);
        if (row.option.id === "ssh-key") {
            delete currentFields.publicKey;
            delete currentFields.fingerprint;
            delete currentFields.algorithm;
        }
        compare(editor.currentFields(), currentFields);
        editor.clearFields();
        compare(editor.currentFields(), emptyFields(row.option.id));
        editor.loadFields(fields);
        compare(editor.currentFields(), currentFields);

        const view = createTemporaryObject(row.option.detail, testCase, {
            width: 1000,
            fields: detailFields(row.option.id),
            entryId: row.option.id + "-entry"
        });
        verify(view !== null, row.option.id + " detail");
        verify(view instanceof SecretContentView, row.option.id + " detail contract");
        compare(view.fields, detailFields(row.option.id));
        compare(view.entryId, row.option.id + "-entry");
        compare(view.revealed, false);
        compare(view.versionDetail, null);
        tryVerify(() => view.implicitHeight > 0);
    }

    function test_selected_detail_propagates_common_properties_data() {
        return typeRows();
    }

    function test_selected_detail_propagates_common_properties(row) {
        const panel = createDetail(row.option.id);
        verify(panel.contentView instanceof SecretContentView);
        compare(panel.contentView.fields, panel.detail.content.fields);
        compare(panel.contentView.entryId, panel.detail.id);
        compare(panel.contentView.revealed, false);

        panel.revealed = true;
        tryCompare(panel.contentView, "revealed", true);

        const updated = entryDetail(row.option.id);
        updated.id += "-updated";
        updated.content.fields.notes = "Updated context";
        panel.detail = updated;
        tryCompare(panel.contentView, "entryId", updated.id);
        compare(panel.contentView.fields, updated.content.fields);

        for (const width of [420, 1200]) {
            panel.width = width;
            verify(waitForPolish(panel));
            verify(panel.contentView.width > 0);
            verify(panel.contentView.width <= panel.width);
            tryVerify(() => panel.contentView.implicitHeight > 0);
        }
    }

    function test_selected_detail_forwards_common_signals_data() {
        return typeRows();
    }

    function test_selected_detail_forwards_common_signals(row) {
        const panel = createDetail(row.option.id);
        assertSignalForwarded(panel, "revealToggled", []);
        assertSignalForwarded(panel, "copyUsernameRequested", []);
        assertSignalForwarded(panel, "copyPasswordRequested", []);
        assertSignalForwarded(panel, "copyTotpRequested", [false]);
        assertSignalForwarded(panel, "copyTotpRequested", [true]);
        assertSignalForwarded(panel, "versionOpenRequested", [3]);
        assertSignalForwarded(panel, "versionCloseRequested", []);
        assertSignalForwarded(panel, "versionCopyRequested", ["password"]);
        assertSignalForwarded(panel, "activityExportRequested", []);
    }

    function test_detail_transitions_discard_previous_type_fields() {
        const panel = createDetail("login");

        panel.detail = entryDetail("note");
        tryVerify(() => panel.contentView instanceof SecretContentView
            && panel.contentView.entryId === "note-entry");
        compare(panel.contentView.fields, detailFields("note"));
        compare(panel.contentView.fields.password, undefined);

        panel.detail = entryDetail("login");
        tryVerify(() => panel.contentView instanceof SecretContentView
            && panel.contentView.entryId === "login-entry");
        compare(panel.contentView.fields, detailFields("login"));
        compare(panel.contentView.fields.value, undefined);

        panel.detail = { id: "unsupported", content: { kind: "unsupported-test-type", fields: {} } };
        tryCompare(panel, "contentView", null);
        panel.detail = null;
        compare(panel.contentView, null);
    }

    function test_selected_detail_propagates_version_detail_data() {
        return typeRows();
    }

    function test_selected_detail_propagates_version_detail(row) {
        const panel = createDetail(row.option.id);
        const version = {
            index: 0,
            at: 1700000000,
            passwordOnly: false,
            title: "Previous title",
            username: "previous@example.org",
            hasPassword: true,
            urls: ["https://example.org"],
            notes: "Previous context",
            hasTotp: false
        };

        panel.versionDetail = version;
        tryCompare(panel.contentView, "versionDetail", version);
        panel.versionDetail = null;
        tryCompare(panel.contentView, "versionDetail", null);
    }
}
