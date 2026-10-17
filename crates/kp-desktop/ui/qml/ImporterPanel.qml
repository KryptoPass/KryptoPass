pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

RowLayout {
    id: panel
    property var importers: []
    property bool busy: false
    property string selectedImporterId: ""
    property Component reviewContent
    readonly property var selectedImporter: importers.find(importer => importer.id === selectedImporterId) || null
    readonly property var filteredImporters: {
        const query = search.text.trim().toLocaleLowerCase();
        return importers.filter(importer => [importer.name, importer.formats.join(" "), importer.entryTypes.join(" ")]
            .join(" ").toLocaleLowerCase().includes(query));
    }
    signal importerSelected(string importerId)
    signal chooseFileRequested(string importerId)
    spacing: Kirigami.Units.largeSpacing

    function selectImporter(importerId) {
        if (busy || !importers.some(importer => importer.id === importerId) || selectedImporterId === importerId)
            return;
        selectedImporterId = importerId;
        importerSelected(importerId);
    }

    function resetSelection() {
        if (busy)
            return;
        search.text = "";
        selectedImporterId = "";
        providers.currentIndex = -1;
    }

    ColumnLayout {
        Layout.preferredWidth: Math.max(180, Math.min(260, panel.width * 0.3))
        Layout.maximumWidth: 280
        Layout.fillHeight: true
        Controls.Label {
            text: qsTr("Integraciones")
            font.weight: Font.DemiBold
        }
        Kirigami.SearchField {
            id: search
            objectName: "importerSearch"
            Layout.fillWidth: true
            enabled: !panel.busy
            placeholderText: qsTr("Buscar gestor o formato…")
            Accessible.name: qsTr("Buscar integraciones de importación")
            Keys.onDownPressed: {
                if (providers.count > 0) {
                    providers.currentIndex = 0;
                    providers.forceActiveFocus();
                }
            }
        }
        Controls.Label {
            text: qsTr("%1 de %2 integraciones").arg(panel.filteredImporters.length).arg(panel.importers.length)
            color: Kirigami.Theme.disabledTextColor
        }
        Controls.Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            padding: 0
            contentItem: Item {
                ListView {
                    id: providers
                    objectName: "importerList"
                    anchors.fill: parent
                    clip: true
                    model: panel.filteredImporters
                    currentIndex: -1
                    enabled: !panel.busy
                    reuseItems: true
                    keyNavigationEnabled: true
                    Controls.ScrollBar.vertical: Controls.ScrollBar {}
                    Keys.onReturnPressed: {
                        if (currentIndex >= 0)
                            panel.selectImporter(panel.filteredImporters[currentIndex].id);
                    }
                    Keys.onEnterPressed: {
                        if (currentIndex >= 0)
                            panel.selectImporter(panel.filteredImporters[currentIndex].id);
                    }
                    delegate: Controls.ItemDelegate {
                        id: providerDelegate
                        required property var modelData
                        width: ListView.view.width
                        objectName: "importerDelegate"
                        highlighted: modelData.id === panel.selectedImporterId
                        Accessible.name: modelData.name
                        onClicked: panel.selectImporter(modelData.id)
                        contentItem: ColumnLayout {
                            Controls.Label {
                                Layout.fillWidth: true
                                text: providerDelegate.modelData.name
                                textFormat: Text.PlainText
                                font.weight: Font.DemiBold
                                elide: Text.ElideRight
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: providerDelegate.modelData.formats.map(extension => "." + extension).join(" · ")
                                textFormat: Text.PlainText
                                color: Kirigami.Theme.disabledTextColor
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
                Kirigami.PlaceholderMessage {
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.largeSpacing * 2
                    visible: providers.count === 0
                    text: panel.importers.length === 0
                        ? qsTr("No hay integraciones disponibles.") : qsTr("No hay coincidencias.")
                }
            }
        }
    }

    Controls.ScrollView {
        id: details
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: details.availableWidth
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Heading {
                Layout.fillWidth: true
                text: panel.selectedImporter ? panel.selectedImporter.name : qsTr("Elige una integración")
                textFormat: Text.PlainText
                level: 2
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !panel.selectedImporter
                text: qsTr("Selecciona el gestor del que exportaste tus datos para consultar sus formatos y capacidades.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!panel.selectedImporter
                text: panel.selectedImporter ? qsTr("Formatos: %1").arg(panel.selectedImporter.formats.map(extension => "." + extension).join(" · ")) : ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!panel.selectedImporter
                text: panel.selectedImporter ? panel.selectedImporter.entryTypes.join(" · ") : ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Repeater {
                model: panel.selectedImporter ? panel.selectedImporter.capabilities : []
                delegate: RowLayout {
                    id: capabilityRow
                    required property var modelData
                    Layout.fillWidth: true
                    Controls.Label {
                        Layout.fillWidth: true
                        text: capabilityRow.modelData.label
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        text: capabilityRow.modelData.support === "supported" ? qsTr("Completo")
                            : capabilityRow.modelData.support === "reference" ? qsTr("Solo referencia") : qsTr("No disponible")
                        color: capabilityRow.modelData.support === "unsupported" ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.textColor
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!panel.selectedImporter && panel.selectedImporter.requiresDecryptor && panel.selectedImporter.decryptionAvailable
                text: qsTr("Si el archivo está cifrado, la integración solicitará el acceso mediante su servicio de descifrado.")
                wrapMode: Text.Wrap
            }
            Controls.Button {
                objectName: "chooseImportFile"
                visible: !!panel.selectedImporter
                enabled: !!panel.selectedImporter && !panel.busy
                text: qsTr("Elegir archivo…")
                icon.name: "document-open"
                onClicked: panel.chooseFileRequested(panel.selectedImporterId)
            }
            Loader {
                Layout.fillWidth: true
                visible: !!panel.selectedImporter
                sourceComponent: panel.reviewContent
            }
        }
    }
}
