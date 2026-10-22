pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Covers the vault while it is locked so names, folders and counts stay hidden.
Rectangle {
    id: root

    property bool unlockEnabled: true
    property bool unlocking: false
    property string errorMessage: ""
    signal unlockRequested

    color: Kirigami.Theme.backgroundColor
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    onVisibleChanged: {
        if (visible)
            Qt.callLater(unlockButton.forceActiveFocus);
    }

    // Swallows pointer input so nothing behind the cover reacts.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 26)
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.alignment: Qt.AlignHCenter
            source: "object-locked"
            implicitWidth: Kirigami.Units.iconSizes.enormous
            implicitHeight: Kirigami.Units.iconSizes.enormous
            color: Kirigami.Theme.disabledTextColor
        }
        Kirigami.Heading {
            Layout.fillWidth: true
            level: 1
            text: qsTr("%1 está bloqueado").arg(Application.displayName)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: root.errorMessage.length > 0
            type: Kirigami.MessageType.Warning
            text: root.errorMessage
        }
        Controls.Button {
            id: unlockButton
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: root.unlocking ? qsTr("Desbloqueando…") : qsTr("Desbloquear")
            icon.name: "object-unlocked"
            highlighted: true
            enabled: root.unlockEnabled && !root.unlocking
            onClicked: root.unlockRequested()
            Keys.onReturnPressed: root.unlockRequested()
            Keys.onEnterPressed: root.unlockRequested()
        }
    }
}
