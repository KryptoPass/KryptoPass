pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "BrowserAccess"
    width: 640
    height: 480
    visible: true
    when: windowShown

    Component {
        id: dialogComponent
        BrowserAccessDialog {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function createDialog() {
        const dialog = createTemporaryObject(dialogComponent, testCase, {
            requestJson: "",
            vaultLocked: false,
            entryMetadata: [
                { id: "work-entry", folder: "work" },
                { id: "personal-entry", folder: "personal" }
            ],
            folders: [
                { id: "root", name: "Accounts", parent: "" },
                { id: "work", name: "Work", parent: "root" },
                { id: "personal", name: "Personal", parent: "root" }
            ],
            folderRows: [
                { id: "root", name: "Accounts", depth: 0 },
                { id: "work", name: "Work", depth: 1 },
                { id: "personal", name: "Personal", depth: 1 }
            ]
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

    function requestFor(token) {
        return JSON.stringify({
            token: token,
            origin: "https://accounts.example.invalid",
            candidates: [
                { id: "work-entry", name: "Work account", revision: 7 },
                { id: "personal-entry", name: "Personal account", revision: 12 }
            ]
        });
    }

    function requestWithUsernames(token) {
        return JSON.stringify({
            token: token,
            origin: "https://accounts.example.invalid",
            previewReady: true,
            candidates: [
                { id: "work-entry", name: "Email account", username: "work.user@example.invalid", revision: 7 },
                { id: "personal-entry", name: "Email account", username: "personal.user@example.invalid", revision: 12 }
            ]
        });
    }

    function requestWithUsernameState(token, state, username) {
        const candidate = {
            id: "work-entry",
            name: "Email account",
            usernameState: state,
            revision: 7
        };
        if (username !== undefined)
            candidate.username = username;

        return JSON.stringify({
            token: token,
            origin: "https://accounts.example.invalid",
            previewReady: true,
            candidates: [
                candidate,
                {
                    id: "personal-entry",
                    name: "Email account",
                    usernameState: "unloaded",
                    revision: 12
                }
            ]
        });
    }

    function openFor(dialog, token) {
        dialog.requestJson = requestFor(token);
        tryCompare(dialog, "visible", true);
        tryCompare(dialog, "opened", true);
    }

    function child(objectName) {
        const object = findChild(testCase, objectName);
        verify(object !== null, "Could not find " + objectName);
        return object;
    }

    function findText(object, wantedText, seen) {
        if (!object)
            return null;
        seen = seen || [];
        if (seen.indexOf(object) !== -1)
            return null;
        seen.push(object);

        if (object.text === wantedText)
            return object;

        const children = object.children || [];
        for (let index = 0; index < children.length; ++index) {
            const result = findText(children[index], wantedText, seen);
            if (result)
                return result;
        }
        if (object.contentItem) {
            const result = findText(object.contentItem, wantedText, seen);
            if (result)
                return result;
        }
        if (object.item) {
            const result = findText(object.item, wantedText, seen);
            if (result)
                return result;
        }
        return null;
    }

    function findLoaderForItem(object, item, seen) {
        if (!object)
            return null;
        seen = seen || [];
        if (seen.indexOf(object) !== -1)
            return null;
        seen.push(object);

        if (object.item === item)
            return object;

        const children = object.children || [];
        for (let index = 0; index < children.length; ++index) {
            const result = findLoaderForItem(children[index], item, seen);
            if (result)
                return result;
        }
        if (object.contentItem) {
            const result = findLoaderForItem(object.contentItem, item, seen);
            if (result)
                return result;
        }
        return null;
    }

    function verifyFitsWidth(item, container, label) {
        verify(item !== null && item.visible, label + " should be visible");
        verify(item.width > 0, label + " should have a positive width");
        verify(item.height > 0, label + " should have a positive height");
        const topLeft = item.mapToItem(container, 0, 0);
        const bottomRight = item.mapToItem(container, item.width, item.height);
        verify(topLeft.x >= -1 && bottomRight.x <= container.width + 1,
            label + " should fit within the dialog content width");
    }

    function test_origin_and_browser_choices_default_to_deny() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-metadata";

        openFor(dialog, token);

        compare(dialog.request.origin, "https://accounts.example.invalid");
        verify(dialog.introduction !== null);
        const originLabel = findText(dialog, "https://accounts.example.invalid");
        verify(originLabel !== null, "The introduction loader should show the requested origin");
        const instructionLabel = findText(dialog,
            "Elige una credencial. Solo se enviará si tiene guardado este mismo sitio. La autorización sirve para esta solicitud.");
        verify(instructionLabel !== null, "The introduction should contain its explanatory text");
        const introductionLoader = findLoaderForItem(dialog, originLabel.parent);
        verify(introductionLoader !== null, "The introduction should be loaded into the shared picker");
        const list = child("entryPickerList");
        const search = child("entryPickerSearch");
        const folderPicker = child("entryPickerFolderPicker");
        const confirm = child("confirmEntryPickerButton");
        const cancel = child("cancelEntryPickerButton");
        tryCompare(list, "count", 2);
        verify(!confirm.enabled, "Approval must stay disabled until a candidate is selected");
        compare(dialog.pendingId, "");
        compare(dialog.options.map(option => [option.id, option.title, option.folder]), [
            ["work-entry", "Work account", "work"],
            ["personal-entry", "Personal account", "personal"]
        ]);

        waitForRendering(list);
        const workChoice = list.itemAtIndex(0);
        const personalChoice = list.itemAtIndex(1);
        verify(workChoice !== null && personalChoice !== null);
        compare(workChoice.text, "", "Shared delegates keep display text in the title model");
        compare(workChoice.modelData.title, "Work account");
        compare(personalChoice.modelData.title, "Personal account");
        compare(workChoice.Accessible.name, "Work account");

        const originalWidth = testCase.width;
        try {
            for (const viewportWidth of [640, 480]) {
                testCase.width = viewportWidth;
                tryVerify(() => dialog.contentItem.width > 0 && dialog.width <= viewportWidth);
                waitForRendering(dialog.contentItem);
                verifyFitsWidth(introductionLoader, dialog.contentItem, "Introduction loader");
                verifyFitsWidth(introductionLoader.item, dialog.contentItem, "Introduction content");
                verifyFitsWidth(originLabel, dialog.contentItem, "Origin label");
                verifyFitsWidth(instructionLabel, dialog.contentItem, "Instruction label");
                verifyFitsWidth(search, dialog.contentItem, "Search field");
                verifyFitsWidth(folderPicker, dialog.contentItem, "Folder picker");
                verifyFitsWidth(list, dialog.contentItem, "Candidate list");
            }
        } finally {
            testCase.width = originalWidth;
        }
        tryVerify(() => dialog.width > 0 && dialog.width <= originalWidth);
        waitForRendering(dialog.contentItem);

        mouseClick(confirm);
        compare(decision.count, 0, "An unselected candidate cannot be approved");
        waitForRendering(cancel);
        mouseClick(cancel);

        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_shared_picker_search_and_folder_filter_use_browser_names() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-filters";

        openFor(dialog, token);
        const list = child("entryPickerList");
        const search = child("entryPickerSearch");
        const folderPicker = child("entryPickerFolderPicker");
        const confirm = child("confirmEntryPickerButton");

        search.text = "Personal account";
        tryCompare(list, "count", 1);
        compare(dialog.pendingId, "");
        waitForRendering(list);
        compare(list.itemAtIndex(0).modelData.title, "Personal account");
        verify(!confirm.enabled);

        search.text = "";
        folderPicker.folderSelected("work");
        tryCompare(dialog, "folderFilterId", "work");
        tryCompare(list, "count", 1);
        compare(dialog.pendingId, "");
        waitForRendering(list);
        compare(list.itemAtIndex(0).modelData.title, "Work account");

        search.text = "Work";
        tryCompare(list, "count", 1);
        compare(list.itemAtIndex(0).Accessible.name, "Work account");
        compare(decision.count, 0);

        mouseClick(child("cancelEntryPickerButton"));
        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_browser_candidates_show_and_search_by_username() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-usernames";

        dialog.requestJson = requestWithUsernames(token);
        tryCompare(dialog, "visible", true);
        tryCompare(dialog, "opened", true);

        const list = child("entryPickerList");
        const search = child("entryPickerSearch");
        const folderPicker = child("entryPickerFolderPicker");
        const confirm = child("confirmEntryPickerButton");
        tryCompare(list, "count", 2);
        compare(dialog.options.map(option => [option.id, option.title, option.subtitle, option.folder]), [
            ["work-entry", "Email account", "work.user@example.invalid", "work"],
            ["personal-entry", "Email account", "personal.user@example.invalid", "personal"]
        ]);

        waitForRendering(list);
        const workChoice = list.itemAtIndex(0);
        const subtitle = findText(workChoice.contentItem, "work.user@example.invalid");
        verify(subtitle !== null, "The candidate row should render its username as the subtitle");
        compare(subtitle.textFormat, Text.PlainText);
        compare(workChoice.Accessible.description, "work.user@example.invalid");

        folderPicker.folderSelected("work");
        tryCompare(dialog, "folderFilterId", "work");
        tryCompare(list, "count", 1);
        waitForRendering(list);
        compare(list.itemAtIndex(0).modelData.id, "work-entry");
        compare(list.itemAtIndex(0).modelData.folder, "work",
            "The browser candidate should retain its folder metadata for filtering");

        folderPicker.folderSelected("");
        tryCompare(dialog, "folderFilterId", "");
        tryCompare(list, "count", 2);
        search.text = "personal.user@example.invalid";
        tryCompare(list, "count", 1);
        waitForRendering(list);
        const selectedChoice = list.itemAtIndex(0);
        compare(selectedChoice.modelData.title, "Email account");
        compare(selectedChoice.modelData.subtitle, "personal.user@example.invalid");
        mouseClick(selectedChoice);
        tryCompare(dialog, "pendingId", "personal-entry");

        verify(confirm.enabled);
        waitForRendering(confirm);
        mouseClick(confirm);
        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "personal-entry", true]);
    }

    function test_username_load_is_lazy_and_same_token_updates_preserve_picker_state() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const usernameRequest = createTemporaryObject(spyComponent, testCase, {
            target: dialog,
            signalName: "usernameRequested"
        });
        verify(usernameRequest !== null && usernameRequest.valid);
        const token = "browser-access-lazy-username";

        dialog.requestJson = requestWithUsernameState(token, "unloaded");
        tryCompare(dialog, "visible", true);
        tryCompare(dialog, "opened", true);
        compare(dialog.displayedToken, token,
            "The active browser token should be recorded before a username update arrives");

        const list = child("entryPickerList");
        const search = child("entryPickerSearch");
        const folderPicker = child("entryPickerFolderPicker");
        const confirm = child("confirmEntryPickerButton");
        tryCompare(list, "count", 2);
        compare(dialog.options.map(option => [option.id, option.title, option.folder]), [
            ["work-entry", "Email account", "work"],
            ["personal-entry", "Email account", "personal"]
        ]);
        compare(usernameRequest.count, 0,
            "Opening the request must not fetch usernames for every candidate");
        waitForRendering(list);
        const workChoice = list.itemAtIndex(0);
        compare(workChoice.modelData.subtitle, "Selecciona para ver el usuario");
        verify(findText(workChoice.contentItem, "Selecciona para ver el usuario") !== null,
            "The unloaded candidate should show its username prompt immediately");
        verify(!confirm.enabled, "An unloaded username cannot be approved");

        folderPicker.folderSelected("work");
        tryCompare(dialog, "folderFilterId", "work");
        search.text = "Email account";
        tryCompare(list, "count", 1);
        waitForRendering(list);
        mouseClick(list.itemAtIndex(0));
        tryCompare(dialog, "pendingId", "work-entry");
        compare(usernameRequest.count, 1);
        compare(Array.from(usernameRequest.signalArguments[0]), [token, "work-entry"]);
        verify(!confirm.enabled, "An unloaded username remains ineligible after selection");

        dialog.requestJson = requestWithUsernameState(token, "loading");
        tryCompare(dialog, "pendingId", "work-entry");
        compare(dialog.folderFilterId, "work");
        compare(search.text, "Email account");
        tryCompare(list, "count", 1);
        waitForRendering(list);
        const loadingChoice = list.itemAtIndex(0);
        compare(loadingChoice.modelData.subtitle, "Cargando usuario…");
        verify(findText(loadingChoice.contentItem, "Cargando usuario…") !== null,
            "A loading update should render its progress subtitle");
        compare(usernameRequest.count, 1, "A same-token loading update must not request the username again");
        verify(!confirm.enabled, "A username still loading cannot be approved");

        dialog.requestJson = requestWithUsernameState(token, "error");
        tryCompare(dialog, "pendingId", "work-entry");
        compare(dialog.folderFilterId, "work");
        compare(search.text, "Email account");
        tryCompare(list, "count", 1);
        waitForRendering(list);
        const errorChoice = list.itemAtIndex(0);
        compare(errorChoice.modelData.subtitle, "No se pudo leer el usuario");
        verify(findText(errorChoice.contentItem, "No se pudo leer el usuario") !== null,
            "An error update should render its recovery subtitle");
        compare(usernameRequest.count, 1);
        verify(!confirm.enabled, "A failed username cannot be approved");

        dialog.requestJson = requestWithUsernameState(token, "ready", "work.user@example.invalid");
        tryCompare(dialog, "pendingId", "work-entry");
        compare(dialog.folderFilterId, "work");
        compare(search.text, "Email account");
        tryCompare(list, "count", 1);
        waitForRendering(list);
        const readyChoice = list.itemAtIndex(0);
        compare(readyChoice.modelData.subtitle, "work.user@example.invalid");
        verify(findText(readyChoice.contentItem, "work.user@example.invalid") !== null,
            "The ready update should render the loaded username");
        compare(usernameRequest.count, 1);
        verify(confirm.enabled, "The candidate becomes approvable when its username is ready");

        mouseClick(confirm);
        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "work-entry", true]);
    }

    function test_approval_emits_only_the_selected_candidate_id() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-approved";

        openFor(dialog, token);
        const list = child("entryPickerList");
        const search = child("entryPickerSearch");
        tryCompare(list, "count", 2);
        search.text = "Personal";
        tryCompare(list, "count", 1);
        waitForRendering(list);
        const selectedChoice = list.itemAtIndex(0);
        verify(selectedChoice !== null);
        compare(selectedChoice.modelData.title, "Personal account");
        mouseClick(selectedChoice);
        tryCompare(dialog, "pendingId", "personal-entry");

        const confirm = child("confirmEntryPickerButton");
        verify(confirm.enabled);
        waitForRendering(confirm);
        mouseClick(confirm);

        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "personal-entry", true]);
    }

    function test_enter_with_initial_reject_focus_denies_without_approval() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-enter-denied";

        openFor(dialog, token);
        const cancel = child("cancelEntryPickerButton");
        const confirm = child("confirmEntryPickerButton");
        tryCompare(cancel, "activeFocus", true);
        verify(!confirm.enabled);

        keyClick(Qt.Key_Return);

        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        compare(Array.from(decision.signalArguments[0]), [token, "", false]);
    }

    function test_clearing_request_closes_without_deciding() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);

        openFor(dialog, "browser-access-cleared");
        dialog.requestJson = "";

        tryCompare(dialog, "visible", false);
        compare(decision.count, 0);
    }

    function test_lock_closes_request_as_denial_after_selection() {
        const dialog = createDialog();
        const decision = decisionSpy(dialog);
        const token = "browser-access-locked";

        openFor(dialog, token);
        const list = child("entryPickerList");
        tryCompare(list, "count", 2);
        waitForRendering(list);
        mouseClick(list.itemAtIndex(0));
        tryCompare(dialog, "pendingId", "work-entry");

        dialog.vaultLocked = true;

        tryCompare(dialog, "visible", false);
        compare(decision.count, 1);
        const args = Array.from(decision.signalArguments[0]);
        compare(args[0], token);
        compare(args[2], false, "Locking must never approve a browser request");
    }
}
