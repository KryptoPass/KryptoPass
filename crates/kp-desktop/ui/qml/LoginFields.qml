pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string username: ""
    property string password: ""
    property var urls: []
    property string urlsText: ""
    property string totpInput: ""
    property bool totpVisible: false
    property string notes: ""
    property bool passwordVisible: false
    readonly property var normalizedUrls: String(urlsText || "").split(/\r?\n/).map(value => value.trim()).filter(value => !!value)
    readonly property bool usernameInvalid: invalidField === "username"
    readonly property bool passwordInvalid: invalidField === "password"
    readonly property bool totpInvalid: invalidField === "totp"
    readonly property bool urlsInvalid: invalidField === "urls"

    onUsernameChanged: edited()
    onPasswordChanged: edited()
    onUrlsTextChanged: edited()
    onTotpInputChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        username = fields.username || "";
        password = fields.password || "";
        setUrls(Array.isArray(fields.urls) ? fields.urls : []);
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

    function focusInvalidField() {
        const target = usernameInvalid ? usernameField : passwordInvalid ? passwordField : totpInvalid ? totpField : urlsInvalid ? urlsField : null;
        if (!target)
            return false;
        target.forceActiveFocus();
        return true;
    }

    spacing: 24

    Binding {
        target: root
        property: "missingRequiredFields"
        value: root.password.length > 0 ? [] : [qsTr("Contraseña")]
    }

    function setUrls(values) {
        const next = Array.isArray(values) ? values.map(value => String(value || "").trim()).filter(value => !!value) : [];
        urls = next;
        urlsText = next.join("\n");
    }

    function updateUrlsText(text) {
        urlsText = String(text || "");
        urls = normalizedUrls;
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 16
        Accessible.role: Accessible.Grouping
        Accessible.name: qsTr("Datos de acceso")
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
                id: usernameField
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                text: root.username
                Accessible.name: qsTr("Correo o usuario")
                Accessible.description: root.usernameInvalid ? qsTr("Revisa este campo.") : ""
                onTextEdited: root.username = text
                Rectangle {
                    anchors.fill: parent
                    visible: root.usernameInvalid
                    color: "transparent"
                    border.width: 1
                    border.color: Kirigami.Theme.negativeTextColor
                    radius: Kirigami.Units.smallSpacing
                    z: 2
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label {
                text: qsTr("Contraseña *")
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
                    Accessible.description: root.passwordInvalid ? qsTr("Obligatorio. Revisa este campo.") : qsTr("Obligatorio.")
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
                        Accessible.checkable: true
                        Accessible.checked: root.passwordVisible
                        onClicked: root.passwordVisible = !root.passwordVisible
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Rectangle {
                        anchors.fill: parent
                        visible: root.passwordInvalid
                        color: "transparent"
                        border.width: 1
                        border.color: Kirigami.Theme.negativeTextColor
                        radius: Kirigami.Units.smallSpacing
                        z: 2
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
                    Accessible.description: root.totpInvalid ? qsTr("La semilla TOTP no es válida. %1").arg(totpHint.text) : totpHint.text
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
                        Accessible.checkable: true
                        Accessible.checked: root.totpVisible
                        onClicked: root.totpVisible = !root.totpVisible
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                    Rectangle {
                        anchors.fill: parent
                        visible: root.totpInvalid
                        color: "transparent"
                        border.width: 1
                        border.color: Kirigami.Theme.negativeTextColor
                        radius: Kirigami.Units.smallSpacing
                        z: 2
                    }
                }
            }
            Controls.Label {
                id: totpHint
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
                id: urlsField
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 4)
                text: root.urlsText
                placeholderText: qsTr("https://ejemplo.test\nhttps://cuenta.ejemplo.test")
                Accessible.name: qsTr("Direcciones de sitios web, una por línea")
                Accessible.description: root.urlsInvalid ? qsTr("Revisa las direcciones web. %1").arg(urlsHint.text) : urlsHint.text
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                textFormat: TextEdit.PlainText
                onTextEdited: root.updateUrlsText(text)
                Keys.onTabPressed: function (event) {
                    const next = nextItemInFocusChain();
                    if (next)
                        next.forceActiveFocus(Qt.TabFocusReason);
                    event.accepted = true;
                }
                Rectangle {
                    anchors.fill: parent
                    visible: root.urlsInvalid
                    color: "transparent"
                    border.width: 1
                    border.color: Kirigami.Theme.negativeTextColor
                    radius: Kirigami.Units.smallSpacing
                    z: 2
                }
            }
            Controls.Label {
                id: urlsHint
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
