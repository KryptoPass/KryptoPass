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
    property var _spoilerPositions: ({})
    property int _layoutRevision: 0
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

    function updateSpoilerPositions() {
        const positions = {};
        // Only the renderer's action anchors reserve space for inline controls.
        // Source text with the same spaces must remain ordinary selectable text.
        const marker = "\u2007\u2007\u2007";
        const plainText = textView.getText(0, textView.length);
        for (let position = plainText.indexOf(marker); position >= 0; position = plainText.indexOf(marker, position + marker.length)) {
            const fragment = textView.getFormattedText(position, position + marker.length);
            const match = fragment.match(/href="(kryptopass-spoiler:[0-9]+:[0-9]+)"/);
            if (match && root.spoilerActions.indexOf(match[1]) >= 0)
                positions[match[1]] = position;
        }
        root._spoilerPositions = positions;
        ++root._layoutRevision;
    }

    onSpoilerActionsChanged: Qt.callLater(root.updateSpoilerPositions)

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
            onTextChanged: Qt.callLater(root.updateSpoilerPositions)
            onWidthChanged: ++root._layoutRevision
            onContentHeightChanged: ++root._layoutRevision
            onFontChanged: ++root._layoutRevision
            HoverHandler {
                cursorShape: textView.hoveredLink ? Qt.PointingHandCursor : Qt.IBeamCursor
            }
            Repeater {
                model: root.spoilerActions
                delegate: Controls.ToolButton {
                    id: spoilerButton
                    required property int index
                    required property string modelData
                    readonly property bool revealed: root.revealedActions.indexOf(spoilerButton.modelData) >= 0
                    readonly property int markerPosition: root._spoilerPositions[modelData] ?? -1
                    readonly property rect markerStart: {
                        const revision = root._layoutRevision;
                        return textView.positionToRectangle(markerPosition);
                    }
                    readonly property rect markerEnd: {
                        const revision = root._layoutRevision;
                        return textView.positionToRectangle(markerPosition + 3);
                    }
                    objectName: "markdownSpoilerToggle" + spoilerButton.index
                    visible: markerPosition >= 0
                    x: Math.min(markerStart.x, markerEnd.x)
                    y: markerStart.y
                    width: Math.abs(markerEnd.x - markerStart.x)
                    height: markerStart.height
                    padding: 0
                    display: Controls.AbstractButton.IconOnly
                    icon.name: revealed ? "view-hidden" : "view-visible"
                    icon.width: Kirigami.Units.iconSizes.small
                    icon.height: Kirigami.Units.iconSizes.small
                    activeFocusOnTab: true
                    Accessible.name: revealed ? qsTr("Ocultar spoiler %1").arg(spoilerButton.index + 1) : qsTr("Mostrar spoiler %1").arg(spoilerButton.index + 1)
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: Accessible.name
                    onClicked: root.document.activateLink(spoilerButton.modelData)
                }
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: !!root.document?.error
            text: root.document?.error || ""
            color: Kirigami.Theme.negativeTextColor
            wrapMode: Text.Wrap
        }
    }
}
