pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: root

    required property var selectedKey
    required property KeyStore keyStore

    property string title: KI18n.i18nc("@title", "Review the key")
    property string subtitle: KI18n.i18n("Verify the fingerprint and mark the key as your own to use it.")
    property bool importedFromFile: true

    readonly property bool ready: !!selectedKey && selectedKey.hasSecret && selectedKey.statusCode === "valid" && selectedKey.canEncrypt && !selectedKey.needsOwnTrust
    readonly property bool busy: keyStore.controller.busy || keyStore.controller.operationBusy

    signal backRequested
    signal accepted

    spacing: Kirigami.Units.largeSpacing

    Kirigami.Heading {
        Layout.fillWidth: true
        level: 1
        text: root.title
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
    }
    Controls.Label {
        Layout.fillWidth: true
        horizontalAlignment: Text.AlignHCenter
        text: root.subtitle
        color: Kirigami.Theme.disabledTextColor
        wrapMode: Text.Wrap
    }
    KeyImportReview {
        id: review
        Layout.fillWidth: true
        selectedKey: root.selectedKey
        keyStore: root.keyStore
        importedFromFile: root.importedFromFile
        onTrustEstablished: Qt.callLater(function () {
            if (root.ready)
                continueButton.forceActiveFocus();
        })
    }
    RowLayout {
        Layout.fillWidth: true
        Layout.topMargin: Kirigami.Units.largeSpacing

        Controls.Button {
            text: KI18n.i18nc("@action:button", "Back")
            icon.name: "go-previous"
            enabled: !root.busy
            onClicked: root.backRequested()
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            id: continueButton
            text: KI18n.i18nc("@action:button", "Continue")
            icon.name: "go-next"
            highlighted: true
            enabled: root.ready && !root.busy
            onClicked: root.accepted()
        }
    }
}
