pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretFields {
    id: root

    property string username: ""
    property string password: ""
    property var urls: []
    property string totpInput: ""
    property bool totpVisible: false
    property string notes: ""
    property bool passwordVisible: false
    readonly property var normalizedUrls: (Array.isArray(urls) ? urls : []).map(value => String(value || "").trim()).filter(value => !!value)
    readonly property bool usernameInvalid: invalidField === "username"
    readonly property bool passwordInvalid: invalidField === "password"
    readonly property bool totpInvalid: invalidField === "totp"
    readonly property bool urlsInvalid: invalidField === "urls"

    onUsernameChanged: edited()
    onPasswordChanged: edited()
    onUrlsChanged: edited()
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
        if (usernameInvalid) {
            usernameField.forceActiveFocus();
            return true;
        }
        if (passwordInvalid) {
            passwordField.forceActiveFocus();
            return true;
        }
        if (totpInvalid) {
            totpField.forceActiveFocus();
            return true;
        }
        if (urlsInvalid)
            return urlsEditor.focusFirstValue();
        return false;
    }

    spacing: 24

    Binding {
        target: root
        property: "missingRequiredFields"
        value: root.password.length > 0 ? [] : [KI18n.i18nc("@label", "Password")]
    }

    function setUrls(values) {
        urls = Array.isArray(values) ? values.map(value => String(value || "").trim()).filter(value => !!value) : [];
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: 16
        Accessible.role: Accessible.Grouping
        Accessible.name: KI18n.i18nc("@title:group", "Sign-in details")
        Controls.Label {
            text: KI18n.i18nc("@title:group", "Sign-in details")
            font.weight: Font.DemiBold
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label {
                text: KI18n.i18nc("@label:textbox", "Email or username")
            }
            Controls.TextField {
                id: usernameField
                inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
                Layout.fillWidth: true
                text: root.username
                Accessible.name: KI18n.i18nc("@label:textbox", "Email or username")
                Accessible.description: root.usernameInvalid ? KI18n.i18n("Check this field.") : ""
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
                text: KI18n.i18nc("@label:textbox required field", "Password *")
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
                    Accessible.name: KI18n.i18nc("@label", "Password")
                    Accessible.description: root.passwordInvalid ? KI18n.i18n("Required. Check this field.") : KI18n.i18n("Required.")
                    onTextEdited: root.password = text
                    Controls.ToolButton {
                        id: revealButton
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: height
                        height: parent.height
                        text: root.passwordVisible ? KI18n.i18nc("@action:button", "Hide Password") : KI18n.i18nc("@action:button", "Show Password")
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
                    text: KI18n.i18nc("@action:button", "Generate")
                    icon.name: "tools-wizard"
                    Accessible.name: KI18n.i18nc("@title:window", "Generate Password")
                    onClicked: root.generatePasswordRequested()
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Controls.Label {
                text: KI18n.i18nc("@title:group", "Two-step authentication (TOTP)")
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
                    placeholderText: KI18n.i18nc("@info:placeholder", "Base32 or otpauth:// (optional)")
                    Accessible.name: KI18n.i18n("TOTP secret, hidden at first")
                    Accessible.description: root.totpInvalid ? KI18n.i18n("The TOTP secret is not valid. %1", totpHint.text) : totpHint.text
                    onTextEdited: root.totpInput = text
                    Controls.ToolButton {
                        id: totpRevealButton
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        width: height
                        height: parent.height
                        text: root.totpVisible ? KI18n.i18nc("@action:button", "Hide TOTP Secret") : KI18n.i18nc("@action:button", "Show TOTP Secret")
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
                text: KI18n.i18n("The secret is stored encrypted. Leave it empty to remove TOTP; it is not shown in the read view.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            StringListEditor {
                id: urlsEditor
                objectName: "loginUrlsEditor"
                Layout.fillWidth: true
                label: KI18n.i18nc("@title:group", "Websites")
                inputLabel: KI18n.i18nc("@label:textbox", "Website")
                addLabel: KI18n.i18nc("@action:button", "Add Website")
                maximumValues: 100
                invalid: root.urlsInvalid
                accessibleDescription: root.urlsInvalid ? KI18n.i18n("Check the web addresses. %1", urlsHint.text) : urlsHint.text
                values: root.urls
                onValuesEdited: function (values) {
                    root.urls = values;
                }
            }
            Controls.Label {
                id: urlsHint
                Layout.fillWidth: true
                text: KI18n.i18n("The first address is kept as the main site.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }

    NotesEditor {
        id: notesEditor
        Layout.fillWidth: true
        placeholderText: KI18n.i18nc("@info:placeholder", "Add optional context to recognize this item.")
        accessibleName: KI18n.i18n("Login notes")
        onSourceTextEdited: value => root.notes = value
    }
}
