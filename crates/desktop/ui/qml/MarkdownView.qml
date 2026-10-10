pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

Controls.Control {
    id: root
    required property MarkdownDocument document
    property bool selectable: true
    readonly property var spoilerActions: document?.spoilerActions || []
    readonly property var revealedActions: document?.revealedActions || []
    readonly property string selectedText: textView.selectedText
    readonly property real contentHeight: body.implicitHeight
    signal linkRequested(string link)

    padding: Kirigami.Units.largeSpacing
    implicitHeight: body.implicitHeight + topPadding + bottomPadding
    implicitWidth: Kirigami.Units.gridUnit * 30

    function copySelection() {
        textView.copy();
    }
    function clearSelection() {
        textView.deselect();
    }

    function updateTheme() {
        if (document)
            document.setTheme(Kirigami.Theme.highlightColor.toString(), Kirigami.Theme.highlightedTextColor.toString(), Kirigami.Theme.linkColor.toString(), Kirigami.Theme.alternateBackgroundColor.toString(), Kirigami.Theme.disabledTextColor.toString());
    }
    onDocumentChanged: updateTheme()
    Component.onCompleted: updateTheme()
    Connections {
        target: Kirigami.Theme
        function onColorsChanged() {
            root.updateTheme();
        }
    }
    Connections {
        target: root.document
        function onHtmlChanged() {
            textView.deselect();
        }
        function onLinkRequested(link) {
            root.linkRequested(link);
        }
    }

    background: Rectangle {
        color: Kirigami.Theme.backgroundColor
        border.width: textView.activeFocus ? 1 : 0
        border.color: Kirigami.Theme.focusColor
    }
    contentItem: ColumnLayout {
        id: body
        spacing: Kirigami.Units.largeSpacing
        TextEdit {
            id: textView
            objectName: "markdownText"
            Layout.fillWidth: true
            Layout.preferredHeight: contentHeight
            text: root.document ? root.document.html : ""
            textFormat: TextEdit.RichText
            readOnly: true
            wrapMode: TextEdit.Wrap
            selectByMouse: root.selectable
            selectByKeyboard: root.selectable
            persistentSelection: true
            activeFocusOnTab: true
            color: Kirigami.Theme.textColor
            selectionColor: Kirigami.Theme.highlightColor
            selectedTextColor: Kirigami.Theme.highlightedTextColor
            font: root.font
            Accessible.name: qsTr("Documento Markdown")
            onLinkActivated: link => root.document.activateLink(link)
            HoverHandler {
                cursorShape: textView.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: !!root.document?.error
            text: root.document?.error || ""
            color: Kirigami.Theme.negativeTextColor
            wrapMode: Text.Wrap
        }
        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            visible: root.spoilerActions.length > 0
            Repeater {
                model: root.spoilerActions
                delegate: Controls.Button {
                    required property string modelData
                    required property int index
                    readonly property bool revealed: root.revealedActions.includes(modelData)
                    text: revealed ? qsTr("Ocultar spoiler %1").arg(index + 1) : qsTr("Mostrar spoiler %1").arg(index + 1)
                    icon.name: revealed ? "view-hidden" : "view-visible"
                    onClicked: root.document.activateLink(modelData)
                }
            }
        }
    }
}
