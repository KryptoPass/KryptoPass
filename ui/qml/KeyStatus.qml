pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

RowLayout {
    id: root
    property string code: ""
    property string text: ""
    property bool selected: false
    readonly property color foreground: selected ? Kirigami.Theme.highlightedTextColor : ["revoked", "invalid"].includes(code) ? Kirigami.Theme.negativeTextColor : ["expired", "disabled"].includes(code) ? Kirigami.Theme.neutralTextColor : Kirigami.Theme.textColor
    spacing: Kirigami.Units.smallSpacing

    Kirigami.Icon {
        visible: ["revoked", "invalid", "expired", "disabled"].includes(root.code)
        source: ["revoked", "invalid"].includes(root.code) ? "dialog-error" : "dialog-warning"
        color: root.foreground
        Layout.preferredWidth: Kirigami.Units.iconSizes.small
        Layout.preferredHeight: width
        Accessible.ignored: true
    }
    Controls.Label {
        Layout.fillWidth: true
        Layout.minimumWidth: 0
        text: root.text
        textFormat: Text.PlainText
        color: root.foreground
        elide: Text.ElideRight
    }
}
