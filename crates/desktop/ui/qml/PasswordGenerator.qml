pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: generator

    required property real windowWidth
    property bool generating: false
    property string operationError: ""

    signal generateRequested(int length, bool lowercase, bool uppercase, bool digits, bool symbols, string excluded)

    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 30, windowWidth - Kirigami.Units.gridUnit * 4)
    modal: true
    title: qsTr("Generar contraseña")
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
        operationError = message || qsTr("No se pudo generar la contraseña. Revisa las opciones e inténtalo de nuevo.");
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Elige la longitud y las clases de caracteres. Se incluirá al menos un carácter de cada clase seleccionada.")
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
                text: qsTr("Longitud")
            }
            Controls.SpinBox {
                id: lengthField
                from: 4
                to: 1024
                value: 20
                editable: true
                Accessible.name: qsTr("Longitud de la contraseña")
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.CheckBox {
                id: lowercaseField
                text: qsTr("Minúsculas")
                checked: true
            }
            Controls.CheckBox {
                id: uppercaseField
                text: qsTr("Mayúsculas")
                checked: true
            }
            Controls.CheckBox {
                id: digitsField
                text: qsTr("Números")
                checked: true
            }
            Controls.CheckBox {
                id: symbolsField
                text: qsTr("Símbolos")
                checked: true
            }
        }

        Controls.Label {
            text: qsTr("Caracteres que quieres excluir")
        }
        Controls.TextField {
            id: excludedField
            Layout.fillWidth: true
            Accessible.name: qsTr("Caracteres excluidos")
            placeholderText: qsTr("Ej. 0O1l")
        }

        Controls.Label {
            Layout.fillWidth: true
            visible: generator.generating
            text: qsTr("Generando con el generador criptográfico del sistema…")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }

    footer: RowLayout {
        Controls.Button {
            text: qsTr("Cancelar")
            enabled: !generator.generating
            onClicked: generator.close()
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: generator.generating ? qsTr("Generando…") : qsTr("Generar")
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
