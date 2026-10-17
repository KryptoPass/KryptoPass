pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtTest
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "AdaptivePanes"
    width: Kirigami.Units.gridUnit * 80
    height: Kirigami.Units.gridUnit * 30
    visible: true
    when: windowShown

    Component {
        id: panesComponent

        AdaptivePanes {
            id: panes
            property real inputFontSize: Kirigami.Theme.defaultFont.pointSize
            property alias navigationField: navigationInput
            property alias headerField: headerInput
            property alias listField: listInput
            property alias detailField: detailInput
            property alias footerField: footerInput

            height: Kirigami.Units.gridUnit * 24

            navigationContent: Controls.TextField {
                id: navigationInput
                width: parent.width
                focus: true
                font.pointSize: panes.inputFontSize
                text: "Navigation"
            }
            headerContent: Controls.TextField {
                id: headerInput
                width: parent.width
                font.pointSize: panes.inputFontSize
                text: "Search"
            }
            listContent: Controls.TextField {
                id: listInput
                width: parent.width
                focus: true
                font.pointSize: panes.inputFontSize
                text: "Selected entry"
            }
            detailContent: Controls.TextField {
                id: detailInput
                width: parent.width
                focus: true
                font.pointSize: panes.inputFontSize
                text: "Unsaved detail"
            }
            footerContent: Controls.TextField {
                id: footerInput
                width: parent.width
                font.pointSize: panes.inputFontSize
                text: "Status"
            }
        }
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createPanes(widthUnits, fontScale = 1) {
        const panes = createTemporaryObject(panesComponent, testCase, {
            width: Kirigami.Units.gridUnit * widthUnits,
            inputFontSize: Kirigami.Theme.defaultFont.pointSize * fontScale
        });
        verify(panes !== null);
        verify(waitForRendering(panes));
        return panes;
    }

    function paneFields(panes) {
        return [panes.navigationField, panes.listField, panes.detailField];
    }

    function compareVisiblePanes(panes, selectedPane) {
        compare(panes.navigationVisible, !panes.compact || selectedPane === 0);
        compare(panes.listVisible, !panes.compact || selectedPane === 1);
        compare(panes.detailVisible, !panes.compact || selectedPane === 2);
    }

    function test_initial_layout_and_breakpoint() {
        const panes = createPanes(72);
        compare(panes.currentPane, 1);
        compare(panes.compact, false);
        compareVisiblePanes(panes, 1);

        panes.width = Kirigami.Units.gridUnit * 60;
        compare(panes.compact, false);
        compareVisiblePanes(panes, 1);

        panes.width -= 1;
        compare(panes.compact, true);
        compareVisiblePanes(panes, 1);
        verify(panes.listField.visible);
        verify(panes.headerField.visible);
        verify(panes.footerField.visible);
    }

    function test_each_pane_can_be_opened_and_edited_data() {
        return [
            {
                tag: "compact",
                widthUnits: 30
            },
            {
                tag: "wide",
                widthUnits: 72
            }
        ];
    }

    function test_each_pane_can_be_opened_and_edited(data) {
        const panes = createPanes(data.widthUnits);
        const fields = paneFields(panes);
        for (let index = 0; index < fields.length; ++index) {
            panes.showPane(index);
            compare(panes.currentPane, index);
            compareVisiblePanes(panes, index);
            const field = fields[index];
            tryCompare(field, "activeFocus", true);
            verify(field.visible);
            tryVerify(() => field.width > 0 && field.height > 0);

            const before = field.text;
            keyClick(Qt.Key_End);
            keyClick(Qt.Key_1);
            compare(field.text, before + "1");
        }
    }

    function test_resize_preserves_children_and_editor_state_data() {
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

    function test_resize_preserves_children_and_editor_state(data) {
        const panes = createPanes(72, data.fontScale);
        const originalFields = paneFields(panes);
        const originalHeader = panes.headerField;
        const originalFooter = panes.footerField;
        const editedTexts = ["Changed navigation", "Changed selection", "Pending edits"];
        for (let index = 0; index < originalFields.length; ++index)
            originalFields[index].text = editedTexts[index];
        originalHeader.text = "Active filter";
        originalFooter.text = "Pending status";
        panes.showPane(2);
        tryCompare(originalFields[2], "activeFocus", true);

        for (const widthUnits of [30, 72, 30, 72, 30]) {
            panes.width = Kirigami.Units.gridUnit * widthUnits;
            tryCompare(panes, "compact", widthUnits < 60);
            compare(panes.currentPane, 2);
            compareVisiblePanes(panes, 2);
            verify(waitForRendering(panes));

            const currentFields = paneFields(panes);
            for (let index = 0; index < currentFields.length; ++index) {
                compare(currentFields[index], originalFields[index]);
                compare(currentFields[index].text, editedTexts[index]);
            }
            compare(panes.headerField, originalHeader);
            compare(panes.footerField, originalFooter);
            compare(panes.headerField.text, "Active filter");
            compare(panes.footerField.text, "Pending status");
        }

        for (let index = 0; index < originalFields.length; ++index) {
            panes.showPane(index);
            compareVisiblePanes(panes, index);
            tryCompare(originalFields[index], "activeFocus", true);
            compare(originalFields[index].text, editedTexts[index]);
        }
    }
}
