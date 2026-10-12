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
                text: qsTr("Destinatarios")
                color: Kirigami.Theme.disabledTextColor
            }
            Repeater {
                model: root.recipients
                delegate: Controls.Label {
                    required property string modelData
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: modelData
                    textFormat: Text.PlainText
                    font.family: Kirigami.Theme.fixedWidthFont.family
                    wrapMode: Text.WrapAnywhere
                }
            }
        }
    }
}
