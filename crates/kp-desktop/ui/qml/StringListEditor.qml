pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property string label: ""
    property string addLabel: qsTr("Agregar elemento")
    property string inputLabel: label
    property var values: []
    property var workingValues: []
    property int maximumValues: 100
    property bool editingValues: false

    signal valuesEdited(var values)

    spacing: Kirigami.Units.smallSpacing

    function readValues() {
        const result = [];
        for (let index = 0; index < valueModel.count; ++index)
            result.push(valueModel.get(index).value);
        return result;
    }

    function publishValues() {
        workingValues = readValues();
        valuesEdited(workingValues.slice());
    }

    function changeValues(change) {
        editingValues = true;
        try {
            change();
            publishValues();
        } finally {
            editingValues = false;
        }
    }

    function syncFromValues() {
        const source = Array.isArray(values) ? values : [];
        if (source.length === valueModel.count) {
            let matches = true;
            for (let index = 0; index < source.length; ++index) {
                if (String(source[index] ?? "") !== valueModel.get(index).value) {
                    matches = false;
                    break;
                }
            }
            if (matches)
                return;
        }

        valueModel.clear();
        for (const value of source)
            valueModel.append({
                value: String(value ?? "")
            });
        workingValues = readValues();
    }

    function editValue(index, value) {
        if (index < 0 || index >= valueModel.count)
            return;
        changeValues(() => valueModel.setProperty(index, "value", String(value)));
    }

    function addValue() {
        if (valueModel.count >= maximumValues)
            return;
        changeValues(() => valueModel.append({
                value: ""
            }));
        Qt.callLater(function () {
            const row = valueRepeater.itemAt(valueRepeater.count - 1);
            if (row)
                row.focusValueField();
        });
    }

    function removeValue(index) {
        if (index < 0 || index >= valueModel.count)
            return;
        changeValues(() => valueModel.remove(index));
    }

    function moveValue(index, delta) {
        const target = index + delta;
        if (index < 0 || index >= valueModel.count || target < 0 || target >= valueModel.count)
            return;
        changeValues(() => valueModel.move(index, target, 1));
    }

    function clearValues() {
        if (valueModel.count === 0)
            return;
        changeValues(() => valueModel.clear());
    }

    onValuesChanged: {
        if (!editingValues)
            syncFromValues();
    }

    ListModel {
        id: valueModel
    }

    Controls.Label {
        Layout.fillWidth: true
        visible: root.label.length > 0
        text: root.label
        textFormat: Text.PlainText
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
    }

    Repeater {
        id: valueRepeater
        model: valueModel

        delegate: RowLayout {
            id: valueRow
            required property int index
            required property string value
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            function focusValueField() {
                valueField.forceActiveFocus();
            }

            Controls.TextField {
                id: valueField
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: valueRow.value
                placeholderText: root.inputLabel
                Accessible.name: root.inputLabel + " " + (valueRow.index + 1)
                onTextEdited: root.editValue(valueRow.index, text)
            }

            Controls.ToolButton {
                text: qsTr("Mover %1 hacia arriba").arg(root.inputLabel)
                icon.name: "arrow-up"
                display: Controls.AbstractButton.IconOnly
                enabled: valueRow.index > 0
                Accessible.name: text
                onClicked: root.moveValue(valueRow.index, -1)
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: text
            }

            Controls.ToolButton {
                text: qsTr("Mover %1 hacia abajo").arg(root.inputLabel)
                icon.name: "arrow-down"
                display: Controls.AbstractButton.IconOnly
                enabled: valueRow.index < valueModel.count - 1
                Accessible.name: text
                onClicked: root.moveValue(valueRow.index, 1)
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: text
            }

            Controls.ToolButton {
                text: qsTr("Quitar %1 %2").arg(root.inputLabel).arg(valueRow.index + 1)
                icon.name: "edit-delete"
                display: Controls.AbstractButton.IconOnly
                Accessible.name: text
                onClicked: root.removeValue(valueRow.index)
                Controls.ToolTip.visible: hovered || visualFocus
                Controls.ToolTip.text: text
            }
        }
    }

    Controls.Button {
        text: root.addLabel
        icon.name: "list-add"
        enabled: valueModel.count < root.maximumValues
        onClicked: root.addValue()
    }
}
