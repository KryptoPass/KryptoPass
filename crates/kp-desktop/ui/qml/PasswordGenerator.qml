pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: generator

    required property real windowWidth
    property bool generating: false
    property string operationError: ""

    signal generateRequested(int length, bool lowercase, bool uppercase, bool digits, bool symbols, string excluded)

    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 30, windowWidth - Kirigami.Units.gridUnit * 4)
    modal: true
    title: KI18n.i18nc("@title:window", "Generate Password")
    closePolicy: generating ? Controls.Popup.NoAutoClose : Controls.Popup.CloseOnEscape | Controls.Popup.CloseOnPressOutside

    function openGenerator() {
        if (generating)
            return;
        operationError = "";
        open();
    }

    function acceptGenerated() {
        generating = false;
        operationError = "";
        close();
    }

    function rejectGeneration(message) {
        generating = false;
        operationError = message || KI18n.i18n("The password could not be generated. Check the options and try again.");
    }

    function clearSensitiveState() {
        if (generating)
            return;
        operationError = "";
        excludedField.clear();
        close();
    }

    onClosed: {
        operationError = "";
        if (!generating)
            excludedField.clear();
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: KI18n.i18n("Choose the length and character classes. At least one character from each selected class will be included.")
            wrapMode: Text.Wrap
        }

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: !!generator.operationError
            type: Kirigami.MessageType.Error
            text: generator.operationError
        }

        RowLayout {
            Layout.fillWidth: true
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18nc("@label:spinbox", "Length")
            }
            Controls.SpinBox {
                id: lengthField
                from: 4
                to: 1024
                value: 20
                editable: true
                Accessible.name: KI18n.i18n("Password length")
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.CheckBox {
                id: lowercaseField
                text: KI18n.i18nc("@option:check", "Lowercase")
                checked: true
            }
            Controls.CheckBox {
                id: uppercaseField
                text: KI18n.i18nc("@option:check", "Uppercase")
                checked: true
            }
            Controls.CheckBox {
                id: digitsField
                text: KI18n.i18nc("@option:check", "Digits")
                checked: true
            }
            Controls.CheckBox {
                id: symbolsField
                text: KI18n.i18nc("@option:check", "Symbols")
                checked: true
            }
        }

        Controls.Label {
            text: KI18n.i18nc("@label:textbox", "Characters to exclude")
        }
        Controls.TextField {
            id: excludedField
            Layout.fillWidth: true
            Accessible.name: KI18n.i18n("Excluded characters")
            placeholderText: KI18n.i18nc("@info:placeholder", "E.g. 0O1l")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: generator.generating
            text: KI18n.i18n("Generating with the system's cryptographic generator…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }

    footer: RowLayout {
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Cancel")
            enabled: !generator.generating
            onClicked: generator.close()
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: generator.generating ? KI18n.i18n("Generating…") : KI18n.i18nc("@action:button", "Generate")
            icon.name: "tools-wizard"
            enabled: !generator.generating && (lowercaseField.checked || uppercaseField.checked || digitsField.checked || symbolsField.checked)
            onClicked: {
                generator.generating = true;
                generator.operationError = "";
                generator.generateRequested(lengthField.value, lowercaseField.checked, uppercaseField.checked, digitsField.checked, symbolsField.checked, excludedField.text);
            }
        }
    }
}
