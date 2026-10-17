pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var detail: null
    property var recipients: []
    property var recipientKeys: []
    property bool revealed: false
    property var versionDetail: null
    readonly property SecretContentView contentView: contentLoader.item as SecretContentView

    signal revealToggled
    signal copyUsernameRequested
    signal copyPasswordRequested
    signal copyTotpRequested(bool next)
    signal copyFieldRequested(string field)
    signal revealFieldRequested(string field)
    signal qrRequested
    signal versionOpenRequested(int index)
    signal versionCloseRequested
    signal versionCopyRequested(string field)
    signal activityExportRequested

    spacing: Kirigami.Units.largeSpacing

    Loader {
        id: contentLoader
        Layout.fillWidth: true
        sourceComponent: EntryTypes.find(root.detail?.content?.kind)?.detail || null
    }

    Binding {
        target: root.contentView
        property: "fields"
        value: root.detail?.content?.fields || null
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "entryId"
        value: root.detail?.id || ""
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "revealed"
        value: root.revealed
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "versionDetail"
        value: root.versionDetail
        when: !!root.contentView
    }

    Connections {
        target: root.contentView
        function onRevealToggled() {
            root.revealToggled();
        }
        function onCopyUsernameRequested() {
            root.copyUsernameRequested();
        }
        function onCopyPasswordRequested() {
            root.copyPasswordRequested();
        }
        function onCopyTotpRequested(next) {
            root.copyTotpRequested(next);
        }
        function onCopyFieldRequested(field) {
            root.copyFieldRequested(field);
        }
        function onRevealFieldRequested(field) {
            root.revealFieldRequested(field);
        }
        function onQrRequested() {
            root.qrRequested();
        }
        function onVersionOpenRequested(index) {
            root.versionOpenRequested(index);
        }
        function onVersionCloseRequested() {
            root.versionCloseRequested();
        }
        function onVersionCopyRequested(field) {
            root.versionCopyRequested(field);
        }
        function onActivityExportRequested() {
            root.activityExportRequested();
        }
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.detail && !EntryTypes.find(root.detail.content?.kind)
        type: Kirigami.MessageType.Error
        text: qsTr("No se puede mostrar este tipo de elemento.")
    }

    Controls.Frame {
        Layout.fillWidth: true
        visible: root.recipients.length > 0
        padding: Kirigami.Units.largeSpacing
        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTr("Quién tiene acceso")
                color: Kirigami.Theme.disabledTextColor
            }
            Repeater {
                model: root.recipients
                delegate: ColumnLayout {
                    id: recipientRow
                    required property string modelData
                    readonly property var recipientKey: root.recipientKeys.find(key => key.id.toUpperCase() === recipientRow.modelData.toUpperCase()) || null
                    readonly property string recipientName: recipientKey?.contactName || recipientKey?.title || recipientKey?.email || qsTr("Clave no disponible")
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        ColumnLayout {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            spacing: 2
                            Controls.Label {
                                Layout.fillWidth: true
                                text: recipientRow.recipientName
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                visible: !!recipientRow.recipientKey?.email
                                text: recipientRow.recipientKey?.email || ""
                                textFormat: Text.PlainText
                                color: Kirigami.Theme.disabledTextColor
                                wrapMode: Text.WrapAnywhere
                            }
                        }
                        Controls.ToolButton {
                            id: recipientDetails
                            text: checked ? qsTr("Ocultar detalles de la clave") : qsTr("Ver detalles de la clave")
                            icon.name: checked ? "arrow-up" : "arrow-down"
                            display: Controls.AbstractButton.IconOnly
                            checkable: true
                            Accessible.name: qsTr("%1: %2").arg(text).arg(recipientRow.recipientName)
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: recipientDetails.checked
                        text: qsTr("Huella OpenPGP: %1").arg(recipientRow.modelData)
                        textFormat: Text.PlainText
                        font.family: Kirigami.Theme.fixedWidthFont.family
                        wrapMode: Text.WrapAnywhere
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }
        }
    }
}
