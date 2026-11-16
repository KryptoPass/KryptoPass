pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    required property var selectedKey
    required property KeyStore keyStore

    readonly property var key: keyStore.entries.find(candidate => candidate.id.toUpperCase() === (selectedKey?.id || "").toUpperCase()) || selectedKey
    readonly property var wording: keyStore.wording
    readonly property bool hasSecret: !!key?.hasSecret
    readonly property bool needsOwnTrust: !!key?.needsOwnTrust
    readonly property bool canEncrypt: !!key?.canEncrypt
    readonly property bool valid: key?.statusCode === "valid"
    readonly property bool busy: keyStore.controller.busy || keyStore.controller.operationBusy
    readonly property string expiryText: wording.expiry(key?.subkeys?.[0]?.expiresAt)
    readonly property string summary: [key?.status, (key?.capabilities || []).join(", "), root.expiryText].filter(part => !!part).join(" · ")
    property bool importedFromFile: true
    property string pendingOperation: ""
    property string operationError: ""
    property string operationMessage: ""

    signal trustEstablished

    spacing: Kirigami.Units.largeSpacing

    function runOperation(action, extra) {
        if (!root.key?.id || root.busy)
            return;
        root.operationError = "";
        root.operationMessage = "";
        root.pendingOperation = action;
        root.keyStore.controller.clearOperationError();
        root.keyStore.controller.runOperation(JSON.stringify(Object.assign({
            action: action,
            fingerprint: root.key.id
        }, extra || {})));
    }

    Connections {
        target: root.keyStore.controller
        function onOperationFinished(fingerprint, message) {
            if (!root.pendingOperation)
                return;
            const completed = root.pendingOperation;
            root.pendingOperation = "";
            if (!root.operationError)
                root.operationMessage = message;
            if (completed === "markAsOwn")
                root.trustEstablished();
        }
        function onOperationFailed(message) {
            if (!root.pendingOperation)
                return;
            root.pendingOperation = "";
            root.operationError = message;
        }
    }

    TextEdit {
        id: fingerprintSource
        visible: false
        readOnly: true
        textFormat: TextEdit.PlainText
        text: root.key?.id || ""
    }

    Controls.Frame {
        Layout.fillWidth: true
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Icon {
                    source: "view-certificate"
                    implicitWidth: Kirigami.Units.iconSizes.huge
                    implicitHeight: Kirigami.Units.iconSizes.huge
                    Layout.alignment: Qt.AlignTop
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: 2
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: qsTr("Clave OpenPGP")
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.key?.pgpTitle || root.key?.title || qsTr("Sin identidad")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.key?.id ? root.wording.groupedFingerprint(root.key.id) : ""
                        textFormat: Text.PlainText
                        font.family: Kirigami.Theme.fixedWidthFont.family
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                }
                Controls.ToolButton {
                    text: qsTr("Copiar huella")
                    icon.name: "edit-copy"
                    display: Controls.AbstractButton.IconOnly
                    enabled: !!root.key?.id
                    Accessible.name: text
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: text
                    onClicked: {
                        fingerprintSource.selectAll();
                        fingerprintSource.copy();
                        fingerprintSource.deselect();
                    }
                }
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: root.summary.length > 0
                text: root.summary
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.operationError
        type: Kirigami.MessageType.Error
        text: root.operationError
    }
    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.operationMessage
        type: Kirigami.MessageType.Positive
        text: root.operationMessage
    }

    IdentityRow {
        visible: root.hasSecret && root.needsOwnTrust
        level: "warning"
        indicator: "security-low"
        title: qsTr("Confianza en GnuPG")
        help: root.wording.tips.ownerTrust
        status: qsTr("Sin confianza absoluta")
        Controls.Button {
            text: qsTr("Marcar como mía")
            highlighted: true
            enabled: !!root.key?.id && !root.busy
            onClicked: root.runOperation("markAsOwn")
        }
    }

    IdentityRow {
        visible: root.hasSecret && !root.needsOwnTrust
        level: "ok"
        indicator: "dialog-ok-apply"
        title: qsTr("Confianza en GnuPG")
        status: qsTr("Confianza absoluta")
    }

    IdentityRow {
        visible: root.hasSecret
        level: "ok"
        indicator: root.importedFromFile ? "document-import" : "view-certificate"
        title: qsTr("Origen de la clave")
        help: root.importedFromFile ? qsTr("La clave se importó desde un archivo local. Conserva ese archivo.") : qsTr("La clave ya estaba en tu llavero GnuPG local.")
        status: root.importedFromFile ? qsTr("Importada desde archivo") : qsTr("Detectada en tu llavero GnuPG")
    }

    IdentityRow {
        visible: !!root.key && !root.canEncrypt
        level: "warning"
        indicator: "security-medium"
        title: qsTr("Cifrado")
        status: qsTr("Sin subclave de cifrado vigente")
        Controls.Button {
            text: qsTr("Añadir subclave de cifrado")
            enabled: !!root.key?.id && !root.busy
            onClicked: root.runOperation("addSubkey", {
                usage: "encrypt",
                expiresDays: 0
            })
        }
    }

    IdentityRow {
        visible: !!root.keyStore.primaryKey && !!root.key && root.keyStore.primaryKey.toUpperCase() !== root.key.id.toUpperCase()
        level: "warning"
        indicator: "view-certificate"
        title: qsTr("Clave principal de la bóveda")
        status: qsTr("Esta bóveda usa otra clave")
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: !!root.key && !root.valid
        text: qsTr("Esta clave no es válida para usarla con la bóveda.")
        color: Kirigami.Theme.negativeTextColor
        wrapMode: Text.Wrap
    }
}
