pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var detail: null
    property bool revealed: false

    signal revealToggled()
    signal copyUsernameRequested()
    signal copyPasswordRequested()

    Layout.fillWidth: true
    spacing: Kirigami.Units.largeSpacing

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.detail && root.detail.kind === "login"
        padding: Kirigami.Units.largeSpacing

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Kirigami.FormLayout {
                Layout.fillWidth: true

                Controls.Label {
                    Kirigami.FormData.label: qsTr("Sitio web:")
                    Layout.fillWidth: true
                    text: root.detail ? root.detail.url || qsTr("Sin URL") : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Controls.Label {
                    Kirigami.FormData.label: qsTr("Usuario:")
                    Layout.fillWidth: true
                    text: root.detail ? root.detail.username || qsTr("Sin usuario") : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: qsTr("Copiar usuario")
                    icon.name: "edit-copy"
                    enabled: !!root.detail && !!root.detail.username
                    onClicked: root.copyUsernameRequested()
                }
                Item { Layout.fillWidth: true }
            }

            Kirigami.FormLayout {
                Layout.fillWidth: true
                Controls.Label {
                    Kirigami.FormData.label: qsTr("Contraseña:")
                    Layout.fillWidth: true
                    text: root.detail && root.detail.password
                            ? root.detail.password
                            : root.revealed ? qsTr("Sin contraseña") : qsTr("Contraseña oculta")
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Controls.Button {
                    text: root.revealed ? qsTr("Ocultar contraseña") : qsTr("Mostrar contraseña")
                    icon.name: root.revealed ? "view-hidden" : "view-visible"
                    onClicked: root.revealToggled()
                }
                Controls.Button {
                    text: qsTr("Copiar contraseña")
                    icon.name: "edit-copy"
                    enabled: !!root.detail && root.detail.kind === "login"
                    onClicked: root.copyPasswordRequested()
                }
                Item { Layout.fillWidth: true }
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.detail && root.detail.kind === "note"
        padding: Kirigami.Units.largeSpacing

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                text: qsTr("Contenido")
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 6)
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                text: root.detail ? root.detail.value || qsTr("Sin contenido") : ""
                Accessible.name: qsTr("Contenido de la nota segura")
            }
        }
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: !!root.detail && !!root.detail.notes
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            Controls.Label {
                text: qsTr("Notas")
                color: Kirigami.Theme.disabledTextColor
            }
            Controls.TextArea {
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 3)
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                textFormat: TextEdit.PlainText
                text: root.detail ? root.detail.notes || "" : ""
                Accessible.name: qsTr("Notas del elemento")
            }
        }
    }
}
