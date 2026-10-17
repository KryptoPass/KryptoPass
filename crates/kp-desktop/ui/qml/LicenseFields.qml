pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

SecretFields {
    id: root

    property string product: ""
    property string licenseKey: ""
    property string holder: ""
    property string email: ""
    property string version: ""
    property string purchased: ""
    property string expires: ""
    property string notes: ""

    spacing: Kirigami.Units.largeSpacing

    onProductChanged: edited()
    onLicenseKeyChanged: edited()
    onHolderChanged: edited()
    onEmailChanged: edited()
    onVersionChanged: edited()
    onPurchasedChanged: edited()
    onExpiresChanged: edited()
    onNotesChanged: edited()

    function loadFields(fields) {
        product = String(fields.product || "");
        licenseKey = String(fields.licenseKey || "");
        holder = String(fields.holder || "");
        email = String(fields.email || "");
        version = String(fields.version || "");
        purchased = String(fields.purchased || "");
        expires = String(fields.expires || "");
        notes = String(fields.notes || "");
    }

    function currentFields() {
        return {
            product: product,
            licenseKey: licenseKey,
            holder: holder,
            email: email,
            version: version,
            purchased: purchased,
            expires: expires,
            notes: notes
        };
    }

    function clearFields() { loadFields({}); }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Producto o servicio")
            value: root.product
            onValueEdited: function(value) { root.product = value; }
        }
        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Clave de licencia")
            value: root.licenseKey
            concealed: true
            onValueEdited: function(value) { root.licenseKey = value; }
        }
        GridLayout {
            Layout.fillWidth: true
            columns: width >= Kirigami.Units.gridUnit * 34 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Titular")
                value: root.holder
                onValueEdited: function(value) { root.holder = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Correo")
                value: root.email
                onValueEdited: function(value) { root.email = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Versión")
                value: root.version
                onValueEdited: function(value) { root.version = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Fecha de compra")
                value: root.purchased
                onValueEdited: function(value) { root.purchased = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Fecha de vencimiento")
                value: root.expires
                onValueEdited: function(value) { root.expires = value; }
            }
        }
        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Notas")
            value: root.notes
            multiline: true
            onValueEdited: function(value) { root.notes = value; }
        }
    }
}
