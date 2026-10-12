pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

// Shared editor contract. Concrete forms own their draft fields and clearing.
ColumnLayout {
    signal edited
    signal generatePasswordRequested
    signal prepareSshKeyRequested(url fileUrl, string passphrase)
    signal prepareTotpQrRequested(url fileUrl)
    signal externalModalRequested(bool opened)

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
    function applyPreparedSshKey(json) {
    }
    function applyPreparedTotp(json) {
    }

    Component.onDestruction: clearFields()
}
