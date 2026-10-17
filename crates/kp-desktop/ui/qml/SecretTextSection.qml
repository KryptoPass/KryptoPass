pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

Controls.Frame {
    id: root

    required property string heading
    required property string value
    required property string accessibleLabel
    property string documentKey: ""
    property string documentObjectName: "markdownSectionDocument"
    property string viewObjectName: "markdownSectionView"
    property string emptyText: ""
    property string emptyObjectName: ""
    property real minimumContentHeight: Kirigami.Units.gridUnit * 3
    readonly property bool effectivelyVisible: {
        let item = root;
        while (item) {
            if (!item.visible)
                return false;
            item = item.parent;
        }
        return true;
    }

    function clearDocument() {
        view.clearSelection();
        renderedDocument.clear();
    }

    function reloadDocument() {
        clearDocument();
        if (root.effectivelyVisible && root.value.length > 0)
            renderedDocument.load(root.value);
    }

    onValueChanged: reloadDocument()
    onDocumentKeyChanged: reloadDocument()
    onEffectivelyVisibleChanged: reloadDocument()
    Component.onCompleted: reloadDocument()
    Component.onDestruction: clearDocument()

    MarkdownDocument {
        id: renderedDocument
        objectName: root.documentObjectName
    }

    padding: Kirigami.Units.largeSpacing
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Controls.Label {
            Layout.fillWidth: true
            text: root.heading
            textFormat: Text.PlainText
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        Controls.Label {
            objectName: root.emptyObjectName
            Layout.fillWidth: true
            Layout.minimumHeight: root.minimumContentHeight
            visible: root.value.length === 0 && root.emptyText.length > 0
            text: root.emptyText
            textFormat: Text.PlainText
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
            verticalAlignment: Text.AlignVCenter
        }
        MarkdownView {
            id: view
            objectName: root.viewObjectName
            Layout.fillWidth: true
            Layout.minimumHeight: root.minimumContentHeight
            visible: root.value.length > 0
            padding: 0
            document: renderedDocument
            accessibleLabel: root.accessibleLabel
            onLinkRequested: link => Qt.openUrlExternally(link)
        }
    }
}
