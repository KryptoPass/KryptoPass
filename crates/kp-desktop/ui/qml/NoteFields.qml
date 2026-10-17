pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

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

    function currentFields() { return { value: value, notes: notes }; }
    function clearFields() {
        value = "";
        notes = "";
        contentEditor.clearSensitiveState();
        notesEditor.clearSensitiveState();
    }

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        Layout.fillWidth: true
        text: qsTr("Contenido")
        font.weight: Font.DemiBold
    }
    MarkdownEditor {
        id: contentEditor
        objectName: "markdownEditor"
        objectNamePrefix: "markdown"
        Layout.fillWidth: true
        editorMinimumHeight: Kirigami.Units.gridUnit * 9
        accessibleName: qsTr("Contenido Markdown de la nota segura")
        placeholderText: qsTr("Escribe el contenido que quieres guardar cifrado.")
        onSourceTextEdited: text => root.value = text
    }

    NotesEditor {
        id: notesEditor
        Layout.fillWidth: true
        Layout.topMargin: 16
        label: qsTr("Notas adicionales")
        accessibleName: qsTr("Markdown de las notas adicionales de la nota segura")
        placeholderText: qsTr("Añade contexto opcional.")
        onSourceTextEdited: text => root.notes = text
    }
}
