pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "BrowserPairing"
    width: 640
    height: 480
    visible: true
    when: windowShown

    Component {
        id: dialogComponent
        BrowserPairingDialog {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createDialog(locked) {
        const dialog = createTemporaryObject(dialogComponent, testCase, {
            requestToken: "",
            vaultLocked: locked
        });
        verify(dialog !== null);
        return dialog;
    }

    function decisionSpy(dialog) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: dialog,
            signalName: "decisionRequested"
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    function footerButton(dialog, label) {
        const children = dialog.footer.children;
        for (let index = 0; index < children.length; ++index) {
            if (children[index].text === label)
                return children[index];
        }
        return null;
    }

    function openFor(dialog, token) {
        dialog.requestToken = token;
        tryCompare(dialog, "visible", true);
    }

    function test_incoming_token_opens_with_rejection_focused_and_enter_denies_data() {
        return [
            { tag: "enter", key: Qt.Key_Enter },
            { tag: "return", key: Qt.Key_Return }
        ];
    }

    function test_incoming_token_opens_with_rejection_focused_and_enter_denies(row) {
        const dialog = createDialog(false);
        const decision = decisionSpy(dialog);
        const reject = footerButton(dialog, "Rechazar conexión");
        verify(reject !== null);
        const token = "browser-request-41";

        openFor(dialog, token);
        tryCompare(reject, "activeFocus", true);
        keyClick(row.key);

        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, false]);
        dialog.requestToken = "";
        tryCompare(dialog, "visible", false);
    }

    function test_buttons_emit_the_selected_decision_for_the_current_token_data() {
        return [
            { tag: "reject", label: "Rechazar conexión", approve: false },
            { tag: "approve", label: "Vincular navegador", approve: true }
        ];
    }

    function test_buttons_emit_the_selected_decision_for_the_current_token(row) {
        const dialog = createDialog(false);
        const decision = decisionSpy(dialog);
        const button = footerButton(dialog, row.label);
        verify(button !== null);
        const token = "browser-request-button";

        openFor(dialog, token);
        waitForRendering(button);
        mouseClick(button);

        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, row.approve]);
        dialog.requestToken = "";
        tryCompare(dialog, "visible", false);
    }

    function test_escape_rejects_the_current_request() {
        const dialog = createDialog(false);
        const decision = decisionSpy(dialog);
        const token = "browser-request-escape";

        openFor(dialog, token);
        keyClick(Qt.Key_Escape);
        tryCompare(dialog, "visible", false);

        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, false]);
    }

    function test_clearing_request_token_closes_without_deciding() {
        const dialog = createDialog(false);
        const decision = decisionSpy(dialog);

        openFor(dialog, "browser-request-cleared");
        dialog.requestToken = "";

        tryCompare(dialog, "visible", false);
        compare(decision.count, 0);
    }

    function test_locked_vault_cannot_approve() {
        const dialog = createDialog(true);
        const decision = decisionSpy(dialog);
        const approve = footerButton(dialog, "Vincular navegador");
        verify(approve !== null);

        openFor(dialog, "browser-request-locked");
        verify(!approve.enabled);
        mouseClick(approve);

        compare(decision.count, 0);
        dialog.requestToken = "";
        tryCompare(dialog, "visible", false);
    }

    function test_content_wraps_without_truncation_at_larger_system_font_size() {
        const dialog = createDialog(false);
        const message = dialog.contentItem;

        openFor(dialog, "browser-request-large-font");
        message.font.pixelSize = Math.max(24, message.font.pixelSize * 1.5);
        tryVerify(() => message.lineCount > 1 && !message.truncated);

        verify(message.height >= message.implicitHeight - 1,
            "The full browser-pairing message should fit at the larger font size");
        dialog.requestToken = "";
        tryCompare(dialog, "visible", false);
    }
}
