pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

/// Where the public key is published, and the third-party certifications it carries.
Controls.ScrollView {
    id: root
    required property var selectedKey
    required property KeyStore keyStore
    required property KeyOperationDialog operationDialog
    readonly property IdentityWording wording: keyStore.wording
    readonly property bool available: !keyStore.controller.busy
    readonly property bool online: !!keyStore.onlineSettings.enabled
    readonly property var servers: Array.from(new Set([...(keyStore.onlineSettings.keyservers || []), ...Object.keys(selectedKey?.meta.publications || {})]))
    readonly property var signatures: (selectedKey?.signatures || []).slice().sort((a, b) => a.signer.localeCompare(b.signer) || a.keyId.localeCompare(b.keyId))
    contentWidth: availableWidth
    clip: true

    OnlineServicesDialog {
        id: settings
        keyStore: root.keyStore
    }
    Controls.Dialog {
        id: publishConfirm
        property string server: ""
        property string fingerprint: ""
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.largeSpacing * 2, Kirigami.Units.gridUnit * 28)
        modal: true
        title: qsTr("Publicar clave pública")
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onAccepted: root.keyStore.controller.publishIdentity(fingerprint, server, true)
        contentItem: Controls.Label {
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: qsTr("Se enviará la clave pública, con sus UID y certificaciones, a %1. Lo publicado no siempre se puede retirar.").arg(publishConfirm.server)
        }
    }
    Controls.Menu {
        id: webMenu
        Instantiator {
            model: root.keyStore.onlineSettings.services
            delegate: Controls.MenuItem {
                required property string modelData
                required property int index
                text: modelData.split("/")[2] || modelData
                onTriggered: root.keyStore.controller.openOnlineService(index, root.selectedKey.id)
            }
            onObjectAdded: (index, object) => webMenu.insertItem(index, object)
            onObjectRemoved: (index, object) => webMenu.removeItem(object)
        }
    }
    ColumnLayout {
        // Rows stay readable on wide windows: controls never drift far from their label.
        width: Math.min(root.availableWidth - 48, 820)
        x: 24
        spacing: 12
        Item {
            implicitHeight: 8
        }
        RowLayout {
            Layout.fillWidth: true
            Kirigami.Heading {
                level: 4
                text: qsTr("Servidores de claves")
            }
            HelpButton {
                help: root.wording.tips.keyservers
            }
            Controls.Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: root.online ? "" : qsTr("Servicios online desactivados")
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
            }
            Controls.Button {
                id: webButton
                text: qsTr("Ver en la web")
                icon.name: "internet-services"
                visible: (root.keyStore.onlineSettings.services || []).length > 0
                enabled: root.online
                onClicked: webMenu.popup(webButton, 0, webButton.height)
            }
            Controls.Button {
                text: qsTr("Actualizar desde servidor…")
                icon.name: "view-refresh"
                enabled: root.available && root.online
                onClicked: root.operationDialog.openFor("refreshExternal", root.selectedKey.id)
            }
            Controls.Button {
                text: qsTr("Configurar servidores…")
                icon.name: "configure"
                enabled: root.available
                onClicked: settings.edit()
            }
        }
        Repeater {
            model: root.servers
            delegate: IdentityRow {
                id: serverRow
                required property string modelData
                readonly property var publication: root.selectedKey?.meta.publications?.[modelData] || {}
                readonly property bool usable: root.available && root.online && root.keyStore.onlineSettings.keyservers.includes(modelData)
                indicator: "network-server"
                level: publication.state === "present" ? "ok" : publication.state === "missing" || publication.state === "error" ? "pending" : ""
                title: modelData.replace(/^[a-z]+:\/\//, "")
                status: root.wording.publication(publication)
                help: root.wording.publicationDetail(publication)
                Controls.Button {
                    text: qsTr("Comprobar")
                    enabled: serverRow.usable
                    onClicked: root.keyStore.controller.publishIdentity(root.selectedKey.id, serverRow.modelData, false)
                }
                Controls.Button {
                    visible: !!root.selectedKey?.own
                    text: serverRow.publication.uploadedAt ? qsTr("Actualizar…") : qsTr("Publicar…")
                    enabled: serverRow.usable
                    onClicked: {
                        publishConfirm.server = serverRow.modelData;
                        publishConfirm.fingerprint = root.selectedKey.id;
                        publishConfirm.open();
                    }
                }
            }
        }
        Controls.Label {
            visible: !root.servers.length
            text: qsTr("No hay servidores configurados.")
            color: Kirigami.Theme.disabledTextColor
        }
        Kirigami.Separator {
            Layout.fillWidth: true
            Layout.topMargin: 8
            Layout.bottomMargin: 8
        }
        RowLayout {
            Layout.fillWidth: true
            Kirigami.Heading {
                level: 4
                text: qsTr("Certificaciones de terceros (%1)").arg(root.signatures.length)
            }
            HelpButton {
                help: root.wording.tips.certifications
            }
            Item {
                Layout.fillWidth: true
            }
        }
        Repeater {
            model: root.signatures
            delegate: RowLayout {
                id: signatureRow
                required property var modelData
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: signatureRow.modelData.signer || qsTr("Firmante desconocido")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    Controls.ToolTip.visible: signerHover.hovered
                    Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
                    Controls.ToolTip.text: qsTr("UID firmado: %1").arg(signatureRow.modelData.identity)
                    HoverHandler {
                        id: signerHover
                    }
                }
                Controls.Label {
                    text: signatureRow.modelData.keyId
                    textFormat: Text.PlainText
                    font: Kirigami.Theme.fixedWidthFont
                    opacity: 0.75
                }
                Controls.Label {
                    text: signatureRow.modelData.status
                    textFormat: Text.PlainText
                    opacity: 0.75
                }
            }
        }
        Controls.Label {
            visible: !root.signatures.length
            text: qsTr("Sin certificaciones de terceros.")
            color: Kirigami.Theme.disabledTextColor
        }
        Item {
            implicitHeight: 8
        }
    }
}
