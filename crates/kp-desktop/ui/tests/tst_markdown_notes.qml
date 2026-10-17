pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kde.kirigami as Kirigami
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

    Component {
        id: markdownViewComponent

        MarkdownView {
            document: MarkdownDocument {}
        }
    }

    Component {
        id: spyComponent
        SignalSpy {}
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
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign|QTextCursor::setPosition/);
    }

    function createNote() {
        const note = createTemporaryObject(noteComponent, testCase, {
            width: 760
        });
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

    function inlineSpoiler(index, revealed = false) {
        const action = 'href="kryptopass-spoiler:1:' + index + '"';
        if (!revealed)
            return '<a ' + action + ' style="color:#606060;text-decoration:none"><span style="white-space:pre">████████████</span></a>';
        return "Disclosed value " + (index + 1) + '<a ' + action + ' style="text-decoration:none"><span style="white-space:pre">&#x2007;&#x2007;&#x2007;</span></a>';
    }

    function previewHtml(revealed = false) {
        return '<p>Before the inline spoiler, several ordinary words make this paragraph wrap as its available width changes. ' + inlineSpoiler(0, revealed) + ' After the first spoiler, the paragraph continues with selectable text.</p><p>' + inlineSpoiler(1) + ' Read the <a href="https://example.invalid/documentation">documentation</a>.</p>';
    }

    function createPreview(html, widthUnits = 40) {
        const view = createTemporaryObject(markdownViewComponent, testCase, {
            width: Kirigami.Units.gridUnit * widthUnits
        });
        verify(view !== null);
        view.document.html = html;
        view.document.spoilerActions = ["kryptopass-spoiler:1:0", "kryptopass-spoiler:1:1"];
        tryVerify(() => findChild(view, "markdownSpoilerToggle0") !== null);
        const text = child(view, "markdownText");
        tryCompare(text, "width", view.availableWidth);
        verify(waitForPolish(text.Window.window));
        return view;
    }

    function allItems(root) {
        const result = [root];
        for (const item of root.children || [])
            result.push(...allItems(item));
        return result;
    }

    function verifyInlineToggle(view, index) {
        const text = child(view, "markdownText");
        const button = child(view, "markdownSpoilerToggle" + index);
        compare(button.parent, text, "Spoiler controls must belong to the rendered text");
        tryVerify(() => button.visible && button.width > 0 && button.height > 0);
        const action = "kryptopass-spoiler:1:" + index;
        tryVerify(() => text.linkAt(button.x + button.width / 2, button.y + button.height / 2) === action, 1000, "The control must cover its own inline action anchor after text layout");
        const plain = text.getText(0, text.length);
        const markers = /████████████|\u2007\u2007\u2007/g;
        let marker;
        let controlRange = null;
        while ((marker = markers.exec(plain)) !== null) {
            const first = text.positionToRectangle(marker.index);
            if (text.linkAt(first.x + 1, first.y + first.height / 2) !== action)
                continue;
            const last = text.positionToRectangle(marker.index + marker[0].length - 1);
            controlRange = {
                first,
                last
            };
            break;
        }
        verify(controlRange !== null, "The inline control must correspond to a rendered action range");
        verify(Math.abs(button.y - controlRange.first.y) <= 1, "The inline control must stay with the first segment of its action anchor");
        if (controlRange.last.y > controlRange.first.y + 1)
            verify(button.height <= controlRange.first.height + 1, "A wrapped mask control must not cover text on another line");
        verify(button.x >= -1 && button.x + button.width <= text.width + 1, "An inline control must stay inside the rendered text width");
        return button;
    }

    function test_mask_reveals_inline_and_only_the_revealed_spoiler_shows_an_eye() {
        const view = createPreview(previewHtml());
        const first = verifyInlineToggle(view, 0);
        const second = verifyInlineToggle(view, 1);
        compare(first.Accessible.name, qsTr("Mostrar spoiler %1").arg(1));
        compare(second.Accessible.name, qsTr("Mostrar spoiler %1").arg(2));
        compare(first.icon.name, "", "A hidden spoiler must expose its mask without an eye icon");
        compare(second.icon.name, "");
        verify(first.activeFocusOnTab && second.activeFocusOnTab);
        verify(second.y > first.y, "Each control must stay with its own paragraph");
        verify(!allItems(view).some(item => item.text === qsTr("Spoilers")), "The rendered note must not add a separate spoiler controls section");

        mouseClick(first);
        compare(view.document.activateLinkCount, 1);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:0");

        view.document.revealedActions = ["kryptopass-spoiler:1:0"];
        view.document.html = previewHtml(true);
        tryVerify(() => first.icon.name === "view-hidden");
        compare(first.Accessible.name, qsTr("Ocultar spoiler %1").arg(1));
        verifyInlineToggle(view, 0);
        compare(second.icon.name, "", "Revealing one spoiler must leave the other mask without an eye");
        first.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(first, "activeFocus", true);
        keyClick(Qt.Key_Space);
        compare(view.document.activateLinkCount, 2);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:0");

        view.document.revealedActions = [];
        view.document.html = previewHtml();
        tryVerify(() => first.icon.name === "");
        compare(first.Accessible.name, qsTr("Mostrar spoiler %1").arg(1));
        verifyInlineToggle(view, 0);
    }

    function test_hidden_mask_is_tab_reachable_and_space_activatable() {
        const view = createPreview(previewHtml());
        const first = verifyInlineToggle(view, 0);
        const second = verifyInlineToggle(view, 1);
        const text = child(view, "markdownText");
        compare(first.icon.name, "");
        text.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(text, "activeFocus", true);
        keyClick(Qt.Key_Tab);
        tryCompare(first, "activeFocus", true);
        keyClick(Qt.Key_Space);
        compare(view.document.activateLinkCount, 1);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:0");
        keyClick(Qt.Key_Tab);
        tryCompare(second, "activeFocus", true);
        keyClick(Qt.Key_Space);
        compare(view.document.activateLinkCount, 2);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:1");
    }

    function test_revealed_eye_is_visible_and_usable_at_the_end_of_a_block_data() {
        const eye = '<a href="kryptopass-spoiler:1:0" style="text-decoration:none"><span style="white-space:pre">&#x2007;&#x2007;&#x2007;</span></a>';
        const contexts = [
            {
                tag: "only-spoiler",
                html: '<p>Disclosed value' + eye + '</p>'
            },
            {
                tag: "paragraph-end",
                html: '<p>Text before the final disclosed value ' + eye + '</p>'
            },
            {
                tag: "before-next-paragraph",
                html: '<p>Disclosed value' + eye + '</p><p>Following paragraph.</p>'
            },
            {
                tag: "heading-end",
                html: '<h2>Disclosed heading' + eye + '</h2>'
            },
            {
                tag: "list-item-end",
                html: '<ul><li>Disclosed item' + eye + '</li></ul>'
            },
            {
                tag: "strong-content",
                html: '<p><strong>Disclosed value</strong>' + eye + '</p>'
            },
            {
                tag: "emphasized-content",
                html: '<p><em>Disclosed value</em>' + eye + '</p>'
            },
            {
                tag: "inside-strong",
                html: '<p><strong>Disclosed value' + eye + '</strong></p>'
            },
            {
                tag: "inside-emphasis",
                html: '<p><em>Disclosed value' + eye + '</em></p>'
            }
        ];
        const rows = [];
        for (const context of contexts) {
            for (const fontScale of [1, 0.75, 1.75])
                rows.push({
                    tag: context.tag + "-font-" + fontScale,
                    html: context.html,
                    fontScale
                });
        }
        return rows;
    }

    function test_revealed_eye_is_visible_and_usable_at_the_end_of_a_block(data) {
        const view = createPreview(data.html);
        view.document.spoilerActions = ["kryptopass-spoiler:1:0"];
        view.document.revealedActions = ["kryptopass-spoiler:1:0"];
        view.font.pointSize = Kirigami.Theme.defaultFont.pointSize * data.fontScale;
        const text = child(view, "markdownText");
        tryVerify(() => text.font.pointSize === view.font.pointSize);
        verify(waitForPolish(text.Window.window));
        const button = child(view, "markdownSpoilerToggle0");
        tryVerify(() => button.visible && button.icon.name === "view-hidden");
        verify(button.width >= button.icon.width && button.height >= button.icon.height, "The revealed eye must have room to render its full icon; actual " + button.width + "x" + button.height + ", icon " + button.icon.width + "x" + button.icon.height);
        verifyInlineToggle(view, 0);
        verify(button.y >= -1 && button.y + button.height <= text.height + 1, "A final-block eye must stay inside the rendered text height");
        const bottom = button.mapToItem(view, 0, button.height).y;
        verify(bottom <= view.height + 1, "A final-block eye must not be clipped by its Markdown view");
        mouseClick(button);
        compare(view.document.activateLinkCount, 1);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:0");
        button.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(button, "activeFocus", true);
        keyClick(Qt.Key_Space);
        compare(view.document.activateLinkCount, 2);
        compare(view.document.lastActivatedLink, "kryptopass-spoiler:1:0");
    }

    function test_revealed_eye_draws_pixels_instead_of_only_setting_an_icon_name_data() {
        return [
            {
                tag: "normal-font",
                fontScale: 1
            },
            {
                tag: "small-font",
                fontScale: 0.75
            }
        ];
    }

    function test_revealed_eye_draws_pixels_instead_of_only_setting_an_icon_name(data) {
        const view = createPreview('<p>' + inlineSpoiler(0, true) + '</p>');
        view.document.spoilerActions = ["kryptopass-spoiler:1:0"];
        view.document.revealedActions = ["kryptopass-spoiler:1:0"];
        view.font.pointSize = Kirigami.Theme.defaultFont.pointSize * data.fontScale;
        const button = child(view, "markdownSpoilerToggle0");
        tryVerify(() => button.visible && button.icon.name === "view-hidden");
        verify(button.contentItem !== null, "A transparent background must retain a separate visual item for the eye");
        const icon = button.contentItem;
        tryVerify(() => icon.visible && icon.width > 0 && icon.height > 0);
        verify(waitForPolish(button.Window.window));
        const withEye = grabImage(button);
        verify(withEye.width > 0 && withEye.height > 0);
        icon.visible = false;
        const withoutEye = grabImage(button);
        icon.visible = true;
        verify(!withEye.equals(withoutEye), "The revealed eye must contribute visible pixels to the rendered control");
    }

    function test_inline_eyes_follow_wrap_unicode_and_table_layout_data() {
        return [
            {
                tag: "paragraph",
                html: previewHtml()
            },
            {
                tag: "emoji-and-unlinked-spaces",
                html: '<p>🌙 Mañana café <span style="white-space:pre">&#x2007;&#x2007;&#x2007;</span> unlinked ████████████ ordinary text.</p>' + previewHtml()
            },
            {
                tag: "table",
                html: '<p>🌙 Table introduction.</p><table><tr><td>First cell before ' + inlineSpoiler(0) + '</td><td>Second cell before ' + inlineSpoiler(1) + '</td></tr></table><p>Text after the table.</p>'
            },
            {
                tag: "constrained-table",
                html: '<table width="220"><tr><td width="110">' + inlineSpoiler(0) + '</td><td width="110">' + inlineSpoiler(1) + '</td></tr></table>'
            },
            {
                tag: "revealed-paragraph",
                html: previewHtml(true),
                revealedActions: ["kryptopass-spoiler:1:0"]
            },
            {
                tag: "revealed-emoji-and-unlinked-spaces",
                html: '<p>🌙 Mañana café <span style="white-space:pre">&#x2007;&#x2007;&#x2007;</span> unlinked ████████████ ordinary text.</p>' + previewHtml(true),
                revealedActions: ["kryptopass-spoiler:1:0"]
            },
            {
                tag: "revealed-table",
                html: '<p>🌙 Table introduction.</p><table><tr><td>First cell before ' + inlineSpoiler(0, true) + '</td><td>Second cell before ' + inlineSpoiler(1, true) + '</td></tr></table><p>Text after the table.</p>',
                revealedActions: ["kryptopass-spoiler:1:0", "kryptopass-spoiler:1:1"]
            }
        ];
    }

    function test_inline_eyes_follow_wrap_unicode_and_table_layout(data) {
        const view = createPreview(data.html);
        view.document.revealedActions = data.revealedActions || [];
        const text = child(view, "markdownText");
        const first = verifyInlineToggle(view, 0);
        const initialY = first.y;
        for (const widthUnits of [16, 30, 20, 40]) {
            view.width = Kirigami.Units.gridUnit * widthUnits;
            tryCompare(text, "width", view.availableWidth);
            verifyInlineToggle(view, 0);
            verifyInlineToggle(view, 1);
            if (widthUnits === 16 && data.tag.endsWith("paragraph"))
                tryVerify(() => first.y > initialY, 1000, "Narrowing the paragraph must move its eye with the wrapped line");
        }
        view.font.pointSize = Kirigami.Theme.defaultFont.pointSize * 1.75;
        tryVerify(() => text.font.pointSize === view.font.pointSize);
        verifyInlineToggle(view, 0);
        verifyInlineToggle(view, 1);
    }

    function test_inline_controls_preserve_selection_and_regular_link_activation() {
        const view = createPreview(previewHtml());
        const text = child(view, "markdownText");
        text.select(0, "Before".length);
        compare(view.selectedText, "Before");
        mouseClick(verifyInlineToggle(view, 0));
        compare(view.selectedText, "Before", "Using an inline control must keep the existing selectable text");
        view.clearSelection();
        compare(view.selectedText, "");
        verify(waitForPolish(text.Window.window));

        const link = "https://example.invalid/documentation";
        const offset = text.getText(0, text.length).indexOf("documentation");
        verify(offset >= 0);
        const character = text.positionToRectangle(offset);
        compare(text.linkAt(character.x + 2, character.y + character.height / 2), link);
        mouseMove(text, character.x + 2, character.y + character.height / 2);
        tryCompare(text, "hoveredLink", link);
        mouseClick(text, character.x + 2, character.y + character.height / 2);
        compare(view.document.lastActivatedLink, link, "Ordinary text links must retain the document authorization path");
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: view,
            signalName: "linkRequested"
        });
        verify(spy !== null && spy.valid);
        view.document.linkRequested(link);
        compare(spy.count, 1);
        compare(spy.signalArguments[0][0], link);
    }

    function test_wrapped_mask_first_and_last_blocks_activate_the_same_action_data() {
        return [
            {
                tag: "default-font",
                fontScale: 1
            },
            {
                tag: "large-font",
                fontScale: 1.75
            }
        ];
    }

    function test_wrapped_mask_first_and_last_blocks_activate_the_same_action(data) {
        const view = createPreview('<table width="220"><tr><td width="110">' + inlineSpoiler(0) + '</td><td width="110">' + inlineSpoiler(1) + '</td></tr></table>');
        const text = child(view, "markdownText");
        view.font.pointSize = Kirigami.Theme.defaultFont.pointSize * data.fontScale;
        tryVerify(() => text.font.pointSize === view.font.pointSize);
        for (let index = 0; index < 2; ++index) {
            const action = "kryptopass-spoiler:1:" + index;
            const button = verifyInlineToggle(view, index);
            const plain = text.getText(0, text.length);
            const mask = plain.indexOf("████████████", index === 0 ? 0 : plain.indexOf("████████████") + 12);
            verify(mask >= 0);
            const first = text.positionToRectangle(mask);
            const last = text.positionToRectangle(mask + 11);
            verify(last.y > first.y, "This narrow cell must exercise a mask split across lines");
            const calls = view.document.activateLinkCount;
            mouseClick(button);
            compare(view.document.activateLinkCount, calls + 1);
            compare(view.document.lastActivatedLink, action);

            const lastX = last.x + 2;
            const lastY = last.y + last.height / 2;
            compare(text.linkAt(lastX, lastY), action);
            mouseMove(text, lastX, lastY);
            tryCompare(text, "hoveredLink", action);
            mouseClick(text, lastX, lastY);
            compare(view.document.activateLinkCount, calls + 2, "The final wrapped segment must retain the native text link action");
            compare(view.document.lastActivatedLink, action);

            button.forceActiveFocus(Qt.TabFocusReason);
            tryCompare(button, "activeFocus", true);
            keyClick(Qt.Key_Space);
            compare(view.document.activateLinkCount, calls + 3);
            compare(view.document.lastActivatedLink, action);
        }
    }

    function test_clearing_the_document_removes_inline_controls_and_selection() {
        const view = createPreview(previewHtml(true));
        view.document.revealedActions = ["kryptopass-spoiler:1:0"];
        verifyInlineToggle(view, 0);
        verifyInlineToggle(view, 1);
        const text = child(view, "markdownText");
        text.select(0, "Before".length);
        compare(view.selectedText, "Before");

        view.document.clear();
        tryCompare(text, "length", 0);
        compare(text.getText(0, text.length), "");
        compare(view.selectedText, "");
        tryVerify(() => findChild(view, "markdownSpoilerToggle0") === null && findChild(view, "markdownSpoilerToggle1") === null);
        compare(view.document.spoilerActions, []);
        compare(view.document.revealedActions, []);
    }

    function test_note_load_and_clear_roundtrip_discards_undo_history() {
        const note = createNote();
        const source = "# Recuperación\nMañana 🌙";
        const notesSource = "**Contexto privado**";
        note.loadFields({
            value: source,
            notes: notesSource
        });

        const valueEditor = child(note, "markdownEditor");
        const notesEditor = child(note, "markdownNotesEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");
        const notesSourceEditor = child(notesEditor, "markdownNotesSourceEditor");
        const undoButton = child(valueEditor, "markdownUndoButton");
        const document = child(valueEditor, "markdownPreviewDocument");
        const notesDocument = child(notesEditor, "markdownNotesPreviewDocument");
        compare(sourceEditor.text, source);
        compare(notesSourceEditor.text, notesSource);
        compare(note.currentFields(), {
            value: source,
            notes: notesSource
        });
        verify(!undoButton.enabled, "Loading a source must start a fresh undo history");

        sourceEditor.forceActiveFocus();
        sourceEditor.cursorPosition = sourceEditor.text.length;
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", source + "x");
        verify(undoButton.enabled, "A user edit should be undoable");

        sourceEditor.select(0, sourceEditor.text.length);
        keyClick(Qt.Key_Backspace);
        tryCompare(sourceEditor, "text", "");
        verify(sourceEditor.canUndo, "Deleting a draft must leave an undo record until the session is cleared");

        const previousClearCount = document.clearCount;
        const previousNotesClearCount = notesDocument.clearCount;
        note.clearFields();
        compare(sourceEditor.text, "");
        compare(notesSourceEditor.text, "");
        compare(note.currentFields(), {
            value: "",
            notes: ""
        });
        verify(document.clearCount > previousClearCount, "Clearing a note must clear its disposable Markdown document");
        verify(notesDocument.clearCount > previousNotesClearCount, "Clearing a note must clear its notes Markdown document too");
        verify(!undoButton.enabled, "Clearing a note must discard undo history");
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo, "Clear must remove both undo and redo records, including restoration of deleted text");
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(sourceEditor.text, "", "Undo must not restore cleared note text");

        sourceEditor.forceActiveFocus();
        keyClick(Qt.Key_X);
        tryCompare(sourceEditor, "text", "x");
        sourceEditor.select(0, 1);
        keyClick(Qt.Key_Backspace);
        tryCompare(sourceEditor, "text", "");
        verify(sourceEditor.canUndo, "The identical-empty-source case must have a pending undo before reload");

        note.loadFields({
            value: "",
            notes: ""
        });
        verify(!sourceEditor.canUndo && !sourceEditor.canRedo, "Reloading an identical empty source must start a fresh native undo history");
        keyClick(Qt.Key_Z, Qt.ControlModifier);
        compare(sourceEditor.text, "");
        keyClick(Qt.Key_Y, Qt.ControlModifier);
        compare(sourceEditor.text, "");
    }

    function test_bold_and_italic_format_unicode_selections_and_undo() {
        const note = createNote();
        const source = "Mañana, 🌙 café";
        note.loadFields({
            value: source,
            notes: ""
        });
        const valueEditor = child(note, "markdownEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");

        sourceEditor.select(0, "Mañana".length);
        tryCompare(sourceEditor, "selectedText", "Mañana");
        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, "**Mañana**, 🌙 café");
        compare(sourceEditor.selectedText, "Mañana", "Formatting keeps the selected text selected without its Markdown markers");
        compare(note.currentFields().value, sourceEditor.text);

        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, source, "Applying bold again to the selected content removes its existing markers");

        click(valueEditor, "markdownUndoButton");
        compare(sourceEditor.text, "**Mañana**, 🌙 café", "One undo reverses the toggle while preserving the preceding formatting action");
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
        note.loadFields({
            value: "🌙 café",
            notes: ""
        });
        const valueEditor = child(note, "markdownEditor");
        const sourceEditor = child(valueEditor, "markdownSourceEditor");
        sourceEditor.cursorPosition = 2;
        compare(sourceEditor.selectedText, "");

        click(valueEditor, "markdownBoldButton");
        compare(sourceEditor.text, "🌙**** café");
        compare(sourceEditor.cursorPosition, 4, "The insertion cursor belongs between the bold markers after the surrogate pair");
        compare(sourceEditor.selectedText, "");
        compare(note.currentFields().value, sourceEditor.text);
    }

    function test_bullet_list_formats_each_selected_line_and_undo_restores_source() {
        const note = createNote();
        const source = "uno\ndos\n三";
        note.loadFields({
            value: source,
            notes: ""
        });
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
        note.loadFields({
            value: source,
            notes: ""
        });
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
        note.loadFields({
            value: source,
            notes: ""
        });
        const editor = child(note, "markdownEditor");
        const sourceEditor = child(editor, "markdownSourceEditor");
        const start = source.indexOf("selected") + 2;
        sourceEditor.cursorPosition = start;

        click(editor, "markdownCodeBlockButton");
        compare(sourceEditor.text, "before\n```\nselected words\n```\nafter");
        click(editor, "markdownUndoButton");
        compare(sourceEditor.text, source, "The code block insertion is one undoable operation");
    }

    function test_markdown_shortcuts_follow_the_focused_field() {
        const note = createNote();
        note.loadFields({
            value: "main text",
            notes: "notes text"
        });
        const valueEditor = child(note, "markdownEditor");
        const notesEditor = child(note, "markdownNotesEditor");
        const valueSource = child(valueEditor, "markdownSourceEditor");
        const notesSource = child(notesEditor, "markdownNotesSourceEditor");

        valueSource.select(0, 4);
        valueSource.forceActiveFocus();
        tryVerify(() => valueSource.activeFocus);
        keyClick(Qt.Key_B, Qt.ControlModifier);
        tryCompare(valueSource, "text", "**main** text");
        compare(notesSource.text, "notes text", "A shortcut in the content field must not format the notes field");

        notesSource.select(0, 5);
        notesSource.forceActiveFocus();
        tryVerify(() => notesSource.activeFocus);
        keyClick(Qt.Key_B, Qt.ControlModifier);
        tryCompare(notesSource, "text", "**notes** text");
        compare(valueSource.text, "**main** text", "The same shortcut in notes must leave the content field unchanged");
        compare(note.currentFields(), {
            value: "**main** text",
            notes: "**notes** text"
        });
    }

    function test_toolbar_buttons_are_tab_reachable_and_keyboard_activatable() {
        const note = createNote();
        note.loadFields({
            value: "bold text",
            notes: ""
        });
        const editor = child(note, "markdownEditor");
        const sourceEditor = child(editor, "markdownSourceEditor");
        const modeButton = child(editor, "markdownModeButton");
        const boldButton = child(editor, "markdownBoldButton");
        sourceEditor.select(0, 4);

        verify(modeButton.activeFocusOnTab);
        verify(boldButton.activeFocusOnTab);
        modeButton.forceActiveFocus();
        keyClick(Qt.Key_Tab);
        tryVerify(() => boldButton.activeFocus, 1000, "Tab should move from mode controls through the toolbar to an enabled format action");
        compare(sourceEditor.selectedText, "bold");
        keyClick(Qt.Key_Space);
        tryCompare(sourceEditor, "text", "**bold** text");
    }

    function test_preview_debounce_loads_source_and_identical_reload_clears_projection() {
        const note = createNote();
        const source = "||same secret source||";
        note.loadFields({
            value: source,
            notes: ""
        });
        const editor = child(note, "markdownEditor");
        const document = child(editor, "markdownPreviewDocument");

        click(editor, "markdownModeButton");
        tryCompare(editor, "previewMode", true);
        tryCompare(document, "loadCount", 1, 3000);
        compare(document.lastLoadedSource, source);
        compare(document.html, "", "The QML test double observes loading but never stands in for Rust rendering");

        const loadsBeforeReset = document.loadCount;
        const clearsBeforeReset = document.clearCount;
        note.loadFields({
            value: source,
            notes: "updated context"
        });
        tryCompare(editor, "previewMode", false);
        verify(document.clearCount > clearsBeforeReset, "Loading even identical source must clear the previous rendered projection");
        wait(500);
        compare(document.loadCount, loadsBeforeReset, "A canceled preview timer must not repopulate the cleared projection");

        click(editor, "markdownModeButton");
        tryCompare(document, "loadCount", loadsBeforeReset + 1, 3000);
        compare(document.lastLoadedSource, source);
    }

    function test_clearing_before_preview_debounce_prevents_document_repopulation() {
        const note = createNote();
        note.loadFields({
            value: "pending preview source",
            notes: ""
        });
        const editor = child(note, "markdownEditor");
        const document = child(editor, "markdownPreviewDocument");

        click(editor, "markdownModeButton");
        tryCompare(editor, "previewMode", true);
        const clearCount = document.clearCount;
        note.clearFields();
        verify(document.clearCount > clearCount, "Clearing a note must invalidate its pending preview document");
        wait(500);

        compare(document.loadCount, 0, "A preview queued before clearing must not reload after the debounce interval");
        compare(document.html, "");
        compare(note.currentFields(), {
            value: "",
            notes: ""
        });
        compare(editor.sourceText, "");
        verify(!editor.previewMode);
    }

    function test_note_detail_reloads_fields_and_entry_id_and_clears_when_hidden() {
        const firstValue = "# First entry\n||first private value||";
        const firstNotes = "*first private context*";
        const detail = createNoteDetail({
            value: firstValue,
            notes: firstNotes
        }, "entry-one");
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
        detail.fields = {
            value: secondValue,
            notes: secondNotes
        };
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
        compare(valueDocument.loadCount, valueLoadsBeforeHide, "Hidden details must not repopulate their Markdown documents");
        compare(notesDocument.loadCount, notesLoadsBeforeHide, "Hidden note context must not repopulate its Markdown document");

        detail.visible = true;
        tryVerify(() => valueDocument.loadCount > valueLoadsBeforeHide, 3000);
        tryVerify(() => notesDocument.loadCount > notesLoadsBeforeHide, 3000);
        compare(valueDocument.lastLoadedSource, secondValue);
        compare(notesDocument.lastLoadedSource, secondNotes);
    }
}
