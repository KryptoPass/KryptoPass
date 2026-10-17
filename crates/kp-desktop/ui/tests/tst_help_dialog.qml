pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtTest
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "HelpDialog"
    width: Kirigami.Units.gridUnit * 30
    height: Kirigami.Units.gridUnit * 30
    visible: true
    when: windowShown

    Component {
        id: windowComponent

        Controls.ApplicationWindow {
            property alias helpDialog: help

            width: Kirigami.Units.gridUnit * 22
            height: Kirigami.Units.gridUnit * 25
            visible: true

            HelpDialog {
                id: help
            }
        }
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function openHelp(fontScale) {
        const window = createTemporaryObject(windowComponent, testCase, {
            "font.pointSize": Kirigami.Theme.defaultFont.pointSize * fontScale
        });
        verify(window !== null);
        tryCompare(window, "visible", true);
        window.helpDialog.open();
        tryCompare(window.helpDialog, "opened", true);
        tryVerify(() => window.helpDialog.contentItem.width > 0 && window.helpDialog.contentItem.height > 0);
        verify(waitForPolish(window));
        return window;
    }

    function readableLabels(item) {
        const labels = [];
        if (typeof item.text === "string" && item.text.length > 0 && typeof item.wrapMode === "number")
            labels.push(item);
        for (const child of item.children || [])
            labels.push(...readableLabels(child));
        return labels;
    }

    function verifyFitsWindow(item, window, label) {
        verify(item !== null && item.visible, label + " should be visible");
        verify(item.width > 0 && item.height > 0, label + " should have positive dimensions");
        const topLeft = item.mapToItem(window.contentItem, 0, 0);
        const bottomRight = item.mapToItem(window.contentItem, item.width, item.height);
        verify(topLeft.x >= -1 && topLeft.y >= -1, label + " should start inside the window");
        verify(bottomRight.x <= window.width + 1 && bottomRight.y <= window.height + 1, label + " should fit inside " + window.width + "x" + window.height + "; actual bounds " + topLeft.x + "," + topLeft.y + " to " + bottomRight.x + "," + bottomRight.y);
    }

    function test_narrow_help_wraps_and_scrolls_without_horizontal_overflow_data() {
        return [
            {
                tag: "default-font",
                fontScale: 1
            },
            {
                tag: "large-font",
                fontScale: 1.75
            }
        ];
    }

    function test_narrow_help_wraps_and_scrolls_without_horizontal_overflow(data) {
        const window = openHelp(data.fontScale);
        const dialog = window.helpDialog;
        const scroll = dialog.contentItem;
        const flickable = scroll.contentItem;
        verify(scroll.contentWidth <= scroll.availableWidth + 1, "Help should not require horizontal scrolling");
        verify(flickable.contentWidth <= flickable.width + 1, "The help viewport should contain the full text width");

        const labels = readableLabels(flickable);
        compare(labels.length, 12, "All six help headings and paragraphs should be present");
        for (const label of labels) {
            compare(label.wrapMode, Text.WordWrap);
            verify(label.width > 0);
            verify(label.contentWidth <= label.width + 1, "Help text should wrap within its allocated width: " + label.text);
            verify(label.contentHeight <= label.height + 1, "Wrapped text should retain its full height: " + label.text);
            const left = label.mapToItem(flickable, 0, 0).x;
            verify(left >= -1 && left + label.width <= flickable.width + 1, "Help text should stay inside the horizontal viewport");
        }
        verify(flickable.contentHeight > flickable.height, "A narrow window should expose vertical scrolling for the full help");
        flickable.contentY = flickable.contentHeight - flickable.height;
        tryVerify(() => flickable.contentY > 0);
        const lastLabel = labels[labels.length - 1];
        const bottom = lastLabel.mapToItem(flickable, 0, lastLabel.height).y;
        verify(bottom <= flickable.height + 1, "The final help section should be reachable by vertical scrolling");

        dialog.close();
        tryCompare(dialog, "visible", false);
    }

    function test_close_button_stays_accessible_data() {
        return [
            {
                tag: "default-font",
                fontScale: 1
            },
            {
                tag: "large-font",
                fontScale: 1.75
            }
        ];
    }

    function test_close_button_stays_accessible(data) {
        const window = openHelp(data.fontScale);
        const dialog = window.helpDialog;
        verifyFitsWindow(dialog.contentItem, window, "Help content");
        verifyFitsWindow(dialog.footer, window, "Help footer");

        const closeButton = dialog.standardButton(Controls.Dialog.Close);
        verifyFitsWindow(closeButton, window, "Close button");
        verify(closeButton.enabled);
        mouseClick(closeButton);
        tryCompare(dialog, "visible", false);
    }

    function test_help_can_be_closed_from_the_keyboard() {
        const window = openHelp(1.75);
        const dialog = window.helpDialog;
        const closeButton = dialog.standardButton(Controls.Dialog.Close);
        verify(closeButton !== null && closeButton.enabled);
        closeButton.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(closeButton, "activeFocus", true);
        keyClick(Qt.Key_Escape);
        tryCompare(dialog, "visible", false);
    }
}
