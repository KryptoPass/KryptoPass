pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property string username: ""
    property string password: ""
    property string url: ""
    property string notes: ""
    property bool passwordVisible: false

    signal generatePasswordRequested()

    spacing: Kirigami.Units.smallSpacing

    Controls.Label {
        text: qsTr("Usuario")
    }
    Controls.TextField {
        Layout.fillWidth: true
        text: root.username
        Accessible.name: qsTr("Usuario")
        onTextEdited: root.username = text
    }

    Controls.Label {
        text: qsTr("Contraseña")
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Controls.TextField {
            Layout.fillWidth: true
            text: root.password
            echoMode: root.passwordVisible ? TextInput.Normal : TextInput.Password
            Accessible.name: qsTr("Contraseña")
            onTextEdited: root.password = text
        }
        Controls.ToolButton {
            text: root.passwordVisible ? qsTr("Ocultar") : qsTr("Mostrar")
            icon.name: root.passwordVisible ? "view-hidden" : "view-visible"
            checkable: true
            checked: root.passwordVisible
            Accessible.name: root.passwordVisible ? qsTr("Ocultar contraseña") : qsTr("Mostrar contraseña")
            onToggled: root.passwordVisible = checked
        }
        Controls.ToolButton {
            text: qsTr("Generar")
            icon.name: "tools-wizard"
            Accessible.name: qsTr("Generar contraseña")
            onClicked: root.generatePasswordRequested()
        }
    }

    Controls.Label {
        text: qsTr("URL")
    }
    Controls.TextField {
        Layout.fillWidth: true
        text: root.url
        placeholderText: qsTr("https://ejemplo.test")
        Accessible.name: qsTr("URL del sitio")
        onTextEdited: root.url = text
    }

    Controls.Label {
        text: qsTr("Notas")
    }
    Controls.TextArea {
        Layout.fillWidth: true
        Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 4)
        text: root.notes
        placeholderText: qsTr("Añade contexto opcional para reconocer este elemento.")
        Accessible.name: qsTr("Notas de la credencial")
        wrapMode: TextEdit.Wrap
        selectByMouse: true
        textFormat: TextEdit.PlainText
        onTextChanged: root.notes = text
    }
}
