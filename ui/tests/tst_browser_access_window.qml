pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "BrowserAccessWindow"
    width: 640
    height: 480
    visible: true
    when: windowShown

    Component {
        id: windowComponent
        BrowserAccessWindow {}
    }

    Component {
        id: hostWindowComponent
        Window {
            width: 640
            height: 480
        }
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createWindow() {
        const window = createTemporaryObject(windowComponent, testCase, {
            requestJson: "",
            vaultLocked: false,
            entryMetadata: [
                {
                    id: "work-entry",
                    folder: "work"
                },
                {
                    id: "personal-entry",
                    folder: "personal"
                }
            ],
            kindOptions: [],
            folders: [
                {
                    id: "root",
                    name: "Accounts",
                    parent: ""
                },
                {
                    id: "work",
                    name: "Work",
                    parent: "root"
                },
                {
                    id: "personal",
                    name: "Personal",
                    parent: "root"
                }
            ],
            folderRows: [
                {
                    id: "root",
                    name: "Accounts",
                    depth: 0
                },
                {
                    id: "work",
                    name: "Work",
                    depth: 1
                },
                {
                    id: "personal",
                    name: "Personal",
                    depth: 1
                }
            ]
        });
        verify(window !== null);
        return window;
    }

    function pickerFor(window) {
        const picker = findChild(window, "browserAccessPicker");
        verify(picker !== null, "Could not find browserAccessPicker");
        return picker;
    }

    function child(parent, objectName) {
        const object = findChild(parent, objectName);
        verify(object !== null, "Could not find " + objectName);
        return object;
    }

    function decisionSpy(window) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: window,
            signalName: "decisionRequested"
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    function requestFor(token) {
        return JSON.stringify({
            token: token,
            origin: "https://accounts.example.invalid",
            candidates: [
                {
                    id: "work-entry",
                    name: "Work account",
                    revision: 7
                },
                {
                    id: "personal-entry",
                    name: "Personal account",
                    revision: 12
                }
            ]
        });
    }

    function openFor(window, token) {
        window.requestJson = requestFor(token);
        tryCompare(window, "visible", true);
        tryCompare(pickerFor(window), "opened", true);
    }

    function selectEntry(picker, index, id) {
        const list = child(picker, "entryPickerList");
        tryCompare(list, "count", 2);
        waitForRendering(list);
        const choice = list.itemAtIndex(index);
        verify(choice !== null);
        mouseClick(choice);
        tryCompare(picker, "pendingId", id);
    }

    function verifyFitsWindow(item, window, label) {
        verify(item !== null && item.visible, label + " should be visible");
        verify(item.width > 0 && item.height > 0, label + " should have positive dimensions");

        const topLeft = item.mapToItem(window.contentItem, 0, 0);
        const bottomRight = item.mapToItem(window.contentItem, item.width, item.height);
        verify(topLeft.x >= -1 && topLeft.y >= -1, label + " should not start outside the prompt window");
        verify(bottomRight.x <= window.contentItem.width + 1 && bottomRight.y <= window.contentItem.height + 1, label + " should fit inside the prompt window");
    }

    function test_window_is_independent_and_hosts_the_shared_picker() {
        const hostWindow = createTemporaryObject(hostWindowComponent, testCase, {
            visible: true
        });
        verify(hostWindow !== null && hostWindow.visible);
        const window = createWindow();
        const picker = pickerFor(window);

        compare(window.transientParent, null);
        verify((window.flags & Qt.WindowStaysOnTopHint) !== 0, "The browser prompt should stay above other windows");
        compare(picker.parent, window.contentItem, "The shared picker should be hosted by the prompt window");
        verify(!window.visible);

        hostWindow.hide();
        verify(!hostWindow.visible);
        openFor(window, "browser-window-independent");
        verify(window.visible);
        verify(!hostWindow.visible, "A request should open its own window while the main window is hidden");
        verify(picker.visible);
        tryCompare(child(picker, "entryPickerList"), "count", 2);
    }

    function test_popup_background_content_header_footer_and_buttons_fit_window() {
        const window = createWindow();
        const picker = pickerFor(window);

        openFor(window, "browser-window-geometry");

        const viewports = [
            {
                width: 640,
                height: 520
            },
            {
                width: 400,
                height: 480
            }
        ];
        for (const viewport of viewports) {
            window.width = viewport.width;
            window.height = viewport.height;
            tryCompare(window, "width", viewport.width);
            tryCompare(window, "height", viewport.height);
            tryCompare(window.contentItem, "width", viewport.width);
            tryCompare(window.contentItem, "height", viewport.height);
            waitForRendering(window.contentItem);

            verifyFitsWindow(picker.background, window, "Popup background");
            verifyFitsWindow(picker.contentItem, window, "Dialog content");
            verifyFitsWindow(picker.header, window, "Dialog header");
            verifyFitsWindow(picker.footer, window, "Dialog footer");
            verifyFitsWindow(child(picker, "cancelEntryPickerButton"), window, "Reject button");
            verifyFitsWindow(child(picker, "confirmEntryPickerButton"), window, "Approve button");
        }
    }

    function test_native_window_close_rejects_the_request_once() {
        const window = createWindow();
        const decision = decisionSpy(window);
        const token = "browser-window-native-close";

        openFor(window, token);
        window.close();

        tryCompare(window, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_dialog_rejection_hides_the_window_and_denies_once() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);
        const token = "browser-window-rejected";

        openFor(window, token);
        mouseClick(child(picker, "cancelEntryPickerButton"));

        tryCompare(window, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_escape_hides_the_window_and_denies_once() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);
        const token = "browser-window-escape";

        openFor(window, token);
        tryCompare(child(picker, "cancelEntryPickerButton"), "activeFocus", true);
        keyClick(Qt.Key_Escape);

        tryCompare(window, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_approval_hides_the_window_and_forwards_only_the_selected_id() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);
        const token = "browser-window-approved";

        openFor(window, token);
        selectEntry(picker, 1, "personal-entry");
        mouseClick(child(picker, "confirmEntryPickerButton"));

        tryCompare(window, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "personal-entry", true]);
    }

    function test_request_clear_hides_the_window_and_clears_selection() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);

        openFor(window, "browser-window-cleared");
        selectEntry(picker, 0, "work-entry");
        window.requestJson = "";

        tryCompare(window, "visible", false);
        compare(picker.pendingId, "");
        compare(decision.count, 0, "Clearing a request is not a user decision");
    }

    function test_lock_hides_the_window_clears_selection_and_denies() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);
        const token = "browser-window-locked";

        openFor(window, token);
        selectEntry(picker, 0, "work-entry");
        window.vaultLocked = true;

        tryCompare(window, "visible", false);
        tryCompare(picker, "pendingId", "");
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_new_token_reopens_with_no_stale_selection() {
        const window = createWindow();
        const picker = pickerFor(window);
        const decision = decisionSpy(window);

        openFor(window, "browser-window-first-token");
        selectEntry(picker, 0, "work-entry");
        mouseClick(child(picker, "cancelEntryPickerButton"));
        tryCompare(window, "visible", false);
        compare(decision.count, 1);

        window.requestJson = requestFor("browser-window-next-token");

        tryCompare(window, "visible", true);
        tryCompare(picker, "opened", true);
        compare(picker.pendingId, "");
        compare(decision.count, 1);

        selectEntry(picker, 1, "personal-entry");
        mouseClick(child(picker, "confirmEntryPickerButton"));
        tryCompare(window, "visible", false);
        compare(decision.count, 2);
        compare(Array.from(decision.signalArguments[1]), ["browser-window-next-token", "personal-entry", true]);
    }
}
