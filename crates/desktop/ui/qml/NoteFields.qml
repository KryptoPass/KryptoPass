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
        value = fields.value || "";
        notes = fields.notes || "";
    }

    function currentFields() { return { value: value, notes: notes }; }
    function clearFields() { loadFields({}); }

    spacing: 8

    Controls.Label {
        text: qsTr("Contenido")
        font.weight: Font.DemiBold
    }
    Controls.TextArea {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 7)
        text: root.value
        Accessible.name: qsTr("Contenido de la nota segura")
        placeholderText: qsTr("Escribe la nota que quieres guardar cifrada.")
        wrapMode: TextEdit.Wrap
        selectByMouse: true
        textFormat: TextEdit.PlainText
        onTextChanged: root.value = text
    }

    Controls.Label {
        Layout.topMargin: 16
        text: qsTr("Notas")
        font.weight: Font.DemiBold
    }
    Controls.TextArea {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(implicitHeight, 80)
        text: root.notes
        Accessible.name: qsTr("Notas de la nota segura")
        placeholderText: qsTr("Añade contexto opcional.")
        wrapMode: TextEdit.Wrap
        selectByMouse: true
        textFormat: TextEdit.PlainText
        onTextChanged: root.notes = text
    }
}
