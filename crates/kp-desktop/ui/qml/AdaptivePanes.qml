pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.SplitView {
    id: root
    readonly property bool compact: width < Kirigami.Units.gridUnit * 60
    property int currentPane: 1
    property alias navigationContent: navigationHost.data
    property alias headerContent: headerHost.data
    property alias listContent: listHost.data
    property alias detailContent: detailHost.data
    property alias footerContent: footerHost.data
    readonly property alias navigationVisible: navigationHost.visible
    readonly property alias listVisible: listHost.visible
    readonly property alias detailVisible: detailHost.visible

    function showPane(index: int) {
        currentPane = index;
        const pane = index === 0 ? navigationHost : index === 1 ? listHost : detailHost;
        Qt.callLater(function () {
            pane.forceActiveFocus(Qt.ShortcutFocusReason);
        });
    }

    orientation: Qt.Horizontal
    handle: SplitterHandle {}

    FocusScope {
        id: navigationHost
        visible: !root.compact || root.currentPane === 0
        Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 14
        Controls.SplitView.minimumWidth: root.compact ? 0 : Kirigami.Units.gridUnit * 11
        Controls.SplitView.fillWidth: root.compact
    }
    Controls.Pane {
        visible: !root.compact || root.currentPane !== 0
        Controls.SplitView.fillWidth: true
        Controls.SplitView.minimumWidth: root.compact ? 0 : Kirigami.Units.gridUnit * 36
        padding: 0
        ColumnLayout {
            anchors.fill: parent
            spacing: 0
            Item {
                id: headerHost
                Layout.fillWidth: true
                implicitHeight: childrenRect.height
            }
            Controls.SplitView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: Qt.Horizontal
                handle: SplitterHandle {}
                FocusScope {
                    id: listHost
                    visible: !root.compact || root.currentPane === 1
                    Controls.SplitView.preferredWidth: Kirigami.Units.gridUnit * 18
                    Controls.SplitView.minimumWidth: root.compact ? 0 : Kirigami.Units.gridUnit * 13
                    Controls.SplitView.fillWidth: root.compact
                }
                FocusScope {
                    id: detailHost
                    visible: !root.compact || root.currentPane === 2
                    Controls.SplitView.fillWidth: true
                    Controls.SplitView.minimumWidth: root.compact ? 0 : Kirigami.Units.gridUnit * 21
                }
            }
            Item {
                id: footerHost
                Layout.fillWidth: true
                implicitHeight: childrenRect.height
            }
        }
    }
}
