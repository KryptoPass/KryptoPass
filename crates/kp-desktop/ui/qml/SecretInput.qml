pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    required property string label
    property string value: ""
    property string placeholderText: ""
    property string hint: ""
    property bool multiline: false
    property bool concealed: false
    property int minimumRows: 3
    property bool secretVisible: false
    property alias validator: singleLineField.validator
    property int inputMethodHints: Qt.ImhNone
    readonly property int sensitiveInputMethodHints: inputMethodHints | Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhNoAutoUppercase
    property alias maximumLength: singleLineField.maximumLength

    signal valueEdited(string value)

    spacing: Kirigami.Units.smallSpacing

    function mask(value) {
        return String(value || "").replace(/[^\r\n]/g, "•");
    }

    Controls.Label {
        Layout.fillWidth: true
        text: root.label
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Controls.TextField {
            id: singleLineField
            visible: !root.multiline
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            text: root.multiline ? "" : root.value
            placeholderText: root.placeholderText
            inputMethodHints: root.sensitiveInputMethodHints
            passwordMaskDelay: 0
            echoMode: root.concealed && !root.secretVisible ? TextInput.Password : TextInput.Normal
            Accessible.name: root.label
            onTextEdited: root.valueEdited(text)
        }

        Controls.TextArea {
            id: multiLineField
            visible: root.multiline
            Layout.fillWidth: true
            Layout.minimumWidth: 0
            Layout.preferredHeight: Math.max(implicitHeight, root.minimumRows * Kirigami.Units.gridUnit)
            text: !root.multiline ? "" : root.concealed && !root.secretVisible ? root.mask(root.value) : root.value
            placeholderText: root.placeholderText
            inputMethodHints: root.sensitiveInputMethodHints
            readOnly: root.concealed && !root.secretVisible
            wrapMode: TextEdit.Wrap
            selectByMouse: true
            textFormat: TextEdit.PlainText
            Accessible.name: root.label
            onTextEdited: {
                if (root.concealed && !root.secretVisible)
                    return;
                root.valueEdited(text);
            }
        }

        Controls.ToolButton {
            visible: root.concealed
            text: root.secretVisible
                ? qsTr("Ocultar %1").arg(root.label)
                : qsTr("Mostrar %1").arg(root.label)
            icon.name: root.secretVisible ? "view-hidden" : "view-visible"
            display: Controls.AbstractButton.IconOnly
            checkable: true
            checked: root.secretVisible
            Accessible.name: text
            onClicked: root.secretVisible = !root.secretVisible
            Controls.ToolTip.visible: hovered || visualFocus
            Controls.ToolTip.text: text
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: root.hint.length > 0
        text: root.hint
        textFormat: Text.PlainText
        color: Kirigami.Theme.disabledTextColor
        wrapMode: Text.Wrap
    }

    onConcealedChanged: secretVisible = false
}
