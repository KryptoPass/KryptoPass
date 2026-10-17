pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string holder: ""
    property string number: ""
    property int expiryMonth: 0
    property int expiryYear: 0
    property string cvv: ""
    property string pin: ""
    property string notes: ""

    spacing: Kirigami.Units.largeSpacing

    onHolderChanged: edited()
    onNumberChanged: edited()
    onExpiryMonthChanged: edited()
    onExpiryYearChanged: edited()
    onCvvChanged: edited()
    onPinChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        holder = String(fields.holder || "");
        number = String(fields.number || "");
        expiryMonth = Number.isInteger(Number(fields.expiryMonth)) ? Number(fields.expiryMonth) : 0;
        expiryYear = Number.isInteger(Number(fields.expiryYear)) ? Number(fields.expiryYear) : 0;
        cvv = String(fields.cvv || "");
        pin = String(fields.pin || "");
        notes = String(fields.notes || "");
        notesEditor.resetSource(notes);
    }

    function currentFields() {
        return {
            holder: holder,
            number: number,
            expiryMonth: expiryMonth,
            expiryYear: expiryYear,
            cvv: cvv,
            pin: pin,
            notes: notes
        };
    }

    function clearFields() { loadFields({}); }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Titular")
            value: root.holder
            onValueEdited: function(value) { root.holder = value; }
        }
        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Número de tarjeta")
            value: root.number
            concealed: true
            maximumLength: 64
            inputMethodHints: Qt.ImhDigitsOnly
            validator: RegularExpressionValidator { regularExpression: /[0-9 -]*/ }
            onValueEdited: function(value) { root.number = value; }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: width >= Kirigami.Units.gridUnit * 34 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing

            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Mes de vencimiento")
                value: root.expiryMonth > 0 ? String(root.expiryMonth) : ""
                placeholderText: qsTr("1–12")
                inputMethodHints: Qt.ImhDigitsOnly
                validator: IntValidator { bottom: 1; top: 12 }
                onValueEdited: function(value) {
                    root.expiryMonth = value.length ? Number(value) : 0;
                }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Año de vencimiento")
                value: root.expiryYear > 0 ? String(root.expiryYear) : ""
                placeholderText: qsTr("AAAA")
                inputMethodHints: Qt.ImhDigitsOnly
                validator: IntValidator { bottom: 1900; top: 9999 }
                hint: qsTr("Escribe el año con cuatro cifras.")
                onValueEdited: function(value) {
                    root.expiryYear = value.length ? Number(value) : 0;
                }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("CVV")
                value: root.cvv
                concealed: true
                maximumLength: 4
                inputMethodHints: Qt.ImhDigitsOnly
                validator: RegularExpressionValidator { regularExpression: /[0-9]{0,4}/ }
                hint: qsTr("Usa 3 o 4 cifras.")
                onValueEdited: function(value) { root.cvv = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("PIN")
                value: root.pin
                concealed: true
                maximumLength: 64
                inputMethodHints: Qt.ImhDigitsOnly
                validator: RegularExpressionValidator { regularExpression: /[0-9]{0,64}/ }
                onValueEdited: function(value) { root.pin = value; }
            }
        }
        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
