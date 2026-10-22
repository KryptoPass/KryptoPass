pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "Importers"
    width: 800
    height: 600
    visible: true
    when: windowShown

    Component {
        id: controllerComponent
        QtObject {
            property bool busy: false
            property bool reviewed: false
            property string summaryJson: ""
            property string warningsJson: ""
            property string status: ""
            property string errorMessage: ""
            property string importersJson: "[]"
            property var reviews: []
            property var imports: []
            property int resets: 0
            signal importCompleted
            function reset() {
                resets += 1;
                reviewed = false;
                summaryJson = "";
                warningsJson = "";
            }
            function clearSensitiveState() {
                reset();
            }
            function review(input, importerId) {
                reviews = reviews.concat([
                    {
                        input: input.toString(),
                        importerId: importerId
                    }
                ]);
            }
            function importReviewed(recipients) {
                imports = imports.concat([recipients]);
            }
        }
    }

    Component {
        id: keyStoreComponent
        QtObject {
            property var controller: QtObject {
                property bool operationBusy: false
            }
            property var entries: [
                {
                    id: "AAAA",
                    title: "Propia",
                    email: "own@example.org",
                    statusCode: "valid",
                    canEncrypt: true,
                    hasSecret: true
                },
                {
                    id: "BBBB",
                    title: "Contacto",
                    email: "contact@example.org",
                    statusCode: "valid",
                    canEncrypt: true,
                    hasSecret: false
                },
                {
                    id: "CCCC",
                    title: "Caducada",
                    email: "old@example.org",
                    statusCode: "expired",
                    canEncrypt: true,
                    hasSecret: true
                }
            ]
        }
    }

    Component {
        id: windowComponent
        ImportWindow {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign/);
    }

    function importers(count) {
        const result = [];
        for (let index = 0; index < count; ++index)
            result.push({
                id: "org.kryptopass.tests.importer" + index,
                name: "Gestor " + index,
                formats: ["json", "zip"],
                fileFilters: ["Gestor " + index + " (*.json *.zip)"],
                entryTypes: [],
                capabilities: []
            });
        return result;
    }

    function open(count) {
        const controller = createTemporaryObject(controllerComponent, testCase, {
            importersJson: JSON.stringify(importers(count === undefined ? 3 : count))
        });
        const keyStore = createTemporaryObject(keyStoreComponent, testCase);
        const window = createTemporaryObject(windowComponent, testCase, {
            controller: controller,
            keyStore: keyStore
        });
        verify(window !== null);
        window.prepare();
        tryVerify(() => window.visible);
        return {
            window: window,
            controller: controller
        };
    }

    function one(window, name) {
        const item = findChild(window, name);
        verify(item !== null, "Could not find " + name);
        return item;
    }

    function visibleTiles(window) {
        const grid = one(window, "importerChoice");
        return grid.visible ? grid.children.filter(tile => tile.objectName === "importerTile" && tile.visible) : [];
    }

    function test_starts_on_provider_grid_without_selection() {
        const fixture = open(3);
        const window = fixture.window;
        compare(window.selectedImporter, null);
        tryCompare(visibleTiles(window), "length", 3);
        verify(!one(window, "chooseImportFile").visible);
        verify(!one(window, "confirmImportWindow").enabled);
        compare(window.hint, "Elige un gestor para continuar");
    }

    function test_single_provider_is_still_chosen_explicitly() {
        const fixture = open(1);
        compare(fixture.window.selectedImporter, null);
        tryCompare(visibleTiles(fixture.window), "length", 1);
    }

    function test_choosing_and_changing_provider() {
        const fixture = open(3);
        const window = fixture.window;
        const resetsBefore = fixture.controller.resets;
        mouseClick(visibleTiles(window)[1]);
        compare(window.selectedImporterId, "org.kryptopass.tests.importer1");
        compare(fixture.controller.resets, resetsBefore + 1);
        tryVerify(() => visibleTiles(window).length === 0);
        tryVerify(() => one(window, "chooseImportFile").visible);
        compare(window.hint, "Elige el archivo exportado para continuar");

        window.selectImporter("");
        compare(window.selectedImporter, null);
        tryCompare(visibleTiles(window), "length", 3);
    }

    function test_file_is_reviewed_with_the_selected_provider() {
        const fixture = open(3);
        const window = fixture.window;
        window.chooseFile("file:///tmp/ignored.json");
        compare(fixture.controller.reviews.length, 0, "No file may be reviewed before a provider is chosen");

        window.selectImporter("org.kryptopass.tests.importer2");
        window.chooseFile("file:///tmp/export.zip");
        compare(fixture.controller.reviews.length, 1);
        compare(fixture.controller.reviews[0].importerId, "org.kryptopass.tests.importer2");
        compare(fixture.controller.reviews[0].input, "file:///tmp/export.zip");
        compare(window.fileName(window.inputPath), "export.zip");
    }

    function test_review_splits_summary_losses_and_keys() {
        const fixture = open(1);
        const window = fixture.window;
        const controller = fixture.controller;
        window.selectImporter("org.kryptopass.tests.importer0");
        window.chooseFile("file:///tmp/export.json");
        controller.summaryJson = JSON.stringify({
            types: [
                {
                    kind: "login",
                    label: "Inicios de sesión",
                    count: 3
                },
                {
                    kind: "note",
                    label: "Notas",
                    count: 0
                }
            ],
            folders: 2,
            items: 3,
            trashed: 1,
            skipped: 0
        });
        controller.warningsJson = JSON.stringify([
            {
                text: "1 elementos tienen adjuntos que no se importarán.",
                loss: true
            },
            {
                text: "3 elementos guardarán sus fechas originales como campos.",
                loss: false
            }
        ]);
        controller.reviewed = true;

        compare(window.losses.length, 1);
        compare(window.conversions.length, 1);
        compare(window.selectedFingerprints, ["AAAA"], "The only valid own key is preselected");
        compare(window.availableKeys.map(key => key.id), ["AAAA", "BBBB"], "Expired keys are not offered");
        verify(!window.keysExpanded, "A preselected key keeps the key list folded");

        const confirm = one(window, "confirmImportWindow");
        tryVerify(() => confirm.enabled);
        mouseClick(confirm);
        compare(controller.imports.length, 1);
        compare(JSON.parse(controller.imports[0]), ["AAAA"]);
    }

    function test_review_without_selected_key_unfolds_keys_and_blocks_import() {
        const fixture = open(1);
        const window = fixture.window;
        window.selectImporter("org.kryptopass.tests.importer0");
        window.chooseFile("file:///tmp/export.json");
        window.selectedFingerprints = [];
        fixture.controller.summaryJson = JSON.stringify({
            types: [],
            folders: 0,
            items: 0,
            trashed: 0,
            skipped: 0
        });
        fixture.controller.reviewed = true;
        verify(window.keysExpanded);
        verify(!one(window, "confirmImportWindow").enabled);
        compare(window.hint, "Elige al menos una clave");
    }

    function test_completion_shows_close_only() {
        const fixture = open(1);
        const window = fixture.window;
        window.selectImporter("org.kryptopass.tests.importer0");
        fixture.controller.importCompleted();
        verify(window.completed);
        verify(!one(window, "confirmImportWindow").visible);
    }
}
