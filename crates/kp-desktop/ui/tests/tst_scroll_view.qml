pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "ListAutoScroll"
    width: 900
    height: 400
    visible: true
    when: windowShown

    Component {
        id: appListComponent

        Controls.ScrollView {
            property alias listView: list
            width: 400
            height: 300

            AppListView {
                id: list
                model: 2000
                currentIndex: 0
                delegate: Item {
                    width: 400
                    height: 48
                }
            }
        }
    }

    Component {
        id: defaultListComponent

        Controls.ScrollView {
            property alias listView: list
            x: 450
            width: 400
            height: 300

            ListView {
                id: list
                model: 2000
                currentIndex: 0
                delegate: Item {
                    width: 400
                    height: 48
                }
            }
        }
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createList(component, count) {
        const view = createTemporaryObject(component, testCase);
        verify(view !== null);
        view.listView.model = count;
        verify(waitForRendering(view));
        compare(view.contentItem, view.listView);
        verify(view.listView.height > 0);
        tryVerify(() => view.listView.currentItem !== null);
        return view;
    }

    function selectedItemIsVisible(list) {
        const item = list.currentItem;
        return item !== null && item.y >= list.contentY - 0.5
                && item.y + item.height <= list.contentY + list.height + 0.5;
    }

    function test_selection_below_viewport_becomes_visible_promptly_data() {
        return [
            { tag: "short-list", count: 20 },
            { tag: "long-list", count: 2000 }
        ];
    }

    function test_selection_below_viewport_becomes_visible_promptly(data) {
        const view = createList(appListComponent, data.count);
        view.listView.currentIndex = data.count - 1;
        tryVerify(() => selectedItemIsVisible(view.listView), 450);
    }

    function test_selection_above_viewport_becomes_visible_promptly() {
        const view = createList(appListComponent, 2000);
        view.listView.currentIndex = 1999;
        tryVerify(() => selectedItemIsVisible(view.listView), 450);
        view.listView.currentIndex = 0;
        tryVerify(() => selectedItemIsVisible(view.listView), 450);
    }

    function test_selection_scroll_has_visible_intermediate_motion() {
        const view = createList(appListComponent, 20);
        if (view.listView.highlightMoveDuration === 0)
            skip("Animations are disabled by the desktop settings");
        const initialPosition = view.listView.contentY;
        const destination = view.listView.contentHeight - view.listView.height;
        const started = Date.now();
        view.listView.currentIndex = 19;
        tryVerify(() => view.listView.contentY > initialPosition + 0.5
                && view.listView.contentY < destination - 0.5, 150,
                "Selection scrolling must show an intermediate position before arrival");
        const remaining = Math.max(1, 450 - (Date.now() - started));
        tryVerify(() => selectedItemIsVisible(view.listView), remaining);
    }

    function test_already_visible_selection_keeps_scroll_position() {
        const view = createList(appListComponent, 2000);
        const initialPosition = view.listView.contentY;
        view.listView.currentIndex = 2;
        wait(300);
        verify(selectedItemIsVisible(view.listView));
        compare(view.listView.contentY, initialPosition);
    }

    function test_wheel_preserves_default_qt_scroll_behavior() {
        const appView = createList(appListComponent, 2000);
        const defaultView = createList(defaultListComponent, 2000);
        const initialPosition = appView.listView.contentY;
        for (let step = 0; step < 3; ++step) {
            mouseWheel(appView, 100, 100, 0, -120);
            mouseWheel(defaultView, 100, 100, 0, -120);
        }
        wait(500);
        verify(defaultView.listView.contentY > initialPosition);
        verify(Math.abs(appView.listView.contentY - defaultView.listView.contentY) < 0.5,
                "AppListView must retain Qt's wheel displacement");
        mouseWheel(appView, 100, 100, 0, 120);
        mouseWheel(defaultView, 100, 100, 0, 120);
        wait(500);
        verify(Math.abs(appView.listView.contentY - defaultView.listView.contentY) < 0.5,
                "AppListView must retain Qt's reverse wheel displacement");
        compare(appView.listView.currentIndex, 0);
        compare(defaultView.listView.currentIndex, 0);
    }
}
