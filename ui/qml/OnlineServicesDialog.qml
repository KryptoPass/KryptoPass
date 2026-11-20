pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: dialog
    required property KeyStore keyStore
    property string errorMessage: ""
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 36, parent ? parent.width - 32 : 640)
    height: Math.min(implicitHeight, parent ? parent.height - 32 : 600)
    modal: true
    title: KI18n.i18nc("@title:window", "OpenPGP Online Services")
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
                text: KI18n.i18nc("@option:check", "Enable online lookups")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("Lookups happen only when you request them. The service receives the key's fingerprint or ID.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                text: KI18n.i18nc("@title:group", "Lookup services in the browser")
            }
            Controls.TextArea {
                id: services
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 6
                wrapMode: TextEdit.WrapAnywhere
                selectByMouse: true
                Accessible.name: KI18n.i18n("Online service URLs, one per line")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("One HTTP or HTTPS URL per line. Add, edit, remove or reorder the lines; the first one is the default. KGpg placeholders: $$FPR$$, $$ID16$$ and $$ID8$$; in lowercase: $$fpr$$, $$id16$$ and $$id8$$.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                text: KI18n.i18nc("@title:group", "Servers for fetching and refreshing keys")
            }
            Controls.TextArea {
                id: servers
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 4
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                Accessible.name: KI18n.i18n("Key servers, one per line")
            }
            Controls.Label {
                Layout.fillWidth: true
                text: KI18n.i18n("One HKP, HKPS, HTTP or HTTPS address per line. The first server is the default. If the list is empty, GnuPG's configuration is used.")
                wrapMode: Text.Wrap
            }
        }
    }
    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Cancel")
            onClicked: dialog.reject()
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Save Settings")
            icon.name: "document-save"
            onClicked: {
                const settings = {
                    enabled: active.checked,
                    services: dialog.lines(services.text),
                    keyservers: dialog.lines(servers.text)
                };
                if (dialog.keyStore.controller.saveOnlineSettings(JSON.stringify(settings)))
                    dialog.accept();
                else
                    dialog.errorMessage = dialog.keyStore.controller.operationError;
            }
        }
    }
}
