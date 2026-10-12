pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

// Shared editor contract. Concrete forms own their draft fields and clearing.
ColumnLayout {
    signal edited
    signal generatePasswordRequested

    function loadFields(fields) {
        throw new Error("SecretFields.loadFields must be implemented by the form");
    }
    function currentFields() {
        throw new Error("SecretFields.currentFields must be implemented by the form");
    }
    function clearFields() {
        loadFields({});
    }
    function applyGeneratedPassword(value) {
    }

    Component.onDestruction: clearFields()
}
