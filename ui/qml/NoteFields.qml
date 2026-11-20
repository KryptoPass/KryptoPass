pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretFields {
    id: root

    property string value: ""
    property string notes: ""

    onValueChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        const loadedValue = fields.value || "";
        const loadedNotes = fields.notes || "";
        value = loadedValue;
        notes = loadedNotes;
        contentEditor.resetSource(loadedValue);
        notesEditor.resetSource(loadedNotes);
    }

    function currentFields() {
        return {
            value: value,
            notes: notes
        };
    }
    function clearFields() {
        value = "";
        notes = "";
        contentEditor.clearSensitiveState();
        notesEditor.clearSensitiveState();
    }

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        Layout.fillWidth: true
        text: KI18n.i18nc("@title:group", "Content")
        font.weight: Font.DemiBold
    }
    MarkdownEditor {
        id: contentEditor
        objectName: "markdownEditor"
        objectNamePrefix: "markdown"
        Layout.fillWidth: true
        editorMinimumHeight: Kirigami.Units.gridUnit * 9
        accessibleName: KI18n.i18n("Markdown content of the secure note")
        placeholderText: KI18n.i18nc("@info:placeholder", "Write the content you want to store encrypted.")
        onSourceTextEdited: text => root.value = text
    }

    NotesEditor {
        id: notesEditor
        Layout.fillWidth: true
        Layout.topMargin: 16
        label: KI18n.i18nc("@title:group", "Additional notes")
        accessibleName: KI18n.i18n("Markdown of the secure note's additional notes")
        placeholderText: KI18n.i18nc("@info:placeholder", "Add optional context.")
        onSourceTextEdited: text => root.notes = text
    }
}
