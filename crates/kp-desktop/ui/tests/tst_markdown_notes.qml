pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "MarkdownNotes"
    width: 900
    height: 800
    visible: true
    when: windowShown

    Component {
        id: noteComponent
        NoteFields {}
    }

    function createNoteDetail(fields, entryId) {
        const detail = createTemporaryObject(EntryTypes.find("note").detail, testCase, {
            width: 760,
            fields: fields,
            entryId: entryId,
            visible: true
        });
        verify(detail !== null);
        return detail;
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createNote() {
        const note = createTemporaryObject(noteComponent, testCase, { width: 760 });
        verify(note !== null);
        return note;
    }

    function child(note, name) {
        const object = findChild(note, name);
        verify(object !== null, "Missing QML object " + name);
        return object;
    }

    function click(note, name) {
        const button = child(note, name);
        tryVerify(() => button.enabled, 1000, name + " should be enabled");
        mouseClick(button);
        return button;
    }

    function test_note_load_and_clear_roundtrip_discards_undo_history() {
        const note = createNote();
        const source = "# Recuperación\nMañana 🌙";
        const notesSource = "**Contexto privado**";
        note.loadFields({ value: source, notes: notesSource });

        const valueEditor = child(note, "markdownEditor");
        const notesEditor = child(note, "markdownNotesEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");
        const notesSourceEditor = child(notesEditor, "markdownNotesSourceEditor");
        const undoButton = child(valueEditor, "markdownUndoButton");
        const document = child(valueEditor, "markdownPreviewDocument");
        const notesDocument = child(notesEditor, "markdownNotesPreviewDocument");
        compare(sourceEditor.text, source);
        compare(notesSourceEditor.text, notesSource);
        compare(note.currentFields(), { value: source, notes: notesSource });
        verify(!undoButton.enabled, "Loading a source must start a fresh undo history");

        sourceEditor.forceActiveFocus();
        sourceEditor.cursorPosition = sourceEditor.text.length;
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", source + "x");
        verify(undoButton.enabled, "A user edit should be undoable");

        sourceEditor.select(0, sourceEditor.text.length);
        keyClick(Qt.Key_Backspace);
        tryCompare(sourceEditor, "text", "");
        verify(sourceEditor.canUndo,
            "Deleting a draft must leave an undo record until the session is cleared");

        const previousClearCount = document.clearCount;
        const previousNotesClearCount = notesDocument.clearCount;
        note.clearFields();
        compare(sourceEditor.text, "");
        compare(notesSourceEditor.text, "");
        compare(note.currentFields(), { value: "", notes: "" });
        verify(document.clearCount > previousClearCount,
            "Clearing a note must clear its disposable Markdown document");
        verify(notesDocument.clearCount > previousNotesClearCount,
            "Clearing a note must clear its notes Markdown document too");
        verify(!undoButton.enabled, "Clearing a note must discard undo history");
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo,
            "Clear must remove both undo and redo records, including restoration of deleted text");
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(sourceEditor.text, "", "Undo must not restore cleared note text");

        sourceEditor.forceActiveFocus();
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", "x");
        sourceEditor.select(0, 1);
        keyClick(Qt.Key_Backspace);
        tryCompare(sourceEditor, "text", "");
        verify(sourceEditor.canUndo,
            "The identical-empty-source case must have a pending undo before reload");

        note.loadFields({ value: "", notes: "" });
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo,
            "Reloading an identical empty source must start a fresh native undo history");
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(sourceEditor.text, "");
        keyClick(Qt.Key_Y, Qt.ControlModifier);
        compare(sourceEditor.text, "");
    }

    function test_bold_and_italic_format_unicode_selections_and_undo() {
        const note = createNote();
        const source = "Mañana, 🌙 café";
        note.loadFields({ value: source, notes: "" });
        const valueEditor = child(note, "markdownEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");

        sourceEditor.select(0, "Mañana".length);
        tryCompare(sourceEditor, "selectedText", "Mañana");
        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, "**Mañana**, 🌙 café");
        compare(sourceEditor.selectedText, "Mañana",
            "Formatting keeps the selected text selected without its Markdown markers");
        compare(note.currentFields().value, sourceEditor.text);

        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, source,
            "Applying bold again to the selected content removes its existing markers");

        click(valueEditor, "markdownUndoButton");
        compare(sourceEditor.text, "**Mañana**, 🌙 café",
            "One undo reverses the toggle while preserving the preceding formatting action");
        click(valueEditor, "markdownUndoButton");
        compare(sourceEditor.text, source);
        compare(note.currentFields().value, source);

        const start = sourceEditor.text.indexOf("café");
        sourceEditor.select(start, start + "café".length);
        tryCompare(sourceEditor, "selectedText", "café");
        click(valueEditor, "markdownItalicButton");
        compare(sourceEditor.text, "Mañana, 🌙 *café*");
        click(valueEditor, "markdownUndoButton");
        compare(sourceEditor.text, source);
    }

    function test_inline_format_at_unicode_cursor_inserts_markers_without_selection() {
        const note = createNote();
        note.loadFields({ value: "🌙 café", notes: "" });
        const valueEditor = child(note, "markdownEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");
        sourceEditor.cursorPosition = 2;
        compare(sourceEditor.selectedText, "");

        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, "🌙**** café");
        compare(sourceEditor.cursorPosition, 4,
            "The insertion cursor belongs between the bold markers after the surrogate pair");
        compare(sourceEditor.selectedText, "");
        compare(note.currentFields().value, sourceEditor.text);
    }

    function test_bullet_list_formats_each_selected_line_and_undo_restores_source() {
        const note = createNote();
        const source = "uno\ndos\n三";
        note.loadFields({ value: source, notes: "" });
        const valueEditor = child(note, "markdownEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");
        sourceEditor.select(0, source.length);
        compare(sourceEditor.selectionStart, 0);
        compare(sourceEditor.selectionEnd, source.length);

        click(valueEditor, "markdownBulletListButton");
        compare(sourceEditor.text, "- uno\n- dos\n- 三");
        compare(note.currentFields().value, sourceEditor.text);
        click(valueEditor, "markdownUndoButton");
        compare(sourceEditor.text, source);
        compare(note.currentFields().value, source);
    }

    function test_list_selection_ending_at_newline_does_not_format_next_line() {
        const note = createNote();
        const source = "one\ntwo\nthree";
        note.loadFields({ value: source, notes: "" });
        const editor = child(note, "markdownEditor");
        const sourceEditor = child(editor, "markdownSourceEditor");
        sourceEditor.select(0, "one\n".length);

        click(editor, "markdownBulletListButton");
        compare(sourceEditor.text, "- one\ntwo\nthree");
        click(editor, "markdownUndoButton");
        compare(sourceEditor.text, source);
    }

    function test_code_block_from_mid_line_wraps_complete_line_and_undoes_once() {
        const note = createNote();
        const source = "before\nselected words\nafter";
        note.loadFields({ value: source, notes: "" });
        const editor = child(note, "markdownEditor");
        const sourceEditor = child(editor, "markdownSourceEditor");
        const start = source.indexOf("selected") + 2;
        sourceEditor.cursorPosition = start;

        click(editor, "markdownCodeBlockButton");
        compare(sourceEditor.text, "before\n```\nselected words\n```\nafter");
        click(editor, "markdownUndoButton");
        compare(sourceEditor.text, source,
            "The code block insertion is one undoable operation");
    }

    function test_markdown_shortcuts_follow_the_focused_field() {
        const note = createNote();
        note.loadFields({ value: "main text", notes: "notes text" });
        const valueEditor = child(note, "markdownEditor");
        const notesEditor = child(note, "markdownNotesEditor");
        const valueSource = child(valueEditor, "markdownSourceEditor");
        const notesSource = child(notesEditor, "markdownNotesSourceEditor");

        valueSource.select(0, 4);
        valueSource.forceActiveFocus();
        tryVerify(() => valueSource.activeFocus);
        keyClick(Qt.Key_B, Qt.ControlModifier);
        tryCompare(valueSource, "text", "**main** text");
        compare(notesSource.text, "notes text",
            "A shortcut in the content field must not format the notes field");

        notesSource.select(0, 5);
        notesSource.forceActiveFocus();
        tryVerify(() => notesSource.activeFocus);
        keyClick(Qt.Key_B, Qt.ControlModifier);
        tryCompare(notesSource, "text", "**notes** text");
        compare(valueSource.text, "**main** text",
            "The same shortcut in notes must leave the content field unchanged");
        compare(note.currentFields(), { value: "**main** text", notes: "**notes** text" });
    }

    function test_toolbar_buttons_are_tab_reachable_and_keyboard_activatable() {
        const note = createNote();
        note.loadFields({ value: "bold text", notes: "" });
        const editor = child(note, "markdownEditor");
        const sourceEditor = child(editor, "markdownSourceEditor");
        const modeButton = child(editor, "markdownModeButton");
        const boldButton = child(editor, "markdownBoldButton");
        sourceEditor.select(0, 4);

        verify(modeButton.activeFocusOnTab);
        verify(boldButton.activeFocusOnTab);
        modeButton.forceActiveFocus();
        keyClick(Qt.Key_Tab);
        tryVerify(() => boldButton.activeFocus, 1000,
            "Tab should move from mode controls through the toolbar to an enabled format action");
        compare(sourceEditor.selectedText, "bold");
        keyClick(Qt.Key_Space);
        tryCompare(sourceEditor, "text", "**bold** text");
    }

    function test_preview_debounce_loads_source_and_identical_reload_clears_projection() {
        const note = createNote();
        const source = "||same secret source||";
        note.loadFields({ value: source, notes: "" });
        const editor = child(note, "markdownEditor");
        const document = child(editor, "markdownPreviewDocument");

        click(editor, "markdownModeButton");
        tryCompare(editor, "previewMode", true);
        tryCompare(document, "loadCount", 1, 3000);
        compare(document.lastLoadedSource, source);
        compare(document.html, "",
            "The QML test double observes loading but never stands in for Rust rendering");

        const loadsBeforeReset = document.loadCount;
        const clearsBeforeReset = document.clearCount;
        note.loadFields({ value: source, notes: "updated context" });
        tryCompare(editor, "previewMode", false);
        verify(document.clearCount > clearsBeforeReset,
            "Loading even identical source must clear the previous rendered projection");
        wait(500);
        compare(document.loadCount, loadsBeforeReset,
            "A canceled preview timer must not repopulate the cleared projection");

        click(editor, "markdownModeButton");
        tryCompare(document, "loadCount", loadsBeforeReset + 1, 3000);
        compare(document.lastLoadedSource, source);
    }

    function test_clearing_before_preview_debounce_prevents_document_repopulation() {
        const note = createNote();
        note.loadFields({ value: "pending preview source", notes: "" });
        const editor = child(note, "markdownEditor");
        const document = child(editor, "markdownPreviewDocument");

        click(editor, "markdownModeButton");
        tryCompare(editor, "previewMode", true);
        const clearCount = document.clearCount;
        note.clearFields();
        verify(document.clearCount > clearCount,
            "Clearing a note must invalidate its pending preview document");
        wait(500);

        compare(document.loadCount, 0,
            "A preview queued before clearing must not reload after the debounce interval");
        compare(document.html, "");
        compare(note.currentFields(), { value: "", notes: "" });
        compare(editor.sourceText, "");
        verify(!editor.previewMode);
    }

    function test_note_detail_reloads_fields_and_entry_id_and_clears_when_hidden() {
        const firstValue = "# First entry\n||first private value||";
        const firstNotes = "*first private context*";
        const detail = createNoteDetail({ value: firstValue, notes: firstNotes }, "entry-one");
        const valueDocument = child(detail, "markdownDetailDocument");
        const notesDocument = child(detail, "markdownNotesDetailDocument");

        tryVerify(() => valueDocument.loadCount > 0, 3000);
        tryVerify(() => notesDocument.loadCount > 0, 3000);
        compare(valueDocument.lastLoadedSource, firstValue);
        compare(notesDocument.lastLoadedSource, firstNotes);
        compare(detail.entryId, "entry-one");

        const firstValueLoads = valueDocument.loadCount;
        const firstNotesLoads = notesDocument.loadCount;
        const valueClears = valueDocument.clearCount;
        const notesClears = notesDocument.clearCount;
        detail.entryId = "entry-two";
        tryVerify(() => valueDocument.loadCount > firstValueLoads, 3000);
        tryVerify(() => notesDocument.loadCount > firstNotesLoads, 3000);
        verify(valueDocument.clearCount > valueClears);
        verify(notesDocument.clearCount > notesClears);
        compare(valueDocument.lastLoadedSource, firstValue);
        compare(notesDocument.lastLoadedSource, firstNotes);

        const secondValue = "## Changed fields\n[second](https://example.invalid)";
        const secondNotes = "Second **context**";
        const valueLoadsBeforeFields = valueDocument.loadCount;
        const notesLoadsBeforeFields = notesDocument.loadCount;
        const valueClearsBeforeFields = valueDocument.clearCount;
        const notesClearsBeforeFields = notesDocument.clearCount;
        detail.fields = { value: secondValue, notes: secondNotes };
        tryVerify(() => valueDocument.loadCount > valueLoadsBeforeFields, 3000);
        tryVerify(() => notesDocument.loadCount > notesLoadsBeforeFields, 3000);
        verify(valueDocument.clearCount > valueClearsBeforeFields);
        verify(notesDocument.clearCount > notesClearsBeforeFields);
        compare(valueDocument.lastLoadedSource, secondValue);
        compare(notesDocument.lastLoadedSource, secondNotes);

        const valueLoadsBeforeHide = valueDocument.loadCount;
        const notesLoadsBeforeHide = notesDocument.loadCount;
        const valueClearsBeforeHide = valueDocument.clearCount;
        const notesClearsBeforeHide = notesDocument.clearCount;
        detail.visible = false;
        verify(valueDocument.clearCount > valueClearsBeforeHide);
        verify(notesDocument.clearCount > notesClearsBeforeHide);
        compare(valueDocument.html, "");
        compare(notesDocument.html, "");
        wait(500);
        compare(valueDocument.loadCount, valueLoadsBeforeHide,
            "Hidden details must not repopulate their Markdown documents");
        compare(notesDocument.loadCount, notesLoadsBeforeHide,
            "Hidden note context must not repopulate its Markdown document");

        detail.visible = true;
        tryVerify(() => valueDocument.loadCount > valueLoadsBeforeHide, 3000);
        tryVerify(() => notesDocument.loadCount > notesLoadsBeforeHide, 3000);
        compare(valueDocument.lastLoadedSource, secondValue);
        compare(notesDocument.lastLoadedSource, secondNotes);
    }
}
