pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string username: ""
    property string password: ""
    property string url: ""
    property var urls: []
    property string urlsText: ""
    property string totpInput: ""
    property bool totpVisible: false
    property string notes: ""
    property bool passwordVisible: false
    readonly property var normalizedUrls: String(urlsText || "").split(/\r?\n/).map(value => value.trim()).filter(value => !!value)

    onUsernameChanged: edited()
    onPasswordChanged: edited()
    onUrlsTextChanged: edited()
    onTotpInputChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        username = fields.username || "";
        password = fields.password || "";
        setUrls(Array.isArray(fields.urls) ? fields.urls : [], fields.url || "");
        totpInput = fields.totpInput || "";
        notes = fields.notes || "";
        notesEditor.resetSource(notes);
        passwordVisible = false;
        totpVisible = false;
    }

    function currentFields() {
        return {
            username: username,
            password: password,
            url: normalizedUrls.length ? normalizedUrls[0] : "",
            urls: normalizedUrls,
            totpInput: totpInput,
            notes: notes
        };
    }

    function clearFields() {
        loadFields({});
    }
    function applyGeneratedPassword(value) {
        password = value;
    }

    spacing: 24

    function setUrls(values, legacyUrl) {
        const next = Array.isArray(values) ? values.map(value => String(value || "").trim()).filter(value => !!value) : [];
        if (!next.length && legacyUrl)
            next.push(String(legacyUrl).trim());
        urls = next;
        url = next.length ? next[0] : "";
        urlsText = next.join("\n");
    }

    function updateUrlsText(text) {
        urlsText = String(text || "");
        urls = normalizedUrls;
        url = normalizedUrls.length ? normalizedUrls[0] : "";
    }

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
            Controls.Label {
                text: qsTr("Correo o usuario")
            }
            Controls.TextField {
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                text: root.username
                Accessible.name: qsTr("Correo o usuario")
                onTextEdited: root.username = text
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label {
                text: qsTr("Contraseña")
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Controls.TextField {
                    id: passwordField
                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                    objectName: "entryPasswordField"
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    rightPadding: revealButton.width + 8
                    text: root.password
                    echoMode: root.passwordVisible ? TextInput.Normal : TextInput.Password
                    passwordMaskDelay: 0
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
            Controls.Label {
                text: qsTr("Autenticación de dos pasos (TOTP)")
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Controls.TextField {
                    id: totpField
                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    rightPadding: totpRevealButton.width + Kirigami.Units.smallSpacing
                    text: root.totpInput
                    echoMode: root.totpVisible ? TextInput.Normal : TextInput.Password
                    passwordMaskDelay: 0
                    placeholderText: qsTr("Base32 u otpauth:// (opcional)")
                    Accessible.name: qsTr("Semilla TOTP, inicialmente oculta")
                    onTextEdited: root.totpInput = text
                    Controls.ToolButton {
                        id: totpRevealButton
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: height
                        height: parent.height
                        text: root.totpVisible ? qsTr("Ocultar semilla TOTP") : qsTr("Mostrar semilla TOTP")
                        icon.name: root.totpVisible ? "view-hidden" : "view-visible"
                        display: Controls.AbstractButton.IconOnly
                        checkable: true
                        checked: root.totpVisible
                        onClicked: root.totpVisible = !root.totpVisible
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("La semilla se guarda cifrada. Déjala vacía para quitar TOTP; no aparece en la vista de lectura.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label {
                text: qsTr("Sitios web")
            }
            Controls.TextArea {
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 4)
                text: root.urlsText
                placeholderText: qsTr("https://ejemplo.test\nhttps://cuenta.ejemplo.test")
                Accessible.name: qsTr("Direcciones de sitios web, una por línea")
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                textFormat: TextEdit.PlainText
                onTextEdited: root.updateUrlsText(text)
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("La primera dirección se conserva como sitio principal.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }

    NotesEditor {
        id: notesEditor
        Layout.fillWidth: true
        placeholderText: qsTr("Añade contexto opcional para reconocer este elemento.")
        accessibleName: qsTr("Notas de la credencial")
        onSourceTextEdited: value => root.notes = value
    }
}
