pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls

ColumnLayout {
    id: root

    property string username: ""
    property string password: ""
    property string url: ""
    property string notes: ""
    property bool passwordVisible: false

    signal generatePasswordRequested()

    spacing: 24

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 16
        Controls.Label {
            text: qsTr("Datos de acceso")
            font.weight: Font.DemiBold
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label { text: qsTr("Usuario") }
            Controls.TextField {
                Layout.fillWidth: true
                text: root.username
                Accessible.name: qsTr("Usuario")
                onTextEdited: root.username = text
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label { text: qsTr("Contraseña") }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Controls.TextField {
                    id: passwordField
                    objectName: "entryPasswordField"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    rightPadding: revealButton.width + 8
                    text: root.password
                    echoMode: root.passwordVisible ? TextInput.Normal : TextInput.Password
                    Accessible.name: qsTr("Contraseña")
                    onTextEdited: root.password = text
                    Controls.ToolButton {
                        id: revealButton
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: height
                        height: parent.height
                        text: root.passwordVisible ? qsTr("Ocultar contraseña") : qsTr("Mostrar contraseña")
                        icon.name: root.passwordVisible ? "view-hidden" : "view-visible"
                        display: Controls.AbstractButton.IconOnly
                        checkable: true
                        checked: root.passwordVisible
                        onClicked: root.passwordVisible = !root.passwordVisible
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
                Controls.Button {
                    text: qsTr("Generar")
                    icon.name: "tools-wizard"
                    Accessible.name: qsTr("Generar contraseña")
                    onClicked: root.generatePasswordRequested()
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label { text: qsTr("URL") }
            Controls.TextField {
                Layout.fillWidth: true
                text: root.url
                placeholderText: qsTr("https://ejemplo.test")
                Accessible.name: qsTr("URL del sitio")
                onTextEdited: root.url = text
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 8
        Controls.Label {
            text: qsTr("Notas")
            font.weight: Font.DemiBold
        }
        Controls.TextArea {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(implicitHeight, 80)
            text: root.notes
            placeholderText: qsTr("Añade contexto opcional para reconocer este elemento.")
            Accessible.name: qsTr("Notas de la credencial")
            wrapMode: TextEdit.Wrap
            selectByMouse: true
            textFormat: TextEdit.PlainText
            onTextChanged: root.notes = text
        }
    }
}
