pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

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
        return {
            ssid: ssid,
            security: security,
            password: password,
            hidden: hidden,
            notes: notes
        };
    }

    function clearFields() {
        loadFields({});
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        SecretInput {
            Layout.fillWidth: true
            label: KI18n.i18nc("@label:textbox", "Network name (SSID)")
            value: root.ssid
            onValueEdited: function (value) {
                root.ssid = value;
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                text: KI18n.i18nc("@label Wi-Fi security", "Security")
            }
            Controls.ComboBox {
                id: securityField
                objectName: "wifiSecurityField"
                Layout.fillWidth: true
                model: [
                    {
                        value: "open",
                        label: KI18n.i18nc("@item:inlistbox Wi-Fi security", "Open")
                    },
                    {
                        value: "wep",
                        label: KI18n.i18nc("@item:inlistbox Wi-Fi security", "WEP")
                    },
                    {
                        value: "wpa",
                        label: KI18n.i18nc("@item:inlistbox Wi-Fi security", "WPA / WPA2")
                    },
                    {
                        value: "wpa3",
                        label: KI18n.i18nc("@item:inlistbox Wi-Fi security", "WPA3")
                    }
                ]
                textRole: "label"
                valueRole: "value"
                currentIndex: Math.max(0, ["open", "wep", "wpa", "wpa3"].indexOf(root.security))
                Accessible.name: KI18n.i18n("Wi-Fi security type")
                onActivated: root.security = currentValue
            }
        }

        SecretInput {
            Layout.fillWidth: true
            label: KI18n.i18nc("@label:textbox", "Network password")
            value: root.password
            concealed: true
            hint: root.security === "wpa3" ? KI18n.i18n("The connection QR code is available for open, WEP and WPA/WPA2 networks.") : root.security === "open" ? KI18n.i18n("Leave it empty for an open network.") : ""
            onValueEdited: function (value) {
                root.password = value;
            }
        }

        Controls.CheckBox {
            text: KI18n.i18nc("@option:check", "Hidden network")
            Accessible.name: KI18n.i18n("The Wi-Fi network is hidden")
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
