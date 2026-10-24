pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "NotesFields"
    width: 1200
    height: 1800
    visible: true
    when: windowShown

    Component {
        id: hostComponent
        Item {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function typeRows() {
        return EntryTypes.options.map(option => ({
                    tag: option.id,
                    option: option
                }));
    }

    function child(parent, name) {
        const object = findChild(parent, name);
        verify(object !== null, "Missing QML object " + name);
        return object;
    }

    function createEditor(option) {
        const editor = createTemporaryObject(option.editor, testCase, {
            width: 1000
        });
        verify(editor !== null, option.id + " editor");
        return editor;
    }

    function sourceSpy(editor) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: editor.parent,
            signalName: "sourceTextEdited"
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    function activate(editor, name) {
        const button = child(editor, name);
        verify(button.enabled, name + " should be enabled");
        button.forceActiveFocus();
        tryVerify(() => button.activeFocus);
        keyClick(Qt.Key_Space);
    }

    function seedRevealedSpoiler(document) {
        document.spoilerActions = ["spoiler:test"];
        document.revealedActions = ["spoiler:test"];
    }

    function compareCleared(document) {
        compare(document.html, "", "The fixture observes lifecycle calls without implementing Rust rendering");
        compare(document.spoilerActions, []);
        compare(document.revealedActions, []);
    }

    function test_every_notes_editor_preserves_source_and_resets_same_source_data() {
        return typeRows();
    }

    function test_every_notes_editor_preserves_source_and_resets_same_source(row) {
        const form = createEditor(row.option);
        const editor = child(form, "markdownNotesEditor");
        verify(editor instanceof MarkdownEditor);
        verify(editor.parent instanceof NotesEditor, "Every entry type must use the shared notes component");
        const sourceEditor = child(editor, "markdownNotesSourceEditor");
        const document = child(editor, "markdownNotesPreviewDocument");
        const spy = sourceSpy(editor);
        const source = "Mañana 🌙\n\n# Recuperación\n||privado||\n<div>literal & texto</div>";

        form.loadFields({
            notes: source
        });
        compare(sourceEditor.text, source);
        compare(editor.sourceText, source);
        compare(form.currentFields().notes, source);
        compare(spy.count, 0, "Loading notes must not emit a user source edit");
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo);

        sourceEditor.forceActiveFocus();
        sourceEditor.cursorPosition = source.length;
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", source + "x");
        compare(form.currentFields().notes, source + "x");
        verify(spy.count > 0, "Typing must propagate through NotesEditor");

        sourceEditor.select(0, "Mañana".length);
        keyClick(Qt.Key_B, Qt.ControlModifier);
        const formatted = "**Mañana** 🌙\n\n# Recuperación\n||privado||\n<div>literal & texto</div>x";
        tryCompare(sourceEditor, "text", formatted);
        compare(form.currentFields().notes, formatted);
        verify(sourceEditor.canUndo, "Formatting must produce an undoable edit");

        activate(editor, "markdownNotesModeButton");
        tryCompare(editor, "previewMode", true);
        tryVerify(() => document.loadCount > 0, 3000);
        compare(document.lastLoadedSource, formatted);
        seedRevealedSpoiler(document);
        const loads = document.loadCount;
        const clears = document.clearCount;
        const edits = spy.count;

        form.loadFields({
            notes: formatted
        });
        compare(sourceEditor.text, formatted);
        compare(form.currentFields().notes, formatted);
        compare(spy.count, edits, "An identical-source load must not emit a user edit");
        verify(!editor.previewMode);
        verify(document.clearCount > clears);
        compareCleared(document);
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo, "Even identical source must start a fresh native undo history");
        sourceEditor.forceActiveFocus();
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        keyClick(Qt.Key_Y, Qt.ControlModifier);
        compare(sourceEditor.text, formatted);
        wait(180);
        compare(document.loadCount, loads, "A reset must cancel pending preview work");
    }

    function test_every_notes_editor_resets_identical_empty_source_data() {
        return typeRows();
    }

    function test_every_notes_editor_resets_identical_empty_source(row) {
        const form = createEditor(row.option);
        const editor = child(form, "markdownNotesEditor");
        const sourceEditor = child(editor, "markdownNotesSourceEditor");
        const spy = sourceSpy(editor);
        form.loadFields({
            notes: ""
        });
        compare(spy.count, 0);

        sourceEditor.forceActiveFocus();
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", "x");
        sourceEditor.select(0, 1);
        keyClick(Qt.Key_Backspace);
        tryCompare(sourceEditor, "text", "");
        verify(sourceEditor.canUndo, "Deleting a draft must first leave an undo record");
        const edits = spy.count;

        form.loadFields({
            notes: ""
        });
        compare(spy.count, edits);
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo, "An identical empty load must discard restoration of a deleted draft");
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        keyClick(Qt.Key_Y, Qt.ControlModifier);
        compare(sourceEditor.text, "");
        compare(form.currentFields().notes, "");
    }

    function test_every_notes_editor_clears_hidden_preview_and_clear_fields_data() {
        return typeRows();
    }

    function test_every_notes_editor_clears_hidden_preview_and_clear_fields(row) {
        const form = createEditor(row.option);
        const editor = child(form, "markdownNotesEditor");
        const sourceEditor = child(editor, "markdownNotesSourceEditor");
        const document = child(editor, "markdownNotesPreviewDocument");
        const source = "## Contexto\n||secreto||";
        form.loadFields({
            notes: source
        });
        activate(editor, "markdownNotesModeButton");
        tryVerify(() => document.loadCount > 0, 3000);
        seedRevealedSpoiler(document);
        const loads = document.loadCount;
        const clears = document.clearCount;

        form.visible = false;
        verify(document.clearCount > clears);
        compareCleared(document);
        compare(sourceEditor.text, source, "Hiding a draft must preserve its Markdown source");
        compare(form.currentFields().notes, source);
        wait(180);
        compare(document.loadCount, loads, "A hidden editor must not repopulate its preview");

        form.visible = true;
        tryVerify(() => document.loadCount > loads, 3000);
        compare(document.lastLoadedSource, source);
        compareCleared(document);
        activate(editor, "markdownNotesEditModeButton");
        sourceEditor.forceActiveFocus();
        sourceEditor.cursorPosition = source.length;
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", source + "x");
        verify(sourceEditor.canUndo);
        activate(editor, "markdownNotesModeButton");
        const loadsBeforeClear = document.loadCount;
        const clearsBeforeClear = document.clearCount;

        form.clearFields();
        compare(sourceEditor.text, "");
        compare(editor.sourceText, "");
        compare(form.currentFields().notes, "");
        verify(!editor.previewMode);
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo);
        verify(document.clearCount > clearsBeforeClear);
        compareCleared(document);
        sourceEditor.forceActiveFocus();
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(sourceEditor.text, "", "Undo must not restore cleared notes");
        wait(180);
        compare(document.loadCount, loadsBeforeClear, "A preview queued before clearFields must stay canceled");
    }

    function test_every_notes_detail_uses_shared_renderer_and_entry_lifecycle_data() {
        return typeRows();
    }

    function test_every_notes_detail_uses_shared_renderer_and_entry_lifecycle(row) {
        const host = createTemporaryObject(hostComponent, testCase, {
            width: 1000
        });
        verify(host !== null);
        const source = "**Contexto**\n||privado||\n<span>literal & texto</span>";
        const detail = createTemporaryObject(row.option.detail, host, {
            width: 1000,
            fields: {
                value: "# Contenido",
                notes: source,
                rows: []
            },
            entryId: "first-entry"
        });
        verify(detail !== null);
        const section = child(detail, "markdownNotesSection");
        verify(section instanceof SecretTextSection);
        const document = child(section, row.option.id === "note" ? "markdownNotesDetailDocument" : "markdownSectionDocument");
        const view = child(section, row.option.id === "note" ? "markdownNotesDetailView" : "markdownSectionView");
        verify(view instanceof MarkdownView);
        compare(view.document, document);
        tryVerify(() => document.loadCount > 0, 3000);
        compare(document.lastLoadedSource, source);
        compareCleared(document);

        seedRevealedSpoiler(document);
        const loadsBeforeRefresh = document.loadCount;
        const clearsBeforeRefresh = document.clearCount;
        detail.fields = {
            value: "# Changed unrelated content",
            notes: source,
            rows: [],
            totp: {
                currentCode: "123456",
                period: 30,
                remainingSeconds: 14
            },
            hasQr: false
        };
        wait(180);
        compare(document.loadCount, loadsBeforeRefresh, "Refreshing unrelated fields must preserve the current notes document");
        compare(document.clearCount, clearsBeforeRefresh);
        compare(document.spoilerActions, ["spoiler:test"]);
        compare(document.revealedActions, ["spoiler:test"], "A TOTP or metadata refresh must preserve revealed notes for the same entry");

        seedRevealedSpoiler(document);
        const initialLoads = document.loadCount;
        const initialClears = document.clearCount;
        detail.entryId = "second-entry";
        tryVerify(() => document.loadCount > initialLoads, 3000);
        verify(document.clearCount > initialClears);
        compare(document.lastLoadedSource, source);
        compareCleared(document);

        seedRevealedSpoiler(document);
        const loadsBeforeHide = document.loadCount;
        const clearsBeforeHide = document.clearCount;
        host.visible = false;
        verify(document.clearCount > clearsBeforeHide, "Hiding an ancestor must clear the rendered notes and revealed spoilers");
        compareCleared(document);
        detail.entryId = "third-entry";
        const changedSource = source + "\n- siguiente";
        detail.fields = {
            value: "# Otro contenido",
            notes: changedSource,
            rows: []
        };
        wait(180);
        compare(document.loadCount, loadsBeforeHide, "Neither entry nor field changes may load a hidden document");

        host.visible = true;
        tryVerify(() => document.loadCount > loadsBeforeHide, 3000);
        compare(document.lastLoadedSource, changedSource);
        compareCleared(document);
        seedRevealedSpoiler(document);
        const clearsBeforeEmpty = document.clearCount;
        detail.fields = {
            value: "# Otro contenido",
            notes: "",
            rows: []
        };
        verify(document.clearCount > clearsBeforeEmpty);
        compareCleared(document);
        verify(!section.visible);
    }

    function test_login_history_notes_reload_same_source_for_each_version_and_entry() {
        const detail = createTemporaryObject(EntryTypes.find("login").detail, testCase, {
            width: 1000,
            fields: {
                notes: "Current context"
            },
            entryId: "first-entry"
        });
        verify(detail !== null);
        const section = child(detail, "markdownVersionNotesSection");
        verify(section instanceof SecretTextSection);
        const document = child(section, "markdownSectionDocument");
        const view = child(section, "markdownSectionView");
        verify(view instanceof MarkdownView);
        compare(view.document, document);
        compare(document.loadCount, 0, "Closed history must not project version notes");
        const source = "## Histórico\n||contexto anterior||";
        const version = {
            index: 0,
            at: 1700000000,
            notes: source
        };

        detail.versionDetail = version;
        tryVerify(() => document.loadCount > 0, 3000);
        compare(document.lastLoadedSource, source);
        seedRevealedSpoiler(document);
        const firstLoads = document.loadCount;
        const firstClears = document.clearCount;
        detail.versionDetail = Object.assign({}, version, {
            index: 1
        });
        tryVerify(() => document.loadCount > firstLoads, 3000);
        verify(document.clearCount > firstClears);
        compare(document.lastLoadedSource, source);
        compareCleared(document);

        seedRevealedSpoiler(document);
        const versionLoads = document.loadCount;
        const versionClears = document.clearCount;
        detail.entryId = "second-entry";
        tryVerify(() => document.loadCount > versionLoads, 3000);
        verify(document.clearCount > versionClears);
        compareCleared(document);

        seedRevealedSpoiler(document);
        const loadsBeforeHide = document.loadCount;
        const clearsBeforeHide = document.clearCount;
        detail.visible = false;
        tryVerify(() => document.clearCount > clearsBeforeHide);
        compareCleared(document);
        wait(180);
        compare(document.loadCount, loadsBeforeHide);
        detail.visible = true;
        tryVerify(() => document.loadCount > loadsBeforeHide, 3000);
        seedRevealedSpoiler(document);

        detail.versionDetail = Object.assign({}, version, {
            notes: ""
        });
        tryCompare(section, "visible", false);
        compareCleared(document);
        const loadsWithoutNotes = document.loadCount;
        detail.versionDetail = null;
        compareCleared(document);
        wait(180);
        compare(document.loadCount, loadsWithoutNotes);
        detail.versionDetail = version;
        tryVerify(() => document.loadCount > loadsWithoutNotes, 3000);
        compare(document.lastLoadedSource, source);
        compareCleared(document);
        seedRevealedSpoiler(document);
        const clearsBeforeClose = document.clearCount;
        detail.versionDetail = null;
        tryVerify(() => document.clearCount > clearsBeforeClose);
        compareCleared(document);
    }
}
