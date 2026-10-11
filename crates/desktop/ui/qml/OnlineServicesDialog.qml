pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: dialog
    required property KeyStore keyStore
    property string errorMessage: ""
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 36, parent ? parent.width - 32 : 640)
    height: Math.min(implicitHeight, parent ? parent.height - 32 : 600)
    modal: true
    title: qsTr("Servicios online de OpenPGP")
    standardButtons: Controls.Dialog.NoButton
    function edit() {
        const settings = JSON.parse(keyStore.controller.onlineSettings);
        active.checked = settings.enabled;
        services.text = settings.services.join("\n");
        servers.text = settings.keyservers.join("\n");
        errorMessage = "";
        open();
    }
    function lines(value) {
        return value.split("\n").map(line => line.trim()).filter(line => !!line);
    }
    contentItem: Controls.ScrollView {
        id: scroll
        contentWidth: availableWidth
        implicitHeight: form.implicitHeight
        clip: true
        ColumnLayout {
            id: form
            width: scroll.availableWidth
            spacing: Kirigami.Units.largeSpacing
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.errorMessage
                type: Kirigami.MessageType.Error
                text: dialog.errorMessage
            }
            Controls.CheckBox {
                id: active
                text: qsTr("Activar consultas online")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Las consultas se realizan cuando las solicitas. El servicio recibe la huella o el ID de la clave.")
                wrapMode: Text.Wrap
            }
            Controls.Label { text: qsTr("Servicios de consulta en el navegador") }
            Controls.TextArea {
                id: services
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 6
                wrapMode: TextEdit.WrapAnywhere
                selectByMouse: true
                Accessible.name: qsTr("URLs de servicios online, una por línea")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Una URL HTTP o HTTPS por línea. Añade, edita, elimina o mueve las líneas; la primera es la predeterminada. Marcadores de KGpg: $$FPR$$, $$ID16$$ y $$ID8$$; en minúsculas: $$fpr$$, $$id16$$ y $$id8$$.")
                wrapMode: Text.Wrap
            }
            Controls.Label { text: qsTr("Servidores para obtener y actualizar claves") }
            Controls.TextArea {
                id: servers
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 4
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                Accessible.name: qsTr("Servidores de claves, uno por línea")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Una dirección HKP, HKPS, HTTP o HTTPS por línea. El primer servidor es el predeterminado. Si la lista está vacía, se usa la configuración de GnuPG.")
                wrapMode: Text.Wrap
            }
        }
    }
    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cancelar")
            onClicked: dialog.reject()
        }
        Controls.Button {
            text: qsTr("Guardar configuración")
            icon.name: "document-save"
            onClicked: {
                const settings = { enabled: active.checked, services: dialog.lines(services.text), keyservers: dialog.lines(servers.text) };
                if (dialog.keyStore.controller.saveOnlineSettings(JSON.stringify(settings)))
                    dialog.accept();
                else
                    dialog.errorMessage = dialog.keyStore.controller.operationError;
            }
        }
    }
}
