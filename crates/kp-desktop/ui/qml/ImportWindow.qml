pragma ComponentBehavior: Bound
import QtQuick
import QtCore
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami

Controls.ApplicationWindow {
    id: dialog
    required property var keyStore
    /// Vault primary key, preselected when it can still encrypt.
    property string primaryKey: ""
    required property var controller
    readonly property bool opened: visible
    readonly property bool busy: dialog.controller.busy || keyStore.controller.operationBusy
    readonly property var importers: JSON.parse(dialog.controller.importersJson || "[]")
    property string selectedImporterId: ""
    readonly property var selectedImporter: importers.find(importer => importer.id === selectedImporterId) || null
    readonly property bool hasFile: !!inputPath.toString()
    readonly property bool reviewed: dialog.controller.reviewed && !completed
    readonly property var summary: JSON.parse(dialog.controller.summaryJson || "{}")
    readonly property var notes: JSON.parse(dialog.controller.warningsJson || "[]")
    readonly property var losses: notes.filter(note => note.loss)
    readonly property var conversions: notes.filter(note => !note.loss)
    property url inputPath: ""
    property bool completed: false
    property bool keysExpanded: false
    property bool detailsExpanded: false
    property var selectedFingerprints: []
    readonly property var availableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property var selectedKeys: availableKeys.filter(key => selectedFingerprints.includes(key.id))
    readonly property bool eligibleSelection: selectedFingerprints.length > 0 && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))
    readonly property string hint: completed ? "" : !selectedImporter ? qsTr("Elige un gestor para continuar") : !hasFile ? qsTr("Elige el archivo exportado para continuar") : !reviewed ? "" : !eligibleSelection ? qsTr("Elige al menos una clave") : qsTr("%n elemento", "", summary.items || 0)
    signal imported

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint | Qt.WindowCloseButtonHint
    modality: Qt.WindowModal
    width: Kirigami.Units.gridUnit * 30
    height: Kirigami.Units.gridUnit * 30
    minimumWidth: Kirigami.Units.gridUnit * 24
    minimumHeight: Kirigami.Units.gridUnit * 20
    title: qsTr("Importar desde otro gestor")

    function prepare() {
        if (busy)
            return;
        if (visible) {
            raise();
            requestActivate();
            return;
        }
        clearReview();
        selectedImporterId = "";
        const primary = availableKeys.find(key => key.id.toUpperCase() === primaryKey.toUpperCase());
        const ownKeys = availableKeys.filter(key => key.hasSecret);
        selectedFingerprints = primary ? [primary.id] : ownKeys.length === 1 ? [ownKeys[0].id] : [];
        x = screen.virtualX + Math.round((screen.width - width) / 2);
        y = screen.virtualY + Math.round((screen.height - height) / 2);
        show();
        requestActivate();
    }

    function clearReview() {
        dialog.controller.reset();
        inputPath = "";
        completed = false;
        keysExpanded = false;
        detailsExpanded = false;
    }

    function clearSensitiveState() {
        dialog.controller.clearSensitiveState();
        inputPath = "";
        completed = false;
        selectedFingerprints = [];
        sourceDialog.close();
        hide();
    }

    function selectImporter(importerId) {
        if (busy)
            return;
        clearReview();
        selectedImporterId = importerId;
    }

    function browse() {
        if (!selectedImporter || busy)
            return;
        sourceDialog.importerId = selectedImporter.id;
        sourceDialog.title = qsTr("Seleccionar exportación de %1").arg(selectedImporter.name);
        sourceDialog.nameFilters = selectedImporter.fileFilters;
        sourceDialog.open();
    }

    function chooseFile(url) {
        if (!opened || busy || !selectedImporter)
            return;
        clearReview();
        inputPath = url;
        controller.review(url, selectedImporter.id);
    }

    function displayPath(path) {
        return decodeURIComponent(path.toString().replace(/^file:\/\//, ""));
    }

    function fileName(path) {
        const parts = displayPath(path).split("/");
        return parts[parts.length - 1];
    }

    function keyName(key) {
        return (key.title || key.email) + (key.hasSecret ? qsTr(" (tu clave)") : "");
    }

    onClosing: function (event) {
        if (busy)
            event.accepted = false;
    }
    onVisibleChanged: {
        if (!visible && !busy) {
            sourceDialog.close();
            clearReview();
        }
    }
    onReviewedChanged: {
        if (reviewed && !eligibleSelection)
            keysExpanded = true;
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.visible && !dialog.busy && !sourceDialog.visible
        onActivated: dialog.close()
    }
    Connections {
        target: dialog.controller
        function onImportCompleted() {
            dialog.completed = true;
            dialog.imported();
        }
    }

    // The chosen source is the header's subject; "Cambiar gestor" returns to the picker.
    header: PanelHeader {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                visible: !!dialog.selectedImporter
                source: dialog.selectedImporter?.iconSource || "document-import"
                fallback: "document-import"
                placeholder: "document-import"
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
            }
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 4
                text: dialog.selectedImporter ? qsTr("Desde %1").arg(dialog.selectedImporter.name) : qsTr("¿Desde qué gestor importas?")
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
            Controls.ToolButton {
                visible: !!dialog.selectedImporter && !dialog.completed
                enabled: !dialog.busy
                text: qsTr("Cambiar gestor")
                icon.name: "go-previous"
                onClicked: dialog.selectImporter("")
            }
        }
    }

    Controls.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            width: scroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: !dialog.selectedImporter
                text: qsTr("Gestores disponibles")
            }
            Flow {
                objectName: "importerChoice"
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: !dialog.selectedImporter
                spacing: Kirigami.Units.largeSpacing
                Repeater {
                    model: dialog.selectedImporter ? [] : dialog.importers
                    delegate: Controls.ItemDelegate {
                        id: tile
                        required property var modelData
                        objectName: "importerTile"
                        width: Kirigami.Units.gridUnit * 7
                        height: Kirigami.Units.gridUnit * 6
                        enabled: !dialog.busy
                        Accessible.name: modelData.name
                        onClicked: dialog.selectImporter(modelData.id)
                        contentItem: ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Item {
                                Layout.fillHeight: true
                            }
                            Kirigami.Icon {
                                Layout.alignment: Qt.AlignHCenter
                                source: tile.modelData.iconSource || "document-import"
                                fallback: "document-import"
                                placeholder: "document-import"
                                implicitWidth: Kirigami.Units.iconSizes.large
                                implicitHeight: Kirigami.Units.iconSizes.large
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                horizontalAlignment: Text.AlignHCenter
                                text: tile.modelData.name
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                            Item {
                                Layout.fillHeight: true
                            }
                        }
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: !dialog.selectedImporter && dialog.importers.length === 0
                text: qsTr("No hay integraciones de importación disponibles.")
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: !dialog.completed && !!dialog.selectedImporter
                text: qsTr("Archivo")
            }

            // Empty state, sized by its own content so nothing spills over the frame.
            DropArea {
                id: drop
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                implicitHeight: emptyFile.implicitHeight + Kirigami.Units.gridUnit * 2
                visible: !dialog.completed && !dialog.hasFile && !!dialog.selectedImporter
                enabled: !!dialog.selectedImporter && !dialog.busy
                onDropped: function (event) {
                    if (event.hasUrls)
                        dialog.chooseFile(event.urls[0]);
                }
                Rectangle {
                    anchors.fill: parent
                    radius: Kirigami.Units.cornerRadius
                    color: drop.containsDrag ? Qt.alpha(Kirigami.Theme.highlightColor, 0.1) : "transparent"
                    border.width: 1
                    border.color: drop.containsDrag ? Kirigami.Theme.highlightColor : Kirigami.ColorUtils.linearInterpolation(Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.25)
                }
                ColumnLayout {
                    id: emptyFile
                    anchors.centerIn: parent
                    width: parent.width - Kirigami.Units.gridUnit * 2
                    spacing: Kirigami.Units.smallSpacing
                    opacity: drop.enabled ? 1 : 0.6
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignHCenter
                        source: dialog.selectedImporter?.iconSource || "document-import"
                        fallback: "document-import"
                        placeholder: "document-import"
                        implicitWidth: Kirigami.Units.iconSizes.large
                        implicitHeight: Kirigami.Units.iconSizes.large
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        text: dialog.selectedImporter ? qsTr("Suelta aquí la exportación de %1").arg(dialog.selectedImporter.name) : qsTr("Suelta aquí la exportación")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        visible: !!dialog.selectedImporter
                        text: dialog.selectedImporter ? qsTr("Archivos admitidos: %1").arg(dialog.selectedImporter.formats.map(extension => "." + extension).join(", ")) : ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Button {
                        objectName: "chooseImportFile"
                        Layout.alignment: Qt.AlignHCenter
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        text: qsTr("Examinar…")
                        icon.name: "document-open"
                        enabled: drop.enabled
                        onClicked: dialog.browse()
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: !dialog.completed && dialog.hasFile
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "text-x-generic"
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.fileName(dialog.inputPath)
                        textFormat: Text.PlainText
                        font.weight: Font.DemiBold
                        elide: Text.ElideMiddle
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: dialog.displayPath(dialog.inputPath)
                        textFormat: Text.PlainText
                        color: Kirigami.Theme.disabledTextColor
                        elide: Text.ElideMiddle
                    }
                }
                Controls.ToolButton {
                    text: qsTr("Cambiar…")
                    icon.name: "document-open"
                    enabled: !dialog.busy
                    onClicked: dialog.browse()
                }
            }
            // Errors belong to the file that caused them.
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: !!dialog.controller.errorMessage
                type: Kirigami.MessageType.Error
                text: dialog.controller.errorMessage
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.reviewed
                text: qsTr("Se importará")
            }
            Repeater {
                model: dialog.reviewed ? (dialog.summary.types || []).concat([
                    {
                        kind: "folder",
                        label: qsTr("Carpetas"),
                        count: dialog.summary.folders || 0
                    }
                ]).filter(row => row.count > 0) : []
                delegate: RowLayout {
                    id: countRow
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.largeSpacing
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing
                    Kirigami.Icon {
                        source: countRow.modelData.kind === "folder" ? "folder" : (EntryTypes.find(countRow.modelData.kind) || {
                                icon: "view-list-details"
                            }).icon
                        implicitWidth: Kirigami.Units.iconSizes.smallMedium
                        implicitHeight: Kirigami.Units.iconSizes.smallMedium
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: countRow.modelData.label
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                    }
                    Controls.Label {
                        text: Number(countRow.modelData.count).toLocaleString(Qt.locale(), "f", 0)
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.reviewed && ((dialog.summary.trashed || 0) > 0 || (dialog.summary.skipped || 0) > 0)
                text: qsTr("Incluye %1 en la papelera · %2 omitidos").arg(dialog.summary.trashed || 0).arg(dialog.summary.skipped || 0)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.reviewed && dialog.losses.length > 0
                type: Kirigami.MessageType.Warning
                text: dialog.losses.map(note => note.text).join("\n")
            }

            Kirigami.ListSectionHeader {
                Layout.fillWidth: true
                visible: dialog.reviewed
                text: qsTr("Cifrar con")
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: dialog.reviewed && !dialog.keysExpanded
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: "view-certificate"
                    implicitWidth: Kirigami.Units.iconSizes.smallMedium
                    implicitHeight: Kirigami.Units.iconSizes.smallMedium
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: dialog.selectedKeys.map(key => dialog.keyName(key)).join(", ")
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Controls.ToolButton {
                    text: qsTr("Cambiar…")
                    icon.name: "document-edit"
                    enabled: !dialog.busy
                    onClicked: dialog.keysExpanded = true
                }
            }
            Repeater {
                model: dialog.reviewed && dialog.keysExpanded ? dialog.availableKeys : []
                delegate: Controls.CheckDelegate {
                    required property var modelData
                    Layout.fillWidth: true
                    text: dialog.keyName(modelData)
                    enabled: !dialog.busy
                    checked: dialog.selectedFingerprints.includes(modelData.id)
                    Accessible.description: modelData.id
                    onToggled: {
                        const selected = dialog.selectedFingerprints.filter(fingerprint => fingerprint !== modelData.id);
                        if (checked)
                            selected.push(modelData.id);
                        dialog.selectedFingerprints = selected;
                    }
                }
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.reviewed && dialog.availableKeys.length === 0
                text: qsTr("No hay claves válidas para cifrar. Crea o importa una clave desde Claves y vuelve a intentarlo.")
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: dialog.reviewed && dialog.eligibleSelection && !dialog.selectedKeys.some(key => key.hasSecret)
                type: Kirigami.MessageType.Warning
                text: qsTr("Para leer los datos necesitarás la clave privada de alguien con acceso.")
            }

            // Informational conversions stay folded; they need no decision.
            Controls.ToolButton {
                Layout.leftMargin: Kirigami.Units.smallSpacing
                visible: dialog.reviewed && dialog.conversions.length > 0
                text: qsTr("Detalles de la conversión (%1)").arg(dialog.conversions.length)
                icon.name: dialog.detailsExpanded ? "arrow-down" : "arrow-right"
                onClicked: dialog.detailsExpanded = !dialog.detailsExpanded
            }
            Repeater {
                model: dialog.reviewed && dialog.detailsExpanded ? dialog.conversions : []
                delegate: Controls.Label {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.leftMargin: Kirigami.Units.gridUnit * 2
                    Layout.rightMargin: Kirigami.Units.largeSpacing
                    text: modelData.text
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
            }

            Kirigami.PlaceholderMessage {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.gridUnit * 4
                visible: dialog.completed
                icon.name: "dialog-ok-apply"
                text: qsTr("Importación completada")
                explanation: qsTr("Los elementos ya están en la bóveda actual.")
            }

            Item {
                Layout.preferredHeight: Kirigami.Units.largeSpacing
            }
        }
    }

    footer: PanelFooter {
        headerHeight: Kirigami.Units.gridUnit * 2.5
        RowLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing
            Controls.BusyIndicator {
                running: dialog.controller.busy
                visible: running
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: Kirigami.Units.iconSizes.smallMedium
                Accessible.name: dialog.controller.status
            }
            Controls.Label {
                Layout.fillWidth: true
                text: dialog.controller.busy ? dialog.controller.status : dialog.hint
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                elide: Text.ElideRight
            }
            Controls.Button {
                objectName: "closeImportWindow"
                text: dialog.completed ? qsTr("Cerrar") : qsTr("Cancelar")
                icon.name: dialog.completed ? "dialog-close" : "dialog-cancel"
                enabled: !dialog.busy
                onClicked: dialog.close()
            }
            Controls.Button {
                objectName: "confirmImportWindow"
                visible: !dialog.completed
                text: qsTr("Importar")
                icon.name: "document-import"
                enabled: dialog.reviewed && dialog.eligibleSelection && !dialog.busy
                onClicked: dialog.controller.importReviewed(JSON.stringify(dialog.selectedFingerprints))
            }
        }
    }

    Dialogs.FileDialog {
        id: sourceDialog
        objectName: "importSourceDialog"
        property string importerId: ""
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (!dialog.selectedImporter || importerId !== dialog.selectedImporter.id)
                return;
            dialog.chooseFile(selectedFile);
        }
    }
}
