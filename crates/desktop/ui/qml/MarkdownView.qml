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
    property string accessibleLabel: qsTr("Documento Markdown")
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
            Accessible.name: root.accessibleLabel
            onLinkActivated: link => root.document.activateLink(link)
            HoverHandler {
                cursorShape: textView.hoveredLink
                    ? Qt.PointingHandCursor
                    : Qt.IBeamCursor
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: !!root.document?.error
            text: root.document?.error || ""
            color: Kirigami.Theme.negativeTextColor
            wrapMode: Text.Wrap
        }
        ColumnLayout {
            Layout.fillWidth: true
            visible: root.spoilerActions.length > 0
            spacing: Kirigami.Units.smallSpacing

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Spoilers")
                color: Kirigami.Theme.disabledTextColor
            }
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Repeater {
                    model: root.spoilerActions
                    delegate: Controls.ToolButton {
                        id: spoilerButton
                        required property int index
                        required property string modelData
                        readonly property bool revealed: root.revealedActions.indexOf(spoilerButton.modelData) >= 0
                        objectName: "markdownSpoilerToggle" + spoilerButton.index
                        display: Controls.AbstractButton.IconOnly
                        icon.name: revealed ? "view-visible" : "view-hidden"
                        activeFocusOnTab: true
                        Accessible.name: revealed
                            ? qsTr("Ocultar spoiler %1").arg(spoilerButton.index + 1)
                            : qsTr("Mostrar spoiler %1").arg(spoilerButton.index + 1)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: revealed
                            ? qsTr("Ocultar spoiler %1").arg(spoilerButton.index + 1)
                            : qsTr("Mostrar spoiler %1").arg(spoilerButton.index + 1)
                        onClicked: root.document.activateLink(spoilerButton.modelData)
                    }
                }
            }
        }
    }
}
