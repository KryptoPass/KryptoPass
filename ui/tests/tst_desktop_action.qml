pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase

    function configure(host, text, icon, shortcuts) {
        host.controller.configuredActionsJson = JSON.stringify({
            openEntry: {
                text: text,
                icon: icon,
                shortcuts: shortcuts
            }
        });
        host.controller.configureShortcuts();
    }
    function createHost(enabled) {
        const host = createTemporaryObject(hostComponent, testCase, {
            visible: true,
            actionEnabled: enabled === undefined ? true : enabled
        });
        verify(host !== null);
        host.requestActivate();
        tryCompare(host, "active", true);
        host.actionButton.forceActiveFocus();
        waitForRendering(host.actionButton);
        return host;
    }
    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign|Injection of parameters/);
    }
    function test_all_registered_shortcuts_follow_enabled_state() {
        const host = createHost();
        const triggered = triggeredSpy(host);

        keySequence("Ctrl+Alt+O");
        compare(triggered.count, 1, "The primary shortcut should trigger once");
        keySequence("Ctrl+Shift+O");
        compare(triggered.count, 2, "The first alternative should trigger once");
        keySequence("Ctrl+Alt+Shift+O");
        compare(triggered.count, 3, "Every registered alternative should be active");

        const synchronizedCount = host.controller.synchronizedCount;
        host.actionEnabled = false;
        tryCompare(host.controlledAction, "enabled", false);
        compare(host.actionButton.enabled, false);
        verify(host.controller.synchronizedCount > synchronizedCount);
        compare(host.controller.synchronizedId, "openEntry");
        compare(host.controller.synchronizedEnabled, false);
        keySequence("Ctrl+Alt+O");
        keySequence("Ctrl+Shift+O");
        keySequence("Ctrl+Alt+Shift+O");
        mouseClick(host.actionButton);
        compare(triggered.count, 3, "Disabled actions must ignore every entry point");

        host.actionEnabled = true;
        tryCompare(host.actionButton, "enabled", true);
        compare(host.controller.synchronizedEnabled, true);
        keySequence("Ctrl+Shift+O");
        compare(triggered.count, 4, "Re-enabling should restore alternative shortcuts");
    }
    function test_configuration_replaces_metadata_and_all_shortcuts() {
        const host = createHost();
        const triggered = triggeredSpy(host);

        configure(host, "Abrir elemento actualizado", "document-edit", ["F8", "F9"]);

        tryCompare(host.controlledAction, "text", "Abrir elemento actualizado");
        compare(host.controlledAction.icon.name, "document-edit");
        compare(host.actionButton.text, "Abrir elemento actualizado");
        compare(host.actionButton.icon.name, "document-edit");
        keySequence("Ctrl+Alt+O");
        keySequence("Ctrl+Shift+O");
        keySequence("Ctrl+Alt+Shift+O");
        compare(triggered.count, 0, "Previous shortcuts must stop activating the action");
        keySequence("F8");
        compare(triggered.count, 1, "The configured primary shortcut should be active");
        keySequence("F9");
        compare(triggered.count, 2, "The configured alternative should be active");

        configure(host, "Abrir sin atajos", "document-open", []);
        tryCompare(host.controlledAction, "text", "Abrir sin atajos");
        keySequence("F8");
        keySequence("F9");
        compare(triggered.count, 2, "Removing shortcuts must unregister primary and alternatives");
        mouseClick(host.actionButton);
        compare(triggered.count, 3, "Removing shortcuts must preserve button activation");
    }
    function test_initially_disabled_action_is_synchronized_and_inactive() {
        const host = createHost(false);
        const triggered = triggeredSpy(host);

        verify(host.controller.synchronizedCount > 0);
        compare(host.controller.synchronizedId, "openEntry");
        compare(host.controller.synchronizedEnabled, false);
        compare(host.actionButton.enabled, false);
        keySequence("Ctrl+Alt+O");
        keySequence("Ctrl+Shift+O");
        compare(triggered.count, 0);
    }
    function test_registry_metadata_and_button_share_the_action() {
        const host = createHost();
        const action = host.controlledAction;
        const triggered = triggeredSpy(host);

        compare(action.text, "Abrir elemento");
        compare(action.icon.name, "document-open");
        compare(host.actionButton.text, action.text);
        compare(host.actionButton.icon.name, action.icon.name);
        verify(host.controller.synchronizedCount > 0);
        compare(host.controller.synchronizedId, "openEntry");
        compare(host.controller.synchronizedEnabled, true);

        mouseClick(host.actionButton);
        compare(triggered.count, 1);
    }
    function triggeredSpy(host) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: host.controlledAction,
            signalName: "triggered"
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    height: 240
    name: "DesktopAction"
    visible: true
    when: windowShown
    width: 400

    Component {
        id: hostComponent

        Window {
            id: host

            property alias actionButton: button
            property bool actionEnabled: true
            property alias controlledAction: desktopAction
            property alias controller: fakeController

            height: 240
            width: 400

            QtObject {
                id: fakeController

                property string actionsJson: JSON.stringify({
                    openEntry: {
                        text: "Abrir elemento",
                        icon: "document-open",
                        shortcuts: ["Ctrl+Alt+O", "Ctrl+Shift+O", "Ctrl+Alt+Shift+O"]
                    },
                    otherEntry: {
                        text: "Otra acción",
                        icon: "document-new",
                        shortcuts: ["Ctrl+N"]
                    }
                })
                property string configuredActionsJson: ""
                property int synchronizedCount: 0
                property bool synchronizedEnabled: false
                property string synchronizedId: ""

                function configureShortcuts() {
                    actionsJson = configuredActionsJson;
                }
                function setActionEnabled(actionId, enabled) {
                    synchronizedId = actionId;
                    synchronizedEnabled = enabled;
                    synchronizedCount += 1;
                }
            }
            DesktopAction {
                id: desktopAction

                actionId: "openEntry"
                controller: fakeController
                enabled: host.actionEnabled
            }
            Controls.Button {
                id: button

                action: desktopAction
                anchors.centerIn: parent
            }
        }
    }
    Component {
        id: spyComponent

        SignalSpy {}
    }
}
