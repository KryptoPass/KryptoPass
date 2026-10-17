pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "Importers"
    width: 1000
    height: 650
    visible: true
    when: windowShown

    Component {
        id: panelComponent
        ImporterPanel {}
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function fakeImporters() {
        const formats = ["csv", "json", "kdbx", "xml"];
        const importers = [];
        for (let index = 0; index < 200; ++index) {
            const suffix = String(index).padStart(3, "0");
            const name = "Catalog Manager " + suffix;
            const format = formats[index % formats.length];
            importers.push({
                id: "org.kryptopass.tests.importer.provider" + suffix,
                name: name,
                formats: [format],
                fileFilters: [name + " export (*." + format + " *." + format + ".backup)", name + " compressed export (*." + format + ".gz)"],
                entryTypes: index % 2 === 0 ? ["Logins", "Notes"] : ["Cards", "Identities"],
                capabilities: [
                    {
                        label: "Folders",
                        support: "supported"
                    },
                    {
                        label: "Attachments",
                        support: "reference"
                    },
                    {
                        label: "Passkeys",
                        support: "unsupported"
                    }
                ],
                decryptionAvailable: index % 3 !== 0 || index % 2 === 0,
                requiresDecryptor: index % 3 === 0
            });
        }
        return importers;
    }

    function child(parent, objectName) {
        const object = findChild(parent, objectName);
        verify(object !== null, "Could not find " + objectName);
        return object;
    }

    function createPanel(importers) {
        const panel = createTemporaryObject(panelComponent, testCase, {
            width: 1000,
            height: 650,
            importers: importers === undefined ? fakeImporters() : importers
        });
        verify(panel !== null);
        verify(waitForPolish(panel));
        const list = child(panel, "importerList");
        tryCompare(list, "count", panel.importers.length);
        tryVerify(() => list.width > 0 && list.height > 0);
        return panel;
    }

    function spyFor(panel, signalName) {
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: panel,
            signalName: signalName
        });
        verify(spy !== null && spy.valid, signalName);
        return spy;
    }

    function delegateAt(list, index) {
        list.forceLayout();
        tryVerify(() => list.itemAtIndex(index) !== null);
        const delegate = list.itemAtIndex(index);
        compare(delegate.objectName, "importerDelegate");
        return delegate;
    }

    function liveDelegateCount(list) {
        return list.contentItem.children.filter(item => item.objectName === "importerDelegate").length;
    }

    function test_catalog_starts_without_selection_or_file_action() {
        const panel = createPanel();
        const selected = spyFor(panel, "importerSelected");
        const requested = spyFor(panel, "chooseFileRequested");
        const list = child(panel, "importerList");
        const choose = child(panel, "chooseImportFile");

        compare(panel.importers.length, 200);
        compare(new Set(panel.importers.map(importer => importer.id)).size, 200);
        compare(panel.filteredImporters.length, 200);
        compare(panel.selectedImporterId, "");
        compare(panel.selectedImporter, null);
        compare(list.currentIndex, -1);
        verify(!choose.visible);
        verify(!choose.enabled);

        list.forceActiveFocus();
        keyClick(Qt.Key_Return);
        panel.selectImporter("org.kryptopass.tests.importer.missing");
        compare(panel.selectedImporterId, "");
        compare(panel.selectedImporter, null);
        compare(selected.count, 0);
        compare(requested.count, 0);
    }

    function test_generic_selection_preserves_metadata_and_requests_correct_importer_data() {
        return [
            {
                tag: "mouse-json",
                index: 137,
                key: 0
            },
            {
                tag: "return-kdbx",
                index: 126,
                key: Qt.Key_Return
            },
            {
                tag: "enter-xml",
                index: 199,
                key: Qt.Key_Enter
            }
        ];
    }

    function test_generic_selection_preserves_metadata_and_requests_correct_importer(row) {
        const panel = createPanel();
        const expected = JSON.parse(JSON.stringify(panel.importers[row.index]));
        const selected = spyFor(panel, "importerSelected");
        const requested = spyFor(panel, "chooseFileRequested");
        const search = child(panel, "importerSearch");
        const list = child(panel, "importerList");
        const choose = child(panel, "chooseImportFile");

        search.text = expected.name;
        tryCompare(list, "count", 1);
        const delegate = delegateAt(list, 0);
        compare(delegate.modelData.id, expected.id);
        if (row.key === 0) {
            mouseClick(delegate);
        } else {
            search.forceActiveFocus();
            keyClick(Qt.Key_Down);
            tryCompare(list, "currentIndex", 0);
            tryCompare(list, "activeFocus", true);
            compare(panel.selectedImporterId, "");
            compare(selected.count, 0);
            keyClick(row.key);
        }

        compare(panel.selectedImporterId, expected.id);
        compare(panel.selectedImporter, expected);
        compare(panel.selectedImporter.fileFilters, expected.fileFilters);
        compare(selected.count, 1);
        compare(Array.from(selected.signalArguments[0]), [expected.id]);
        compare(requested.count, 0);

        mouseClick(delegate);
        compare(selected.count, 1, "Selecting the same integration should not emit another change");
        verify(waitForPolish(panel));
        verify(choose.visible && choose.enabled);
        mouseClick(choose);
        compare(requested.count, 1);
        compare(Array.from(requested.signalArguments[0]), [expected.id]);
        compare(panel.selectedImporter.fileFilters, expected.fileFilters);
        compare(panel.importers[row.index], expected);
    }

    function test_search_filters_the_entire_catalog_data() {
        return [
            {
                tag: "name-case-and-whitespace",
                query: "  cAtALog mANager 173  ",
                indexes: [173]
            },
            {
                tag: "name-substring",
                query: "Manager 19",
                indexes: Array.from({
                    length: 10
                }, (_, i) => 190 + i)
            },
            {
                tag: "csv-format",
                query: " CsV ",
                indexes: Array.from({
                    length: 50
                }, (_, i) => i * 4)
            },
            {
                tag: "kdbx-format",
                query: " KdBx ",
                indexes: Array.from({
                    length: 50
                }, (_, i) => i * 4 + 2)
            },
            {
                tag: "entry-type",
                query: "Identities",
                indexes: Array.from({
                    length: 100
                }, (_, i) => i * 2 + 1)
            }
        ];
    }

    function test_search_filters_the_entire_catalog(row) {
        const panel = createPanel();
        const selected = spyFor(panel, "importerSelected");
        const requested = spyFor(panel, "chooseFileRequested");
        const search = child(panel, "importerSearch");
        const list = child(panel, "importerList");
        const expectedIds = row.indexes.map(index => panel.importers[index].id);

        search.text = row.query;
        tryCompare(list, "count", expectedIds.length);
        compare(panel.filteredImporters.map(importer => importer.id), expectedIds);
        compare(delegateAt(list, 0).modelData.id, expectedIds[0]);
        compare(panel.importers.length, 200);
        compare(panel.selectedImporterId, "");
        compare(panel.selectedImporter, null);
        compare(selected.count, 0);
        compare(requested.count, 0);
        verify(!child(panel, "chooseImportFile").enabled);

        search.text = "";
        tryCompare(list, "count", 200);
        compare(panel.filteredImporters.map(importer => importer.id), panel.importers.map(importer => importer.id));
        compare(panel.selectedImporterId, "");
    }

    function test_empty_results_are_safe_data() {
        return [
            {
                tag: "empty-catalog",
                emptyCatalog: true,
                query: ""
            },
            {
                tag: "unmatched-search",
                emptyCatalog: false,
                query: "No such manager or format"
            },
            {
                tag: "no-assumed-proton",
                emptyCatalog: false,
                query: "Proton"
            }
        ];
    }

    function test_empty_results_are_safe(row) {
        const panel = createPanel(row.emptyCatalog ? [] : fakeImporters());
        const selected = spyFor(panel, "importerSelected");
        const requested = spyFor(panel, "chooseFileRequested");
        const search = child(panel, "importerSearch");
        const list = child(panel, "importerList");
        const choose = child(panel, "chooseImportFile");

        search.text = row.query;
        tryCompare(list, "count", 0);
        compare(panel.filteredImporters.length, 0);
        verify(!choose.visible && !choose.enabled);
        search.forceActiveFocus();
        keyClick(Qt.Key_Down);
        compare(list.currentIndex, -1);
        list.forceActiveFocus();
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Enter);
        panel.selectImporter("proton");

        compare(panel.selectedImporterId, "");
        compare(panel.selectedImporter, null);
        compare(selected.count, 0);
        compare(requested.count, 0);

        search.text = "";
        tryCompare(list, "count", row.emptyCatalog ? 0 : 200);
        compare(panel.selectedImporter, null);
        verify(!choose.enabled);
    }

    function test_busy_blocks_actions_and_selection_data() {
        return [
            {
                tag: "without-selection",
                selected: false
            },
            {
                tag: "with-selection",
                selected: true
            }
        ];
    }

    function test_busy_blocks_actions_and_selection(row) {
        const panel = createPanel();
        const selected = spyFor(panel, "importerSelected");
        const requested = spyFor(panel, "chooseFileRequested");
        const search = child(panel, "importerSearch");
        const list = child(panel, "importerList");
        const choose = child(panel, "chooseImportFile");
        const first = panel.importers[190];
        const next = panel.importers[191];

        search.text = "Catalog Manager 19";
        tryCompare(list, "count", 10);
        if (row.selected)
            mouseClick(delegateAt(list, 0));
        compare(panel.selectedImporterId, row.selected ? first.id : "");
        selected.clear();
        const nextDelegate = delegateAt(list, 1);
        verify(waitForPolish(panel));

        panel.busy = true;
        verify(!search.enabled && !list.enabled && !choose.enabled);
        verify(!nextDelegate.enabled);
        mouseClick(nextDelegate);
        if (row.selected)
            mouseClick(choose);
        panel.selectImporter(next.id);
        panel.resetSelection();

        compare(panel.selectedImporterId, row.selected ? first.id : "");
        compare(panel.selectedImporter, row.selected ? first : null);
        compare(search.text, "Catalog Manager 19");
        compare(list.count, 10);
        compare(selected.count, 0);
        compare(requested.count, 0);

        panel.busy = false;
        verify(search.enabled && list.enabled && nextDelegate.enabled);
        mouseClick(nextDelegate);
        compare(panel.selectedImporterId, next.id);
        compare(panel.selectedImporter.fileFilters, next.fileFilters);
        compare(selected.count, 1);
        compare(Array.from(selected.signalArguments[0]), [next.id]);
        verify(waitForPolish(panel));
        verify(choose.visible && choose.enabled);
        mouseClick(choose);
        compare(requested.count, 1);
        compare(Array.from(requested.signalArguments[0]), [next.id]);
    }

    function test_large_catalog_keeps_live_delegates_bounded_while_scrolling() {
        const panel = createPanel();
        const selected = spyFor(panel, "importerSelected");
        const list = child(panel, "importerList");
        compare(list.count, 200);
        const indexes = [0, 99, 199, 0];

        for (let step = 0; step < indexes.length; ++step) {
            const index = indexes[step];
            list.positionViewAtIndex(index, ListView.Center);
            const delegate = delegateAt(list, index);
            const count = liveDelegateCount(list);
            verify(count > 0 && count < panel.importers.length / 2, "ListView should keep substantially fewer than 200 live delegates; found " + count);
            verify(list.contentHeight > list.height);
            compare(delegate.modelData.id, panel.importers[index].id);

            mouseClick(delegate);
            compare(panel.selectedImporterId, panel.importers[index].id);
            compare(panel.selectedImporter.fileFilters, panel.importers[index].fileFilters);
            compare(selected.count, step + 1);
            compare(Array.from(selected.signalArguments[step]), [panel.importers[index].id]);
        }
    }
}
