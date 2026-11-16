pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    required property var selectedKey
    required property KeyStore keyStore

    property string title: qsTr("Revisa la clave")
    property string subtitle: qsTr("Verifica la huella digital y marca la clave como propia para usarla.")
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
            text: qsTr("Atrás")
            icon.name: "go-previous"
            enabled: !root.busy
            onClicked: root.backRequested()
        }
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            id: continueButton
            text: qsTr("Continuar")
            icon.name: "go-next"
            highlighted: true
            enabled: root.ready && !root.busy
            onClicked: root.accepted()
        }
    }
}
