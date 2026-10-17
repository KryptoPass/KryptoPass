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
    required property var controller
    readonly property bool opened: visible
    readonly property bool busy: dialog.controller.busy || keyStore.controller.operationBusy
    property url inputPath: ""
    property bool completed: false
    property var selectedFingerprints: []
    readonly property var availableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property bool eligibleSelection: selectedFingerprints.length > 0
        && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))
    signal imported()

    flags: Qt.Dialog | Qt.WindowTitleHint | Qt.WindowSystemMenuHint
        | Qt.WindowCloseButtonHint | Qt.WindowMaximizeButtonHint
    modality: Qt.WindowModal
    width: Math.min(1040, screen.width - Kirigami.Units.gridUnit * 2)
    height: Math.min(760, screen.height - Kirigami.Units.gridUnit * 2)
    minimumWidth: 640
    minimumHeight: 440
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
        picker.resetSelection();
        const ownKeys = availableKeys.filter(key => key.hasSecret);
        selectedFingerprints = ownKeys.length === 1 ? [ownKeys[0].id] : [];
        x = screen.virtualX + Math.round((screen.width - width) / 2);
        y = screen.virtualY + Math.round((screen.height - height) / 2);
        show();
        requestActivate();
    }

    function clearReview() {
        dialog.controller.reset();
        inputPath = "";
        completed = false;
    }

    function clearSensitiveState() {
        dialog.controller.clearSensitiveState();
        inputPath = "";
        completed = false;
        selectedFingerprints = [];
        sourceDialog.close();
        hide();
    }

    onClosing: function(event) {
        if (busy)
            event.accepted = false;
    }
    onVisibleChanged: {
        if (!visible && !busy) {
            sourceDialog.close();
            clearReview();
            picker.resetSelection();
        }
    }

    Shortcut {
        sequence: StandardKey.Cancel
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

    ImporterPanel {
        id: picker
        objectName: "importerPanel"
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        importers: JSON.parse(dialog.controller.importersJson || "[]")
        busy: dialog.busy
        onImporterSelected: dialog.clearReview()
        onChooseFileRequested: function(importerId) {
            if (!selectedImporter || importerId !== selectedImporter.id)
                return;
            sourceDialog.importerId = importerId;
            sourceDialog.title = qsTr("Seleccionar exportación de %1").arg(selectedImporter.name);
            sourceDialog.nameFilters = selectedImporter.fileFilters;
            sourceDialog.open();
        }
        reviewContent: Component {
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Los elementos se añadirán a la bóveda actual y se protegerán con las claves que elijas. Primero se revisará el archivo.")
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!dialog.inputPath.toString()
                    text: decodeURIComponent(dialog.inputPath.toString().replace(/^file:\/\//, ""))
                    textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!dialog.controller.summary
                    text: dialog.controller.summary
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!dialog.controller.warnings
                    type: Kirigami.MessageType.Warning
                    text: dialog.controller.warnings
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: dialog.controller.reviewed
                    text: qsTr("Claves que protegerán los elementos importados")
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: dialog.controller.reviewed ? dialog.availableKeys : []
                    delegate: Controls.CheckBox {
                        required property var modelData
                        Layout.fillWidth: true
                        text: (modelData.title || modelData.email) + (modelData.hasSecret ? qsTr(" (tu clave)") : "")
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
                    visible: dialog.controller.reviewed && !dialog.eligibleSelection
                    text: dialog.availableKeys.length > 0
                        ? qsTr("Elige al menos una clave para continuar.")
                        : qsTr("No hay claves válidas para cifrar. Crea o importa una clave desde Claves y vuelve a intentarlo.")
                    wrapMode: Text.Wrap
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: dialog.controller.reviewed && dialog.eligibleSelection
                        && !dialog.availableKeys.some(key => key.hasSecret && dialog.selectedFingerprints.includes(key.id))
                    type: Kirigami.MessageType.Warning
                    text: qsTr("Para leer los datos necesitarás la clave privada de alguien con acceso.")
                }
                Controls.BusyIndicator {
                    Layout.alignment: Qt.AlignHCenter
                    running: dialog.controller.busy
                    visible: running
                    Accessible.name: dialog.controller.status
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !!dialog.controller.status
                    text: dialog.controller.status
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: !!dialog.controller.errorMessage
                    type: Kirigami.MessageType.Error
                    text: dialog.controller.errorMessage
                }
            }
        }
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            objectName: "closeImportWindow"
            text: qsTr("Cerrar")
            enabled: !dialog.busy
            onClicked: dialog.close()
        }
        Controls.Button {
            objectName: "confirmImportWindow"
            visible: !dialog.completed
            text: qsTr("Importar")
            icon.name: "document-import"
            enabled: dialog.controller.reviewed && dialog.eligibleSelection && !dialog.busy
            onClicked: dialog.controller.importReviewed(JSON.stringify(dialog.selectedFingerprints))
        }
    }
    Dialogs.FileDialog {
        id: sourceDialog
        objectName: "importSourceDialog"
        property string importerId: ""
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        onAccepted: {
            if (!dialog.opened || dialog.busy || importerId !== picker.selectedImporterId)
                return;
            dialog.clearReview();
            dialog.inputPath = selectedFile;
            dialog.controller.review(selectedFile, importerId);
        }
    }
}
