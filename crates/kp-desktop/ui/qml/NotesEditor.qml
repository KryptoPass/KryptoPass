pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property string label: qsTr("Notas")
    property alias accessibleName: editor.accessibleName
    property alias placeholderText: editor.placeholderText
    readonly property string sourceText: editor.sourceText

    signal sourceTextEdited(string value)

    spacing: Kirigami.Units.smallSpacing

    function resetSource(value) {
        editor.resetSource(value);
    }

    function clearSensitiveState() {
        editor.clearSensitiveState();
    }

    Controls.Label {
        Layout.fillWidth: true
        text: root.label
        textFormat: Text.PlainText
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
    }

    MarkdownEditor {
        id: editor
        objectName: "markdownNotesEditor"
        objectNamePrefix: "markdownNotes"
        Layout.fillWidth: true
        editorMinimumHeight: Kirigami.Units.gridUnit * 5
        accessibleName: qsTranslate("SecretDetail", "Notas del elemento")
        placeholderText: qsTranslate("NoteFields", "Añade contexto opcional.")
        onSourceTextEdited: value => root.sourceTextEdited(value)
    }
}
