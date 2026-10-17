pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

// Reusable editor for original Markdown source and its disposable preview.
// `sourceText` always holds the plain source. `sourceTextEdited` reports edits
// made in the source field or by toolbar actions; `resetSource()` loads a new
// draft without reporting an edit. Consumers must call `resetSource()` for
// every loaded source, even when it matches the previous value, and call
// `clearSensitiveState()` to clear the source, undo/redo history and preview
// when discarding the draft. A visual consumer should set its width and, when
// needed, `editorMinimumHeight`.
ColumnLayout {
    id: root

    objectName: "markdownEditor"
    property string objectNamePrefix: "markdown"
    property string sourceText: ""
    property bool previewMode: false
    property string accessibleName: qsTr("Fuente Markdown")
    property string placeholderText: qsTr("Escribe el texto que quieres guardar cifrado.")
    property real editorMinimumHeight: Kirigami.Units.gridUnit * 9
    property bool synchronizingSource: false
    readonly property bool effectivelyVisible: {
        let item = root;
        while (item) {
            if (!item.visible)
                return false;
            item = item.parent;
        }
        return true;
    }

    signal sourceTextEdited(string value)

    spacing: Kirigami.Units.smallSpacing

    readonly property var toolbarActions: [
        {
            name: "undo",
            suffix: "UndoButton",
            label: qsTr("Deshacer"),
            icon: "edit-undo",
            tip: qsTr("Deshacer")
        },
        {
            name: "redo",
            suffix: "RedoButton",
            label: qsTr("Rehacer"),
            icon: "edit-redo",
            tip: qsTr("Rehacer")
        },
        {
            name: "bold",
            suffix: "BoldButton",
            label: qsTr("Negrita"),
            icon: "format-text-bold",
            tip: qsTr("Negrita · Ctrl+B")
        },
        {
            name: "italic",
            suffix: "ItalicButton",
            label: qsTr("Cursiva"),
            icon: "format-text-italic",
            tip: qsTr("Cursiva · Ctrl+I")
        },
        {
            name: "strike",
            suffix: "StrikeButton",
            label: qsTr("Tachado"),
            icon: "format-text-strikethrough",
            tip: qsTr("Tachado")
        },
        {
            name: "heading",
            suffix: "HeadingButton",
            label: qsTr("Encabezado"),
            icon: "format-text-uppercase",
            tip: qsTr("Encabezado de nivel 2")
        },
        {
            name: "bulletList",
            suffix: "BulletListButton",
            label: qsTr("Lista con viñetas"),
            icon: "format-list-unordered",
            tip: qsTr("Lista con viñetas")
        },
        {
            name: "orderedList",
            suffix: "OrderedListButton",
            label: qsTr("Lista numerada"),
            icon: "format-list-ordered",
            tip: qsTr("Lista numerada")
        },
        {
            name: "taskList",
            suffix: "TaskListButton",
            label: qsTr("Lista de tareas"),
            icon: "view-task",
            tip: qsTr("Lista de tareas")
        },
        {
            name: "quote",
            suffix: "QuoteButton",
            label: qsTr("Cita"),
            icon: "format-text-blockquote",
            tip: qsTr("Cita")
        },
        {
            name: "inlineCode",
            suffix: "InlineCodeButton",
            label: qsTr("Código en línea"),
            icon: "format-text-code",
            tip: qsTr("Código en línea")
        },
        {
            name: "codeBlock",
            suffix: "CodeBlockButton",
            label: qsTr("Bloque de código"),
            icon: "code-context",
            tip: qsTr("Bloque de código")
        },
        {
            name: "link",
            suffix: "LinkButton",
            label: qsTr("Insertar enlace"),
            icon: "insert-link",
            tip: qsTr("Insertar enlace · Ctrl+K")
        },
        {
            name: "spoiler",
            suffix: "SpoilerButton",
            label: qsTr("Insertar spoiler"),
            icon: "view-hidden",
            tip: qsTr("Insertar spoiler")
        },
        {
            name: "highlight",
            suffix: "HighlightButton",
            label: qsTr("Resaltar"),
            icon: "draw-highlight",
            tip: qsTr("Resaltar")
        }
    ]

    function clearPreview() {
        previewTimer.stop();
        previewView.clearSelection();
        previewDocument.clear();
    }

    function resetTextDocument(value) {
        const source = value || "";
        root.synchronizingSource = true;
        root.sourceText = source;
        sourceEditor.clear();
        // clear() resets pending IME input, but setting an already-empty text is
        // a no-op and can preserve native undo history. Force a distinct value
        // before restoring the requested source so Qt resets that history.
        sourceEditor.text = source.length > 0 ? "" : "\u2060";
        sourceEditor.text = source;
        sourceEditor.deselect();
        sourceEditor.cursorPosition = 0;
        root.synchronizingSource = false;
    }

    function resetSource(value) {
        const source = value || "";
        root.previewMode = false;
        root.clearPreview();
        root.resetTextDocument(source);
    }

    function clearSensitiveState() {
        root.clearPreview();
        root.previewMode = false;
        root.resetTextDocument("");
    }

    function setPreviewMode(enabled) {
        if (root.previewMode === enabled) {
            if (enabled && root.effectivelyVisible)
                previewTimer.restart();
            return;
        }

        root.previewMode = enabled;
        if (!enabled)
            sourceEditor.forceActiveFocus();
    }

    function replaceText(start, end, replacement) {
        sourceEditor.select(start, end);
        sourceEditor.cursorSelection.text = replacement;
    }

    function toggleInline(opening, closing) {
        const start = sourceEditor.selectionStart;
        const end = sourceEditor.selectionEnd;
        const selected = end > start ? sourceEditor.getText(start, end) : "";
        const source = sourceEditor.text;
        const adjacentOpening = start > opening.length ? source.substring(start - opening.length - 1, start - opening.length) : "";
        const adjacentClosing = source.substring(end + closing.length, end + closing.length + 1);

        if (end > start && start >= opening.length && source.substring(start - opening.length, start) === opening && source.substring(end, end + closing.length) === closing && adjacentOpening !== opening.charAt(0) && adjacentClosing !== closing.charAt(0)) {
            root.replaceText(start - opening.length, end + closing.length, selected);
            sourceEditor.select(start - opening.length, start - opening.length + selected.length);
        } else if (selected.length >= opening.length + closing.length && selected.startsWith(opening) && selected.endsWith(closing)) {
            const inner = selected.substring(opening.length, selected.length - closing.length);
            root.replaceText(start, end, inner);
            sourceEditor.select(start, start + inner.length);
        } else if (end > start) {
            root.replaceText(start, end, opening + selected + closing);
            sourceEditor.select(start + opening.length, start + opening.length + selected.length);
        } else {
            root.replaceText(start, end, opening + closing);
            sourceEditor.cursorPosition = start + opening.length;
        }

        sourceEditor.forceActiveFocus();
    }

    function transformLines(transform) {
        const source = sourceEditor.text;
        const start = sourceEditor.selectionStart;
        const end = sourceEditor.selectionEnd;
        const hasSelection = end > start;
        const cursor = sourceEditor.cursorPosition;
        const anchor = hasSelection ? start : cursor;
        const lineStart = anchor > 0 ? source.lastIndexOf("\n", anchor - 1) + 1 : 0;
        const rangeEnd = hasSelection && source.charAt(end - 1) === "\n" ? end - 1 : (hasSelection ? end : cursor);
        let lineEnd = source.indexOf("\n", rangeEnd);
        if (lineEnd < 0)
            lineEnd = source.length;

        const original = sourceEditor.getText(lineStart, lineEnd);
        const originalLines = original.split("\n");
        const transformedLines = transform(originalLines);
        const replacement = transformedLines.join("\n");
        root.replaceText(lineStart, lineEnd, replacement);

        if (hasSelection) {
            sourceEditor.select(lineStart, lineStart + replacement.length);
        } else {
            const offset = Math.max(0, Math.min(originalLines[0].length, cursor - lineStart));
            sourceEditor.cursorPosition = lineStart + Math.max(0, Math.min(transformedLines[0].length, offset + transformedLines[0].length - originalLines[0].length));
        }

        sourceEditor.forceActiveFocus();
    }

    function applyPrefix(prefix, matches, strip) {
        root.transformLines(lines => {
            const removePrefix = lines.length > 0 && lines.every(matches);
            return lines.map(line => removePrefix ? strip(line) : prefix + line);
        });
    }

    function applyHeading() {
        root.transformLines(lines => {
            const removeHeading = lines.length > 0 && lines.every(line => /^##(?:\s|$)/.test(line));
            return lines.map(line => {
                if (removeHeading)
                    return line.replace(/^##(?:\s|$)/, "");
                return "## " + line.replace(/^#{1,6}[ \t]+/, "");
            });
        });
    }

    function toggleCodeBlock() {
        const hadSelection = sourceEditor.selectionEnd > sourceEditor.selectionStart;
        const cursor = sourceEditor.cursorPosition;
        const lineStart = cursor > 0 ? sourceEditor.text.lastIndexOf("\n", cursor - 1) + 1 : 0;
        root.transformLines(lines => {
            if (lines.length >= 2 && lines[0] === "```" && lines[lines.length - 1] === "```")
                return lines.slice(1, -1);
            return ["```"].concat(lines, ["```"]);
        });
        if (!hadSelection)
            sourceEditor.cursorPosition = lineStart + 4 + cursor - lineStart;
    }

    function insertLink() {
        const start = sourceEditor.selectionStart;
        const end = sourceEditor.selectionEnd;
        const selected = end > start ? sourceEditor.getText(start, end) : "";
        if (selected.length > 0) {
            root.replaceText(start, end, "[" + selected + "](https://)");
            const urlStart = start + selected.length + 3;
            sourceEditor.select(urlStart, urlStart + "https://".length);
        } else {
            root.replaceText(start, end, "[](https://)");
            sourceEditor.cursorPosition = start + 1;
        }
        sourceEditor.forceActiveFocus();
    }

    function performAction(name) {
        switch (name) {
        case "undo":
            sourceEditor.undo();
            break;
        case "redo":
            sourceEditor.redo();
            break;
        case "bold":
            root.toggleInline("**", "**");
            break;
        case "italic":
            root.toggleInline("*", "*");
            break;
        case "strike":
            root.toggleInline("~~", "~~");
            break;
        case "heading":
            root.applyHeading();
            break;
        case "bulletList":
            root.applyPrefix("- ", line => /^[-*+]\s/.test(line), line => line.replace(/^[-*+]\s/, ""));
            break;
        case "orderedList":
            root.applyPrefix("1. ", line => /^\d+\.\s/.test(line), line => line.replace(/^\d+\.\s/, ""));
            break;
        case "taskList":
            root.applyPrefix("- [ ] ", line => /^- \[ \]\s/.test(line), line => line.replace(/^- \[ \]\s/, ""));
            break;
        case "quote":
            root.applyPrefix("> ", line => /^>\s/.test(line), line => line.replace(/^>\s/, ""));
            break;
        case "inlineCode":
            root.toggleInline("`", "`");
            break;
        case "codeBlock":
            root.toggleCodeBlock();
            break;
        case "link":
            root.insertLink();
            break;
        case "spoiler":
            root.toggleInline("||", "||");
            break;
        case "highlight":
            root.toggleInline("==", "==");
            break;
        }
    }

    onSourceTextChanged: {
        if (!root.synchronizingSource && sourceEditor.text !== root.sourceText)
            root.resetSource(root.sourceText);
    }
    onPreviewModeChanged: {
        if (root.previewMode && root.effectivelyVisible) {
            previewTimer.restart();
        } else {
            root.clearPreview();
            sourceEditor.deselect();
        }
    }
    onEffectivelyVisibleChanged: {
        if (!root.effectivelyVisible) {
            root.clearPreview();
            sourceEditor.deselect();
        } else if (root.previewMode) {
            previewTimer.restart();
        }
    }
    Component.onCompleted: root.resetSource(root.sourceText)
    Component.onDestruction: root.clearSensitiveState()

    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Controls.ButtonGroup {
            id: previewModeGroup
        }

        Controls.ToolButton {
            objectName: root.objectNamePrefix + "EditModeButton"
            text: qsTr("Editar")
            icon.name: "document-edit"
            checkable: true
            autoExclusive: true
            Controls.ButtonGroup.group: previewModeGroup
            checked: !root.previewMode
            activeFocusOnTab: true
            Accessible.name: qsTr("Editar Markdown")
            onClicked: root.setPreviewMode(false)
        }
        Controls.ToolButton {
            objectName: root.objectNamePrefix + "ModeButton"
            text: qsTr("Previsualizar")
            icon.name: "document-preview"
            checkable: true
            autoExclusive: true
            Controls.ButtonGroup.group: previewModeGroup
            checked: root.previewMode
            activeFocusOnTab: true
            Accessible.name: qsTr("Previsualizar Markdown")
            onClicked: root.setPreviewMode(true)
        }
        Item {
            Layout.fillWidth: true
        }
    }

    Flow {
        id: toolbar
        objectName: root.objectNamePrefix + "Toolbar"
        Layout.fillWidth: true
        visible: !root.previewMode
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: root.toolbarActions
            delegate: Controls.ToolButton {
                id: toolButton
                required property var modelData
                objectName: root.objectNamePrefix + toolButton.modelData.suffix
                width: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
                height: width
                padding: Kirigami.Units.smallSpacing
                display: Controls.AbstractButton.IconOnly
                icon.name: toolButton.modelData.icon
                enabled: toolButton.modelData.name === "undo" ? sourceEditor.canUndo : toolButton.modelData.name === "redo" ? sourceEditor.canRedo : true
                activeFocusOnTab: true
                Accessible.name: toolButton.modelData.label
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: toolButton.modelData.tip
                onClicked: root.performAction(toolButton.modelData.name)
            }
        }
    }

    StackLayout {
        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.minimumHeight: root.editorMinimumHeight
        Layout.preferredHeight: root.editorMinimumHeight
        currentIndex: root.previewMode ? 1 : 0

        Controls.ScrollView {
            id: sourceScroll
            objectName: root.objectNamePrefix + "SourceScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth

            Controls.TextArea {
                id: sourceEditor
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                objectName: root.objectNamePrefix + "SourceEditor"
                width: sourceScroll.availableWidth
                implicitHeight: Math.max(root.editorMinimumHeight, contentHeight + topPadding + bottomPadding)
                textFormat: TextEdit.PlainText
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                selectByKeyboard: true
                persistentSelection: true
                activeFocusOnTab: true
                Accessible.name: root.accessibleName
                placeholderText: root.placeholderText
                onTextChanged: {
                    if (!root.synchronizingSource) {
                        root.sourceText = text;
                        root.sourceTextEdited(text);
                    }
                }
            }
        }

        Controls.ScrollView {
            id: previewScroll
            objectName: root.objectNamePrefix + "PreviewScroll"
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth

            ColumnLayout {
                width: previewScroll.availableWidth
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    visible: root.sourceText.length === 0
                    text: qsTr("No hay contenido para previsualizar.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                MarkdownView {
                    id: previewView
                    objectName: root.objectNamePrefix + "Preview"
                    Layout.fillWidth: true
                    visible: root.sourceText.length > 0
                    document: previewDocument
                    accessibleLabel: root.accessibleName
                    onLinkRequested: link => Qt.openUrlExternally(link)
                }
            }
        }
    }

    MarkdownDocument {
        id: previewDocument
        objectName: root.objectNamePrefix + "PreviewDocument"
    }

    Timer {
        id: previewTimer
        objectName: root.objectNamePrefix + "PreviewTimer"
        interval: 120
        repeat: false
        onTriggered: {
            if (!root.effectivelyVisible || !root.previewMode)
                return;
            previewView.clearSelection();
            if (root.sourceText.length > 0)
                previewDocument.load(root.sourceText);
            else
                previewDocument.clear();
        }
    }

    Shortcut {
        sequence: "Ctrl+B"
        context: Qt.WindowShortcut
        enabled: !root.previewMode && sourceEditor.activeFocus
        onActivated: root.toggleInline("**", "**")
    }
    Shortcut {
        sequence: "Ctrl+I"
        context: Qt.WindowShortcut
        enabled: !root.previewMode && sourceEditor.activeFocus
        onActivated: root.toggleInline("*", "*")
    }
    Shortcut {
        sequence: "Ctrl+K"
        context: Qt.WindowShortcut
        enabled: !root.previewMode && sourceEditor.activeFocus
        onActivated: root.insertLink()
    }
}
