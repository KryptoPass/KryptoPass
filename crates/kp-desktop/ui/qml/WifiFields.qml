pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string ssid: ""
    property string security: "wpa"
    property string password: ""
    property bool hidden: false
    property string notes: ""

    spacing: Kirigami.Units.largeSpacing

    onSsidChanged: edited()
    onSecurityChanged: edited()
    onPasswordChanged: edited()
    onHiddenChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        ssid = String(fields.ssid || "");
        security = ["open", "wep", "wpa", "wpa3"].includes(fields.security) ? fields.security : "wpa";
        password = String(fields.password || "");
        hidden = !!fields.hidden;
        notes = String(fields.notes || "");
        notesEditor.resetSource(notes);
    }

    function currentFields() {
        return { ssid: ssid, security: security, password: password, hidden: hidden, notes: notes };
    }

    function clearFields() { loadFields({}); }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Nombre de la red (SSID)")
            value: root.ssid
            onValueEdited: function(value) { root.ssid = value; }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Label { text: qsTr("Seguridad") }
            Controls.ComboBox {
                id: securityField
                objectName: "wifiSecurityField"
                Layout.fillWidth: true
                model: [
                    { value: "open", label: qsTr("Abierta") },
                    { value: "wep", label: qsTr("WEP") },
                    { value: "wpa", label: qsTr("WPA / WPA2") },
                    { value: "wpa3", label: qsTr("WPA3") }
                ]
                textRole: "label"
                valueRole: "value"
                currentIndex: Math.max(0, ["open", "wep", "wpa", "wpa3"].indexOf(root.security))
                Accessible.name: qsTr("Tipo de seguridad Wi-Fi")
                onActivated: root.security = currentValue
            }
        }

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Contraseña de la red")
            value: root.password
            concealed: true
            hint: root.security === "wpa3"
                ? qsTr("El QR de conexión está disponible para redes abiertas, WEP y WPA/WPA2.")
                : root.security === "open" ? qsTr("Déjala vacía para una red abierta.") : ""
            onValueEdited: function(value) { root.password = value; }
        }

        Controls.CheckBox {
            text: qsTr("Red oculta")
            Accessible.name: qsTr("La red Wi-Fi está oculta")
            checked: root.hidden
            onToggled: root.hidden = checked
        }

        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
