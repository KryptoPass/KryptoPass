pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretFields {
    id: root
    objectName: "customRecordsEditor"

    property var records: []
    property string notes: ""
    readonly property int maximumRecords: 100

    spacing: Kirigami.Units.largeSpacing

    onRecordsChanged: edited()
    onNotesChanged: edited()

    function createRecordId() {
        const bytes = [];
        for (let index = 0; index < 16; ++index)
            bytes.push(Math.floor(Math.random() * 256));
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        const hex = bytes.map(value => (value < 16 ? "0" : "") + value.toString(16));
        return hex.slice(0, 4).join("") + "-" + hex.slice(4, 6).join("") + "-" + hex.slice(6, 8).join("") + "-" + hex.slice(8, 10).join("") + "-" + hex.slice(10, 16).join("");
    }

    function addRecord() {
        if (recordModel.count >= maximumRecords)
            return;
        recordModel.append({
            recordId: createRecordId(),
            key: "",
            value: "",
            multiline: false,
            concealed: true
        });
        syncRecords();
        Qt.callLater(function () {
            const record = recordRepeater.itemAt(recordRepeater.count - 1);
            if (record)
                record.focusKeyField();
        });
    }

    function updateRecord(index, key, value) {
        if (index < 0 || index >= recordModel.count)
            return;
        if (["key", "value", "multiline", "concealed"].indexOf(key) < 0)
            return;
        recordModel.setProperty(index, key, value);
        syncRecords();
    }

    function removeRecord(index) {
        if (index < 0 || index >= recordModel.count)
            return;
        recordModel.remove(index);
        syncRecords();
    }

    function moveRecord(index, delta) {
        const target = index + delta;
        if (index < 0 || index >= recordModel.count || target < 0 || target >= recordModel.count)
            return;
        recordModel.move(index, target, 1);
        syncRecords();
    }

    function syncRecords() {
        const next = [];
        for (let index = 0; index < recordModel.count; ++index) {
            const record = recordModel.get(index);
            next.push({
                id: record.recordId,
                key: record.key,
                value: record.value,
                multiline: !!record.multiline,
                concealed: !!record.concealed
            });
        }
        records = next;
    }

    function loadFields(fields) {
        const input = Array.isArray(fields.records) ? fields.records : [];
        const normalized = input.map(record => ({
                    id: String(record.id || createRecordId()),
                    key: String(record.key === undefined || record.key === null ? "" : record.key),
                    value: String(record.value === undefined || record.value === null ? "" : record.value),
                    multiline: !!record.multiline,
                    concealed: record.concealed === undefined ? true : !!record.concealed
                }));
        recordModel.clear();
        for (const record of normalized) {
            recordModel.append({
                recordId: record.id,
                key: record.key,
                value: record.value,
                multiline: record.multiline,
                concealed: record.concealed
            });
        }
        records = normalized;
        notes = String(fields.notes || "");
        notesEditor.resetSource(notes);
    }

    function currentFields() {
        return {
            records: records.slice(),
            notes: notes
        };
    }

    function clearFields() {
        recordModel.clear();
        records = [];
        notes = "";
        notesEditor.clearSensitiveState();
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Campos personalizados")
            font.weight: Font.DemiBold
            wrapMode: Text.Wrap
        }
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Añade los datos que necesites. Los campos pueden repetirse y cambiar de orden.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        ListModel {
            id: recordModel
        }

        Repeater {
            id: recordRepeater
            model: recordModel

            delegate: Controls.Frame {
                id: recordFrame
                required property int index
                required property string recordId
                required property string key
                required property string value
                required property bool multiline
                required property bool concealed
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing

                function focusKeyField() {
                    recordKeyField.forceActiveFocus();
                }

                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        Controls.TextField {
                            id: recordKeyField
                            objectName: "customRecordKeyField"
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: recordFrame.key
                            placeholderText: qsTr("Nombre del campo")
                            Accessible.name: qsTr("Nombre del campo %1").arg(recordFrame.index + 1)
                            onTextEdited: root.updateRecord(recordFrame.index, "key", text)
                        }
                        Controls.ToolButton {
                            text: qsTr("Mover campo hacia arriba")
                            icon.name: "arrow-up"
                            display: Controls.AbstractButton.IconOnly
                            enabled: recordFrame.index > 0
                            Accessible.name: text
                            onClicked: root.moveRecord(recordFrame.index, -1)
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                        }
                        Controls.ToolButton {
                            text: qsTr("Mover campo hacia abajo")
                            icon.name: "arrow-down"
                            display: Controls.AbstractButton.IconOnly
                            enabled: recordFrame.index < recordModel.count - 1
                            Accessible.name: text
                            onClicked: root.moveRecord(recordFrame.index, 1)
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                        }
                        Controls.ToolButton {
                            text: qsTr("Quitar campo %1").arg(recordFrame.index + 1)
                            icon.name: "edit-delete"
                            display: Controls.AbstractButton.IconOnly
                            Accessible.name: text
                            onClicked: root.removeRecord(recordFrame.index)
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: text
                        }
                    }

                    SecretInput {
                        objectName: "customRecordValueField"
                        Layout.fillWidth: true
                        label: qsTr("Valor")
                        value: recordFrame.value
                        multiline: !!recordFrame.multiline || recordFrame.value.indexOf("\n") >= 0 || recordFrame.value.indexOf("\r") >= 0
                        concealed: !!recordFrame.concealed
                        minimumRows: 3
                        onValueEdited: function (value) {
                            root.updateRecord(recordFrame.index, "value", value);
                        }
                    }

                    RowLayout {
                        Layout.fillWidth: true
                        Controls.CheckBox {
                            text: qsTr("Permitir varias líneas")
                            Accessible.name: qsTr("Permitir varias líneas en %1").arg(recordFrame.key || qsTr("campo sin nombre"))
                            checked: !!recordFrame.multiline
                            onToggled: root.updateRecord(recordFrame.index, "multiline", checked)
                        }
                        Controls.CheckBox {
                            text: qsTr("Ocultar en el detalle")
                            Accessible.name: qsTr("Ocultar %1 en el detalle").arg(recordFrame.key || qsTr("campo sin nombre"))
                            checked: !!recordFrame.concealed
                            onToggled: root.updateRecord(recordFrame.index, "concealed", checked)
                        }
                    }
                }
            }
        }

        Controls.Button {
            text: qsTr("Agregar campo")
            icon.name: "list-add"
            enabled: recordModel.count < root.maximumRecords
            onClicked: root.addRecord()
        }
    }

    NotesEditor {
        id: notesEditor
        Layout.fillWidth: true
        onSourceTextEdited: value => root.notes = value
    }
}
