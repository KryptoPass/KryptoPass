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
        const actions = {};
        for (const action of root.spoilerActions)
            actions[action] = true;
        // Only the renderer's action anchors become inline controls. Matching
        // source text without an action remains ordinary selectable text.
        const markers = /████████████|\u2007\u2007\u2007/g;
        const plainText = textView.getText(0, textView.length);
        let marker;
        while ((marker = markers.exec(plainText)) !== null) {
            const position = marker.index;
            const length = marker[0].length;
            const fragment = textView.getFormattedText(position, position + length);
            const match = fragment.match(/href="(kryptopass-spoiler:[0-9]+:[0-9]+)"/);
            if (match && actions[match[1]])
                positions[match[1]] = {
                    position,
                    length
                };
        }
        root._spoilerPositions = positions;
        ++root._layoutRevision;
    }

    onSpoilerActionsChanged: Qt.callLater(root.updateSpoilerPositions)

    function markerRectangle(marker, layoutRevision) {
        // Layout changes invalidate positions without replacing the controls.
        if (!marker || marker.position < 0 || marker.position + marker.length > textView.length)
            return Qt.rect(0, 0, 0, 0);
        const start = textView.positionToRectangle(marker.position);
        let end = start;
        for (let offset = 1; offset <= marker.length; ++offset) {
            const next = textView.positionToRectangle(marker.position + offset);
            if (next.y !== start.y)
                break;
            end = next;
        }
        // The focusable control covers the first visual run. Qt's anchor still
        // handles clicks on a mask that continues on subsequent lines.
        return Qt.rect(Math.min(start.x, end.x), start.y, Math.max(1, Math.abs(end.x - start.x)), start.height);
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
                    readonly property var marker: root._spoilerPositions[modelData]
                    readonly property int markerPosition: marker ? marker.position : -1
                    readonly property rect markerBounds: root.markerRectangle(marker, root._layoutRevision)
                    objectName: "markdownSpoilerToggle" + spoilerButton.index
                    visible: markerPosition >= 0
                    x: markerBounds.x
                    y: markerBounds.y
                    width: markerBounds.width
                    height: markerBounds.height
                    padding: 0
                    display: Controls.AbstractButton.IconOnly
                    icon.name: revealed ? "view-hidden" : ""
                    icon.width: Math.min(Kirigami.Units.iconSizes.small, markerBounds.height)
                    icon.height: Math.min(Kirigami.Units.iconSizes.small, markerBounds.height)
                    activeFocusOnTab: true
                    contentItem: Kirigami.Icon {
                        source: spoilerButton.icon.name
                        color: Kirigami.Theme.textColor
                    }
                    background: Rectangle {
                        color: "transparent"
                        border.width: spoilerButton.visualFocus ? 1 : 0
                        border.color: Kirigami.Theme.focusColor
                        radius: Kirigami.Units.cornerRadius
                    }
                    Accessible.name: revealed ? qsTr("Ocultar spoiler %1").arg(spoilerButton.index + 1) : qsTr("Mostrar spoiler %1").arg(spoilerButton.index + 1)
                    Controls.ToolTip.visible: hovered || visualFocus
                    Controls.ToolTip.text: spoilerButton.Accessible.name
                    onClicked: root.document.activateLink(spoilerButton.modelData)
                    HoverHandler {
                        cursorShape: Qt.PointingHandCursor
                    }
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
