pragma ComponentBehavior: Bound
import QtQuick
import QtCore
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.Dialog {
    id: dialog
    required property KeyStore keyStore
    readonly property bool busy: backend.busy || keyStore.controller.operationBusy
    property url inputPath: ""
    property bool completed: false
    property var selectedFingerprints: []
    readonly property var availableKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt)
    readonly property bool eligibleSelection: selectedFingerprints.length > 0
        && selectedFingerprints.every(fingerprint => availableKeys.some(key => key.id === fingerprint))
    signal imported()

    anchors.centerIn: parent
    width: Math.min(620, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    modal: true
    title: qsTr("Importar desde Proton Pass")
    closePolicy: busy ? Controls.Popup.NoAutoClose : Controls.Popup.CloseOnEscape

    function prepare() {
        if (busy)
            return;
        backend.reset();
        inputPath = "";
        completed = false;
        const ownKeys = availableKeys.filter(key => key.hasSecret);
        selectedFingerprints = ownKeys.length === 1 ? [ownKeys[0].id] : [];
        open();
    }
    onClosed: backend.reset()

    Backend.ImportController {
        id: backend
        onImportCompleted: {
            dialog.completed = true;
            dialog.imported();
        }
    }

    contentItem: Controls.ScrollView {
        implicitWidth: 540
        implicitHeight: Math.min(body.implicitHeight, 520)
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            id: body
            width: parent.width
            spacing: Kirigami.Units.largeSpacing
            Controls.Label {
                Layout.fillWidth: true
                visible: !dialog.completed
                text: qsTr("Importa una exportación JSON, ZIP o PGP. Los elementos se añadirán a la bóveda actual y se protegerán con las claves que elijas. Si el archivo está cifrado, GnuPG pedirá su contraseña.")
                wrapMode: Text.Wrap
            }
            Controls.Button {
                visible: !dialog.completed
                text: qsTr("Seleccionar exportación…")
                icon.name: "document-open"
                enabled: !dialog.busy
                onClicked: sourceDialog.open()
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!dialog.inputPath.toString()
                text: decodeURIComponent(dialog.inputPath.toString().replace(/^file:\/\//, ""))
                textFormat: Text.PlainText
                wrapMode: Text.WrapAnywhere
            }
            Controls.Button {
                visible: !dialog.completed && !!dialog.inputPath.toString()
                text: qsTr("Revisar archivo")
                enabled: !dialog.busy
                onClicked: backend.review(dialog.inputPath)
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!backend.summary
                text: backend.summary
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!backend.warnings
                type: Kirigami.MessageType.Warning
                text: backend.warnings
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: backend.reviewed
                text: qsTr("Claves que protegerán los elementos importados")
                font.weight: Font.DemiBold
                wrapMode: Text.Wrap
            }
            Repeater {
                model: backend.reviewed ? dialog.availableKeys : []
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
                visible: backend.reviewed && !dialog.eligibleSelection
                text: dialog.availableKeys.length > 0
                    ? qsTr("Elige al menos una clave para continuar.")
                    : qsTr("No hay claves válidas para cifrar. Crea o importa una clave desde Claves y vuelve a intentarlo.")
                wrapMode: Text.Wrap
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: backend.reviewed && dialog.eligibleSelection
                    && !dialog.availableKeys.some(key => key.hasSecret && dialog.selectedFingerprints.includes(key.id))
                type: Kirigami.MessageType.Warning
                text: qsTr("Para leer los datos necesitarás la clave privada de alguien con acceso.")
            }
            Controls.BusyIndicator {
                Layout.alignment: Qt.AlignHCenter
                running: backend.busy
                visible: running
                Accessible.name: backend.status
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!backend.status
                text: backend.status
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!backend.errorMessage
                type: Kirigami.MessageType.Error
                text: backend.errorMessage
            }
        }
    }
    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cerrar")
            enabled: !dialog.busy
            onClicked: dialog.close()
        }
        Controls.Button {
            visible: !dialog.completed
            text: qsTr("Importar")
            icon.name: "document-import"
            enabled: backend.reviewed && dialog.eligibleSelection && !dialog.busy
            onClicked: backend.importReviewed(JSON.stringify(dialog.selectedFingerprints))
        }
    }
    Dialogs.FileDialog {
        id: sourceDialog
        title: qsTr("Seleccionar exportación de Proton Pass")
        fileMode: Dialogs.FileDialog.OpenFile
        currentFolder: StandardPaths.writableLocation(StandardPaths.DocumentsLocation)
        nameFilters: [qsTr("Exportación de Proton Pass (*.json *.zip *.pgp *.gpg)"), qsTr("Todos los archivos (*)")]
        onAccepted: {
            backend.reset();
            dialog.inputPath = selectedFile;
            backend.review(selectedFile);
        }
    }
}
