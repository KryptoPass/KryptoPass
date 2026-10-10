pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property string value: ""
    property string notes: ""

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        text: qsTr("Contenido")
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
        text: qsTr("Notas")
    }
    Controls.TextArea {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 4)
        text: root.notes
        Accessible.name: qsTr("Notas de la nota segura")
        placeholderText: qsTr("Añade contexto opcional.")
        wrapMode: TextEdit.Wrap
        selectByMouse: true
        textFormat: TextEdit.PlainText
        onTextChanged: root.notes = text
    }
}
