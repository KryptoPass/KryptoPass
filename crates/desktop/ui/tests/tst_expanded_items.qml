pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop
import org.kde.kirigami as Kirigami

TestCase {
    id: testCase
    name: "ExpandedItems"
    width: 1200
    height: 900
    visible: true
    when: windowShown

    Component {
        id: spyComponent
        SignalSpy {}
    }

    Component {
        id: snapshotSurfaceComponent
        Rectangle {
            color: Kirigami.Theme.backgroundColor
        }
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createEditor(kind) {
        const option = EntryTypes.find(kind);
        verify(option !== null, "missing editor option for " + kind);
        const editor = createTemporaryObject(option.editor, testCase, { width: 720 });
        verify(editor !== null, "could not create editor for " + kind);
        return editor;
    }

    function findObject(root, name) {
        if (!root)
            return null;
        if (root.objectName === name)
            return root;
        const children = root.children || [];
        for (const child of children) {
            const match = findObject(child, name);
            if (match)
                return match;
        }
        return null;
    }

    function findSecretInput(root, label) {
        if (!root)
            return null;
        if (root.label === label && typeof root.valueEdited === "function")
            return root;
        const children = root.children || [];
        for (const child of children) {
            const match = findSecretInput(child, label);
            if (match)
                return match;
        }
        return null;
    }

    function findDetailField(root, label) {
        if (!root)
            return null;
        if (typeof root.copyRequested === "function"
                && typeof root.revealRequested === "function"
                && root.label === label) {
            return root;
        }
        const children = root.children || [];
        for (const child of children) {
            const match = findDetailField(child, label);
            if (match)
                return match;
        }
        return null;
    }

    function findButtonWithText(root, text) {
        if (!root)
            return null;
        if (root.text === text && typeof root.clicked === "function")
            return root;
        const children = root.children || [];
        for (const child of children) {
            const match = findButtonWithText(child, text);
            if (match)
                return match;
        }
        return null;
    }

    function test_custom_records_add_move_remove_and_clear() {
        const editor = createEditor("custom");
        compare(editor.currentFields(), { records: [], notes: "" });

        editor.addRecord();
        const firstId = editor.currentFields().records[0].id;
        verify(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(firstId));
        editor.updateRecord(0, "key", "first");
        editor.updateRecord(0, "value", "first value");
        editor.addRecord();
        const secondId = editor.currentFields().records[1].id;
        editor.updateRecord(1, "key", "second");

        editor.moveRecord(0, 1);
        let fields = editor.currentFields();
        compare(fields.records.map(record => record.key), ["second", "first"]);
        compare(fields.records.map(record => record.id), [secondId, firstId]);

        editor.removeRecord(0);
        fields = editor.currentFields();
        compare(fields.records.length, 1);
        compare(fields.records[0].key, "first");
        editor.clearFields();
        compare(editor.currentFields(), { records: [], notes: "" });
    }

    function test_custom_record_keeps_focus_while_multiple_characters_are_typed() {
        const editor = createEditor("custom");
        editor.addRecord();
        const keyField = findObject(editor, "customRecordKeyField");
        verify(keyField !== null, "custom record key input must be addressable");
        tryVerify(() => keyField.activeFocus);

        for (const character of "Recovery code")
            keyClick(character === " " ? Qt.Key_Space : character);
        tryVerify(() => editor.currentFields().records[0].key === "Recovery code");
        compare(keyField.text, "Recovery code");

        for (const character of " 2")
            keyClick(character === " " ? Qt.Key_Space : character);
        tryVerify(() => editor.currentFields().records[0].key === "Recovery code 2");
    }

    function test_identity_documents_contacts_and_ssh_hosts_keep_list_order() {
        const identity = createEditor("identity");
        identity.addDocument();
        identity.updateDocument(0, "documentType", "Passport");
        identity.updateDocument(0, "number", "first-document");
        const firstId = identity.currentFields().documents[0].id;
        identity.addDocument();
        identity.updateDocument(1, "documentType", "National ID");
        identity.updateDocument(1, "number", "second-document");
        const secondId = identity.currentFields().documents[1].id;
        identity.moveDocument(0, 1);
        let fields = identity.currentFields();
        compare(fields.documents.map(document => document.number), ["second-document", "first-document"]);
        compare(fields.documents.map(document => document.id), [secondId, firstId]);
        identity.removeDocument(0);
        compare(identity.currentFields().documents.map(document => document.number), ["first-document"]);

        const emails = findObject(identity, "identityEmailsEditor");
        const phones = findObject(identity, "identityPhonesEditor");
        verify(emails !== null);
        verify(phones !== null);
        emails.addValue();
        tryVerify(() => emails.values.length === 1);
        emails.editValue(0, "one@example.invalid");
        tryVerify(() => identity.currentFields().emails[0] === "one@example.invalid");
        emails.addValue();
        tryVerify(() => emails.values.length === 2);
        emails.editValue(1, "two@example.invalid");
        tryVerify(() => identity.currentFields().emails[1] === "two@example.invalid");
        emails.moveValue(0, 1);
        tryVerify(() => identity.currentFields().emails[0] === "two@example.invalid");
        compare(identity.currentFields().emails, ["two@example.invalid", "one@example.invalid"]);
        emails.removeValue(0);
        compare(identity.currentFields().emails, ["one@example.invalid"]);
        phones.addValue();
        phones.editValue(0, "+1 555 0100");
        compare(identity.currentFields().phones, ["+1 555 0100"]);
        identity.clearFields();
        compare(identity.currentFields(), {
            firstName: "", lastName: "", documents: [], emails: [], phones: [], address: "", notes: ""
        });

        const ssh = createEditor("ssh-key");
        ssh.loadFields({
            importToken: "opaque-import-token",
            passphrase: "synthetic-passphrase",
            user: "ada",
            hosts: ["first.example.invalid", "second.example.invalid"],
            notes: "SSH notes",
            publicKey: "ssh-ed25519 synthetic",
            fingerprint: "SHA256:synthetic",
            algorithm: "ssh-ed25519"
        });
        const hosts = findObject(ssh, "sshHostsEditor");
        compare(ssh.currentFields().passphrase, "");
        verify(hosts !== null);
        hosts.moveValue(0, 1);
        compare(ssh.currentFields().hosts, ["second.example.invalid", "first.example.invalid"]);
        hosts.removeValue(0);
        compare(ssh.currentFields().hosts, ["first.example.invalid"]);
        ssh.applyPreparedSshKey({
            importToken: "prepared-token",
            publicKey: "ssh-ed25519 prepared",
            fingerprint: "SHA256:prepared",
            algorithm: "ssh-ed25519"
        });
        fields = ssh.currentFields();
        compare(fields.importToken, "prepared-token");
        compare(fields.passphrase, "");
        compare(fields.privateKey, undefined);
        compare(fields.publicKey, undefined);
        compare(fields.fingerprint, undefined);
        compare(fields.algorithm, undefined);
        ssh.clearFields();
        compare(ssh.currentFields(), { importToken: "", passphrase: "", user: "", hosts: [], notes: "" });
    }

    function test_totp_prepared_input_is_editable_and_clearable() {
        const editor = createEditor("totp");
        editor.applyPreparedTotp({
            totpInput: "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&issuer=Example"
        });
        compare(editor.currentFields().totpInput,
                "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&issuer=Example");
        editor.clearFields();
        compare(editor.currentFields(), { totpInput: "", notes: "" });
    }

    function test_card_wifi_and_license_fields_load_and_clear() {
        const card = createEditor("card");
        const cardFields = {
            holder: "Ada Example",
            number: "4111 1111 1111 1111",
            expiryMonth: 12,
            expiryYear: 2032,
            cvv: "123",
            pin: "2468",
            notes: "Card note"
        };
        card.loadFields(cardFields);
        compare(card.currentFields(), cardFields);
        card.clearFields();
        compare(card.currentFields(), {
            holder: "", number: "", expiryMonth: 0, expiryYear: 0,
            cvv: "", pin: "", notes: ""
        });

        const wifi = createEditor("wifi");
        const security = findObject(wifi, "wifiSecurityField");
        verify(security !== null, "Wi-Fi security selector must be addressable");
        compare(security.currentValue, "wpa");
        const wifiFields = {
            ssid: "Lab network",
            security: "wpa3",
            password: "synthetic-wifi-password",
            hidden: true,
            notes: "Wi-Fi note"
        };
        wifi.loadFields(wifiFields);
        compare(security.currentValue, "wpa3");
        compare(wifi.currentFields(), wifiFields);
        wifi.loadFields({
            ssid: "Open lab", security: "open", password: "", hidden: false, notes: ""
        });
        compare(security.currentValue, "open");
        wifi.clearFields();
        compare(security.currentValue, "wpa");
        compare(wifi.currentFields(), {
            ssid: "", security: "wpa", password: "", hidden: false, notes: ""
        });

        const license = createEditor("license");
        const licenseFields = {
            product: "Example App",
            licenseKey: "SYNTHETIC-LICENSE-KEY",
            holder: "Ada Example",
            email: "ada@example.invalid",
            version: "1.0",
            purchased: "2030-01-01",
            expires: "2032-01-01",
            notes: "License note"
        };
        license.loadFields(licenseFields);
        compare(license.currentFields(), licenseFields);
        license.clearFields();
        compare(license.currentFields(), {
            product: "", licenseKey: "", holder: "", email: "", version: "",
            purchased: "", expires: "", notes: ""
        });
    }

    function test_card_expiry_editor_keeps_invalid_intermediate_numbers_for_rust_validation() {
        const card = createEditor("card");
        card.loadFields({ expiryMonth: 12, expiryYear: 2026 });
        const monthInput = findSecretInput(card, qsTr("Mes de vencimiento"));
        const yearInput = findSecretInput(card, qsTr("Año de vencimiento"));
        verify(monthInput !== null, "card month input should be discoverable");
        verify(yearInput !== null, "card year input should be discoverable");

        monthInput.valueEdited("13");
        compare(card.currentFields().expiryMonth, 13);
        yearInput.valueEdited("2");
        compare(card.currentFields().expiryYear, 2);
    }

    function test_expanded_forms_render_at_narrow_and_wide_widths() {
        const kinds = ["custom", "card", "totp", "identity", "wifi", "ssh-key"];
        const fields = {
            "custom": {
                records: [{
                    id: "00000000-0000-4000-8000-000000000001",
                    key: "Recovery code",
                    value: "synthetic-recovery-value",
                    multiline: false,
                    concealed: true
                }],
                notes: "Synthetic custom notes"
            },
            "card": {
                holder: "Ada Example", number: "4111 1111 1111 1111",
                expiryMonth: 12, expiryYear: 2032, cvv: "123", pin: "2468", notes: ""
            },
            "totp": { totpInput: "JBSWY3DPEHPK3PXP", notes: "" },
            "identity": {
                firstName: "Ada", lastName: "Example", documents: [],
                emails: ["ada@example.invalid"], phones: ["+1 555 0100"],
                address: "1 Example Street", notes: "Synthetic identity notes"
            },
            "wifi": {
                ssid: "Lab network", security: "wpa", password: "synthetic-wifi-password",
                hidden: false, notes: ""
            },
            "ssh-key": {
                importToken: "", passphrase: "", user: "ada",
                hosts: ["host.example.invalid"], notes: "",
                publicKey: "ssh-ed25519 synthetic", fingerprint: "SHA256:synthetic",
                algorithm: "ssh-ed25519"
            }
        };

        for (const kind of kinds) {
            const surface = createTemporaryObject(snapshotSurfaceComponent, testCase, {
                width: 360,
                height: 1
            });
            verify(surface !== null);
            const editor = createTemporaryObject(EntryTypes.find(kind).editor, surface, {
                width: 360
            });
            verify(editor !== null, "could not create snapshot form for " + kind);
            editor.visible = true;
            editor.loadFields(fields[kind]);
            surface.height = Qt.binding(function() { return editor.height; });
            for (const viewportWidth of [360, 800]) {
                editor.width = viewportWidth;
                tryVerify(() => editor.width === viewportWidth && editor.height > 0);
                surface.width = viewportWidth;
                tryVerify(() => surface.height === editor.height && surface.height > 0);
                verify(waitForRendering(surface), "form was not rendered at " + viewportWidth + " px");
                const image = grabImage(surface);
                compare(image.width, viewportWidth);
                verify(image.height > 0, "empty snapshot for " + kind + " at " + viewportWidth + " px");
                image.save("/tmp/kryptopass-" + kind + "-" + viewportWidth + ".png");
            }
        }
    }

    function test_structured_detail_forwards_real_field_and_qr_actions() {
        const fields = {
            rows: [{
                id: "number",
                label: "Card number",
                value: "",
                multiline: false,
                concealed: true,
                revealed: false,
                hasValue: true
            }],
            notes: "",
            hasQr: false
        };
        const detail = createTemporaryObject(EntryTypes.find("card").detail, testCase, {
            width: 720,
            fields: fields,
            entryId: "card-entry"
        });
        verify(detail !== null);
        const field = findDetailField(detail, "Card number");
        verify(field !== null, "structured detail must create a field row");
        compare(field.label, "Card number");

        const copySpy = createTemporaryObject(spyComponent, testCase, {
            target: detail,
            signalName: "copyFieldRequested"
        });
        const revealSpy = createTemporaryObject(spyComponent, testCase, {
            target: detail,
            signalName: "revealFieldRequested"
        });
        verify(copySpy.valid && revealSpy.valid);
        const copyButton = findButtonWithText(field, qsTr("Copiar Card number"));
        const revealButton = findButtonWithText(field, qsTr("Mostrar Card number"));
        verify(copyButton !== null, "detail row copy button should be addressable");
        verify(revealButton !== null, "detail row reveal button should be addressable");
        mouseClick(copyButton);
        mouseClick(revealButton);
        compare(copySpy.count, 1);
        compare(Array.from(copySpy.signalArguments[0]), ["number"]);
        compare(revealSpy.count, 1);
        compare(Array.from(revealSpy.signalArguments[0]), ["number"]);

        const wifi = createTemporaryObject(EntryTypes.find("wifi").detail, testCase, {
            width: 720,
            fields: { rows: [], notes: "", hasQr: true },
            entryId: "wifi-entry"
        });
        verify(wifi !== null);
        const qrButton = findObject(wifi, "qrToggle");
        verify(qrButton !== null, "Wi-Fi detail must expose its QR toggle");
        const qrSpy = createTemporaryObject(spyComponent, testCase, {
            target: wifi,
            signalName: "qrRequested"
        });
        verify(qrSpy.valid);
        mouseClick(qrButton, qrButton.width / 2, qrButton.height / 2);
        compare(qrSpy.count, 1);
        compare(qrButton.text, qsTr("Mostrar código QR"));

        wifi.fields = {
            rows: [], notes: "", hasQr: true,
            qrSource: "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyIiBoZWlnaHQ9IjIiPjxyZWN0IHdpZHRoPSIyIiBoZWlnaHQ9IjIiIGZpbGw9ImJsYWNrIi8+PC9zdmc+"
        };
        tryVerify(() => qrButton.text === qsTr("Ocultar código QR"));
        mouseClick(qrButton, qrButton.width / 2, qrButton.height / 2);
        compare(qrSpy.count, 2);
        wifi.entryId = "another-wifi-entry";
        wifi.fields = { rows: [], notes: "", hasQr: true };
        tryVerify(() => qrButton.text === qsTr("Mostrar código QR"));
    }
}
