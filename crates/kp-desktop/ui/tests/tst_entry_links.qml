pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "EntryLinks"
    width: 640
    height: 320
    visible: true
    when: windowShown

    Component {
        id: pickerComponent
        RelatedEntryPicker { width: 560 }
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    readonly property var kindOptions: [
        { id: "login", name: "Inicio de sesión" },
        { id: "note", name: "Nota" }
    ]

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function entries(relatedEntryId) {
        return [
            { id: "current", title: "Current", kind: "login", trashed: false, relatedEntryId: relatedEntryId || "" },
            { id: "partner", title: "Partner", kind: "note", trashed: false, relatedEntryId: relatedEntryId === "partner" ? "current" : "" },
            { id: "free", title: "Free", kind: "note", trashed: false, relatedEntryId: "" },
            { id: "occupied", title: "Occupied", kind: "login", trashed: false, relatedEntryId: "elsewhere" },
            { id: "trash", title: "Trashed", kind: "note", trashed: true, relatedEntryId: relatedEntryId === "trash" ? "current" : "" }
        ];
    }

    function createPicker(relatedEntryId, candidates, folders, folderRows) {
        const picker = createTemporaryObject(pickerComponent, testCase, {
            entries: candidates || entries(relatedEntryId),
            kindOptions: kindOptions,
            folders: folders || [],
            folderRows: folderRows || [],
            entryId: "current",
            relatedEntryId: relatedEntryId || ""
        });
        verify(picker !== null);
        return picker;
    }

    function ids(picker) {
        return picker.options.map(option => option.id);
    }

    function spyFor(target, signalName) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: target,
            signalName: signalName
        });
        verify(spy !== null && spy.valid);
        return spy;
    }

    function child(objectName) {
        const object = findChild(testCase, objectName);
        verify(object !== null, "Could not find " + objectName);
        return object;
    }

    function test_eligibility_unavailable_companion_and_unlink() {
        const picker = createPicker("trash");
        compare(ids(picker), ["", "free", "partner", "trash"]);
        const unavailable = picker.options.find(option => option.id === "trash");
        verify(unavailable !== undefined && unavailable.unavailable);
        compare(picker.filteredOptions.map(option => option.id), ["free", "partner"]);

        verify(!picker.selectRelatedEntry("trash"));
        verify(!picker.selectRelatedEntry("occupied"));
        verify(!picker.selectRelatedEntry("missing"));

        const changed = spyFor(picker, "selectionChanged");
        const unlinkButton = findChild(picker, "unlinkRelatedEntryButton");
        verify(unlinkButton !== null && unlinkButton.visible);
        mouseClick(unlinkButton);
        compare(changed.count, 1);
        compare(Array.from(changed.signalArguments[0]), [""]);
    }

    function test_folder_picker_filters_and_candidate_uses_entry_icon() {
        const candidates = [
            { id: "current", title: "Current", kind: "note", trashed: false, relatedEntryId: "" },
            {
                id: "work-entry",
                title: "Work credential",
                kind: "login",
                folder: "work",
                icon: { kind: "theme", name: "network-server" },
                trashed: false,
                relatedEntryId: ""
            },
            {
                id: "personal-entry",
                title: "Personal credential",
                kind: "login",
                folder: "personal",
                icon: { kind: "theme", name: "user-home" },
                trashed: false,
                relatedEntryId: ""
            }
        ];
        const folders = [
            { id: "root", name: "Accounts", parent: "" },
            { id: "work", name: "Work", parent: "root" },
            { id: "personal", name: "Personal", parent: "root" }
        ];
        const folderRows = [
            { id: "root", name: "Accounts", depth: 0 },
            { id: "work", name: "Work", depth: 1 },
            { id: "personal", name: "Personal", depth: 1 }
        ];
        const picker = createPicker("", candidates, folders, folderRows);

        picker.openPicker();
        const dialog = child("relatedEntryDialog");
        const folderPicker = child("relatedEntryFolderPicker");
        const list = child("relatedEntryList");
        tryCompare(dialog, "opened", true);
        compare(folderPicker.selectedName, "Todas las carpetas");
        compare(folderPicker.options.map(option => option.id), ["", "unfiled", "root", "work", "personal"]);
        compare(list.count, 2);

        folderPicker.folderSelected("work");
        tryCompare(picker, "folderFilterId", "work");
        compare(folderPicker.selectedName, "Work");
        tryCompare(list, "count", 1);
        compare(list.currentIndex, -1);
        waitForRendering(list);
        const candidateIcon = findChild(list.itemAtIndex(0), "relatedEntryCandidateIcon");
        verify(candidateIcon !== null, "Filtered row should have its EntryIcon delegate");
        compare(candidateIcon.entryIcon.kind, "theme");
        compare(candidateIcon.entryIcon.name, "network-server");
        verify(candidateIcon.children.some(child => child.source === "network-server" && child.visible));
        picker.closePicker();
        tryCompare(dialog, "visible", false);
    }

    function test_large_picker_search_confirm_and_bounded_virtualized_list() {
        const candidates = [
            { id: "current", title: "Current", kind: "note", trashed: false, relatedEntryId: "" }
        ];
        for (let index = 0; index < 998; ++index) {
            candidates.push({
                id: "entry-" + index,
                title: "Record " + index,
                kind: "note",
                folder: "work",
                trashed: false,
                relatedEntryId: ""
            });
        }
        candidates.push({
            id: "target-0999",
            title: "Recovery Vault",
            kind: "login",
            folder: "emergency",
            trashed: false,
            relatedEntryId: ""
        });
        compare(candidates.length, 1000);

        const folders = [
            { id: "root", name: "Accounts", parent: "" },
            { id: "work", name: "Work", parent: "root" },
            { id: "emergency", name: "Emergency", parent: "" }
        ];
        const picker = createPicker("", candidates, folders);
        const changed = spyFor(picker, "selectionChanged");
        const modalState = spyFor(picker, "externalModalRequested");

        picker.openPicker();
        const dialog = child("relatedEntryDialog");
        const search = child("relatedEntrySearch");
        const list = child("relatedEntryList");
        const confirmButton = child("confirmRelatedEntryButton");
        tryCompare(dialog, "visible", true);
        tryCompare(dialog, "opened", true);
        verify(dialog.width > 0 && dialog.width <= testCase.width);
        verify(dialog.height > 0 && dialog.height <= testCase.height);
        compare(list.count, 999);
        verify(list.height > 0 && list.height < dialog.height);
        verify(list.contentHeight > list.height);
        verify(list.contentItem.children.length < list.count,
            "ListView should realize only the visible delegates for 1000 records");
        compare(Array.from(modalState.signalArguments[0]), [true]);

        search.text = "Recovery Vault";
        tryCompare(list, "count", 1);
        compare(list.currentIndex, -1);
        search.text = "Inicio de sesión";
        tryCompare(list, "count", 1);
        search.text = "Emergency";
        tryCompare(list, "count", 1);
        compare(list.currentIndex, -1);
        verify(!confirmButton.enabled);

        list.currentIndex = 0;
        compare(dialog.pendingId, "target-0999");
        verify(confirmButton.enabled);
        waitForRendering(confirmButton);
        mouseClick(confirmButton);
        tryCompare(dialog, "visible", false);
        compare(changed.count, 1);
        compare(Array.from(changed.signalArguments[0]), ["target-0999"]);
        compare(modalState.count, 2);
        compare(Array.from(modalState.signalArguments[1]), [false]);

        picker.openPicker();
        tryCompare(dialog, "opened", true);
        search.text = "Emergency";
        tryCompare(list, "count", 1);
        waitForRendering(search);
        search.forceActiveFocus();
        keyClick(Qt.Key_Down);
        tryCompare(list, "currentIndex", 0);
        tryCompare(list, "activeFocus", true);
        compare(dialog.pendingId, "target-0999");
        keyClick(Qt.Key_Enter);
        tryCompare(dialog, "visible", false);
        compare(changed.count, 2);
        compare(Array.from(changed.signalArguments[1]), ["target-0999"]);
        compare(modalState.count, 4);
        compare(Array.from(modalState.signalArguments[2]), [true]);
        compare(Array.from(modalState.signalArguments[3]), [false]);
    }

    function test_cancel_and_escape_close_without_committing() {
        const picker = createPicker("");
        const changed = spyFor(picker, "selectionChanged");
        const modalState = spyFor(picker, "externalModalRequested");

        picker.openPicker();
        const dialog = child("relatedEntryDialog");
        const search = child("relatedEntrySearch");
        const list = child("relatedEntryList");
        const cancelButton = child("cancelRelatedEntryButton");
        tryCompare(dialog, "opened", true);
        search.text = "Free";
        tryCompare(list, "count", 1);
        list.currentIndex = 0;
        compare(dialog.pendingId, "free");
        waitForRendering(cancelButton);
        mouseClick(cancelButton);
        tryCompare(dialog, "visible", false);
        compare(changed.count, 0);

        picker.openPicker();
        tryCompare(dialog, "opened", true);
        search.text = "Partner";
        tryCompare(list, "count", 1);
        list.currentIndex = 0;
        compare(dialog.pendingId, "partner");
        waitForRendering(search);
        search.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        tryCompare(dialog, "visible", false);
        compare(changed.count, 0);
        compare(modalState.count, 4);
        compare(Array.from(modalState.signalArguments[0]), [true]);
        compare(Array.from(modalState.signalArguments[1]), [false]);
        compare(Array.from(modalState.signalArguments[2]), [true]);
        compare(Array.from(modalState.signalArguments[3]), [false]);
    }
}
