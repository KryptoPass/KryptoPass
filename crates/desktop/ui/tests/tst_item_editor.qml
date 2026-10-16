pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "ItemEditor"
    width: 1000
    height: 900
    visible: true
    when: windowShown

    Component {
        id: hostComponent
        SecretFieldsHost {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    Component {
        id: baselineComponent
        DraftBaseline {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function fields(kind, session) {
        if (kind === "note")
            return { value: session + " private note", notes: session + " note context" };
        return {
            username: session + "@example.invalid",
            password: session + " private password",
            url: "https://example.invalid/" + session,
            urls: ["https://example.invalid/" + session, "https://alternate.invalid/" + session],
            totpInput: "JBSWY3DPEHPK3PXP",
            notes: session + " login context"
        };
    }

    function emptyFields(kind) {
        if (kind === "note")
            return { value: "", notes: "" };
        return { username: "", password: "", url: "", urls: [], totpInput: "", notes: "" };
    }

    function createHost() {
        const host = createTemporaryObject(hostComponent, testCase, { width: 640 });
        verify(host !== null);
        awaitKind(host, "login");
        return host;
    }

    function awaitKind(host, kind) {
        tryCompare(host, "kind", kind);
        tryVerify(() => host.ready && host.form instanceof SecretFields);
    }

    function select(host, kind) {
        host.selectType(kind);
        awaitKind(host, kind);
    }

    function seedBothDrafts(host) {
        host.loadContent({ kind: "login", fields: fields("login", "previous") });
        awaitKind(host, "login");
        select(host, "note");
        host.form.loadFields(fields("note", "previous"));
    }

    function spyFor(host, signalName) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: host,
            signalName: signalName
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    function test_type_switch_restores_each_unfinished_draft() {
        const host = createHost();
        const login = fields("login", "draft");
        const note = fields("note", "draft");
        host.loadContent({ kind: "login", fields: login });
        compare(host.currentFields(), login);

        select(host, "note");
        compare(host.currentFields(), emptyFields("note"));
        host.form.loadFields(note);
        select(host, "login");
        compare(host.currentFields(), login);
        select(host, "note");
        compare(host.currentFields(), note);
    }

    function test_clear_removes_active_and_cached_drafts_data() {
        return [{ tag: "login", activeKind: "login" }, { tag: "note", activeKind: "note" }];
    }

    function test_clear_removes_active_and_cached_drafts(row) {
        const host = createHost();
        seedBothDrafts(host);
        select(host, row.activeKind);

        host.clearFields();
        compare(host.currentFields(), emptyFields(row.activeKind));
        const otherKind = row.activeKind === "login" ? "note" : "login";
        select(host, otherKind);
        compare(host.currentFields(), emptyFields(otherKind));
        select(host, row.activeKind);
        compare(host.currentFields(), emptyFields(row.activeKind));
    }

    function test_new_session_cannot_restore_previous_drafts_data() {
        return [
            { tag: "login-to-login", activeKind: "login", nextKind: "login" },
            { tag: "login-to-note", activeKind: "login", nextKind: "note" },
            { tag: "note-to-login", activeKind: "note", nextKind: "login" },
            { tag: "note-to-note", activeKind: "note", nextKind: "note" }
        ];
    }

    function test_new_session_cannot_restore_previous_drafts(row) {
        const host = createHost();
        seedBothDrafts(host);
        select(host, row.activeKind);
        const next = fields(row.nextKind, "next-session");

        host.loadContent({ kind: row.nextKind, fields: next });
        awaitKind(host, row.nextKind);
        compare(host.currentFields(), next);
        const otherKind = row.nextKind === "login" ? "note" : "login";
        select(host, otherKind);
        compare(host.currentFields(), emptyFields(otherKind));
        select(host, row.nextKind);
        compare(host.currentFields(), next);
    }

    function test_generated_password_updates_only_the_active_login() {
        const host = createHost();
        const login = fields("login", "generated");
        const note = fields("note", "generated");
        host.loadContent({ kind: "login", fields: login });
        host.applyGeneratedPassword("first generated password");
        login.password = "first generated password";
        compare(host.currentFields(), login);

        select(host, "note");
        host.form.loadFields(note);
        const edited = spyFor(host, "edited");
        host.applyGeneratedPassword("ignored while editing a note");
        compare(host.currentFields(), note);
        compare(edited.count, 0);
        select(host, "login");
        compare(host.currentFields(), login);
    }

    function test_edited_distinguishes_user_changes_from_load_and_clear() {
        const host = createHost();
        const edited = spyFor(host, "edited");
        host.loadContent({ kind: "login", fields: fields("login", "loaded") });
        awaitKind(host, "login");
        compare(edited.count, 0);

        host.form.password = "edited password";
        compare(edited.count, 1);
        edited.clear();
        host.selectType("login");
        compare(edited.count, 0);

        select(host, "note");
        compare(edited.count, 1);
        edited.clear();
        host.form.value = "Edited note";
        compare(edited.count, 1);

        edited.clear();
        host.loadContent({ kind: "login", fields: fields("login", "another-session") });
        awaitKind(host, "login");
        compare(edited.count, 0);
        host.clearFields();
        compare(edited.count, 0);
        compare(host.currentFields(), emptyFields("login"));
    }

    function test_password_generation_request_is_forwarded() {
        const host = createHost();
        const generated = spyFor(host, "generatePasswordRequested");
        host.form.generatePasswordRequested();
        compare(generated.count, 1);
    }

    function test_unknown_kind_has_no_editable_form() {
        const host = createHost();
        host.loadContent({ kind: "unsupported-test-type", fields: { password: "must not load" } });
        tryCompare(host, "form", null);
        compare(host.ready, false);
        compare(host.currentFields(), {});

        select(host, "login");
        compare(host.currentFields(), emptyFields("login"));
    }

    function test_baseline_detects_nested_changes_and_reversion() {
        const baseline = createTemporaryObject(baselineComponent, testCase);
        verify(baseline !== null);
        const draft = {
            title: "Original title",
            content: { kind: "login", fields: fields("login", "baseline") },
            recipients: ["first", "second"],
            icon: null
        };
        baseline.capture(draft);
        verify(baseline.matches(draft));
        const password = draft.content.fields.password;
        draft.content.fields.password = "modified secret";
        verify(!baseline.matches(draft));
        draft.content.fields.password = password;
        verify(baseline.matches(draft));

        draft.content.fields.urls.push("https://new.invalid");
        verify(!baseline.matches(draft));
        draft.content.fields.urls.pop();
        verify(baseline.matches(draft));
        draft.recipients.reverse();
        verify(!baseline.matches(draft));
        draft.recipients.reverse();
        verify(baseline.matches(draft));
        draft.icon = {};
        verify(!baseline.matches(draft));
        draft.icon = null;
        verify(baseline.matches(draft));
    }

    function test_baseline_owns_nested_snapshot_without_aliasing_draft() {
        const baseline = createTemporaryObject(baselineComponent, testCase);
        verify(baseline !== null);
        const draft = {
            content: { fields: { password: "original", urls: ["https://original.invalid"] } },
            recipients: ["original recipient"]
        };
        baseline.capture(draft);
        draft.content.fields.password = "replacement";
        draft.content.fields.urls[0] = "https://replacement.invalid";
        draft.recipients.push("additional recipient");
        compare(baseline.value.content.fields.password, "original");
        compare(baseline.value.content.fields.urls, ["https://original.invalid"]);
        compare(baseline.value.recipients, ["original recipient"]);
        verify(!baseline.matches(draft));
    }

    function test_baseline_clear_drops_previous_session_snapshot() {
        const baseline = createTemporaryObject(baselineComponent, testCase);
        verify(baseline !== null);
        const previous = { content: { fields: { password: "previous secret" } } };
        baseline.capture(previous);
        verify(baseline.matches(previous));
        baseline.clear();
        compare(baseline.value, null);
        verify(!baseline.matches(previous));
        verify(!baseline.matches(null));
        const next = { content: { fields: { password: "next secret" } } };
        baseline.capture(next);
        verify(baseline.matches(next));
        verify(!baseline.matches(previous));
        baseline.clear();
        compare(baseline.value, null);
    }
}
