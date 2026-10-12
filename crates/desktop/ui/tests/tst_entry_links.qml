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
        RelatedEntryPicker { width: 480 }
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

    function createPicker(relatedEntryId) {
        const picker = createTemporaryObject(pickerComponent, testCase, {
            entries: entries(relatedEntryId),
            kindOptions: kindOptions,
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

    function test_picker_lists_only_unlinked_or_current_companion() {
        const picker = createPicker("partner");
        compare(ids(picker), ["", "free", "partner"]);

        const field = findChild(picker, "relatedEntryField");
        verify(field !== null);
        tryCompare(field, "currentValue", "partner");
    }

    function test_picker_selection_and_unlink_emit_the_draft_value() {
        const picker = createPicker("partner");
        const changed = spyFor(picker, "selectionChanged");

        verify(picker.selectRelatedEntry("free"));
        compare(changed.count, 1);
        compare(Array.from(changed.signalArguments[0]), ["free"]);

        verify(picker.selectRelatedEntry(""));
        compare(changed.count, 2);
        compare(Array.from(changed.signalArguments[1]), [""]);
        verify(!picker.selectRelatedEntry("occupied"));
        verify(!picker.selectRelatedEntry("missing"));
        compare(changed.count, 2);
    }

    function test_unavailable_persisted_companion_stays_visible_until_unlinked() {
        const picker = createPicker("trash");
        const choices = picker.options;
        compare(ids(picker), ["", "free", "partner", "trash"]);
        const unavailable = choices.find(option => option.id === "trash");
        verify(unavailable !== undefined && unavailable.unavailable);

        const field = findChild(picker, "relatedEntryField");
        verify(field !== null);
        tryCompare(field, "currentValue", "trash");
        verify(!picker.selectRelatedEntry("trash"));

        const changed = spyFor(picker, "selectionChanged");
        verify(picker.selectRelatedEntry(""));
        compare(changed.count, 1);
        compare(Array.from(changed.signalArguments[0]), [""]);
    }
}
