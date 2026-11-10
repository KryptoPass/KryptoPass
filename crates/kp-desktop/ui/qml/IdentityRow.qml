pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

/// One line of a settings list or checklist: icon, title, optional help, state and actions.
RowLayout {
    id: root
    property string title: ""
    /// Theme icon shown before the title. Every row of a list sets one so titles line up.
    property string indicator: ""
    /// Health level that colors the state text: ok, pending, warning, problem.
    property string level: ""
    property string help: ""
    property string status: ""
    default property alias actions: actionRow.data
    Layout.fillWidth: true
    Layout.topMargin: 4
    Layout.bottomMargin: 4
    spacing: Kirigami.Units.largeSpacing

    Kirigami.Icon {
        source: root.indicator
        Layout.preferredWidth: 20
        Layout.preferredHeight: 20
        Layout.alignment: Qt.AlignVCenter
        opacity: root.indicator ? 1 : 0
        Accessible.ignored: true
    }
    Controls.Label {
        text: root.title
        textFormat: Text.PlainText
        elide: Text.ElideRight
        Layout.maximumWidth: root.width * 0.55
        Layout.alignment: Qt.AlignVCenter
    }
    HelpButton {
        visible: !!root.help
        help: root.help
        Layout.leftMargin: -Kirigami.Units.smallSpacing
    }
    Controls.Label {
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        Layout.alignment: Qt.AlignVCenter
        text: root.status
        textFormat: Text.PlainText
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignRight
        color: root.level === "ok" ? Kirigami.Theme.positiveTextColor : root.level === "problem" ? Kirigami.Theme.negativeTextColor : root.level === "pending" || root.level === "warning" ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
        opacity: root.level ? 1 : 0.75
    }
    RowLayout {
        id: actionRow
        Layout.alignment: Qt.AlignVCenter
        spacing: Kirigami.Units.smallSpacing
    }
}
