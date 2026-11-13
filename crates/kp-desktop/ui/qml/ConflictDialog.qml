pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// Merges two diverging versions of an item: one side per difference. Concealed
// conflicts come with nothing selected, so the person has to look at both.
Controls.Dialog {
    id: root

    required property SecretStore storeAdapter
    readonly property var conflict: {
        const json = storeAdapter.controller.conflictJson;
        return json ? JSON.parse(json) : null;
    }
    readonly property var rows: conflict ? conflict.rows : []
    property var choices: ({})
    readonly property bool complete: rows.every(row => !!(choices[row.key] || row.suggested))

    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, parent.width - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: qsTr("Elegir qué conservar")

    // Accepting closes the dialog on its own, so visibility cannot be a binding.
    onConflictChanged: {
        if (conflict) {
            open();
        } else {
            choices = {};
            close();
        }
    }

    function choose(key, side) {
        const updated = Object.assign({}, choices);
        updated[key] = side;
        choices = updated;
    }

    function chosen(row) {
        return choices[row.key] || row.suggested;
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cancelar")
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
        }
        Controls.Button {
            text: qsTr("Guardar combinación")
            enabled: root.complete && !root.storeAdapter.controller.busy
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.AcceptRole
        }
        onAccepted: root.storeAdapter.controller.resolveConflict(JSON.stringify(root.choices))
        onRejected: root.storeAdapter.controller.closeConflict()
    }

    contentItem: Controls.ScrollView {
        contentWidth: availableWidth

        ColumnLayout {
            width: parent.width
            spacing: Kirigami.Units.largeSpacing

            Controls.Label {
                Layout.fillWidth: true
                text: root.conflict ? root.conflict.title : ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font.bold: true
            }

            Repeater {
                model: root.rows

                ColumnLayout {
                    id: row

                    required property var modelData
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true

                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 4
                            text: row.modelData.label
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                        Controls.Button {
                            visible: row.modelData.concealed
                            flat: true
                            icon.name: row.modelData.revealed ? "view-hidden" : "view-visible"
                            text: row.modelData.revealed ? qsTr("Ocultar") : qsTr("Mostrar")
                            onClicked: root.storeAdapter.controller.toggleConflictValue(row.modelData.key)
                        }
                    }

                    Repeater {
                        model: [
                            {
                                side: "ours",
                                label: qsTr("Este equipo"),
                                value: row.modelData.ours
                            },
                            {
                                side: "theirs",
                                label: qsTr("Otro dispositivo"),
                                value: row.modelData.theirs
                            }
                        ]

                        Controls.RadioButton {
                            id: choice

                            required property var modelData
                            Layout.fillWidth: true
                            checked: root.chosen(row.modelData) === modelData.side
                            onClicked: root.choose(row.modelData.key, modelData.side)
                            text: {
                                let value = modelData.value;
                                if (row.modelData.concealed && !row.modelData.revealed)
                                    value = "••••••••";
                                else if (!value)
                                    value = qsTr("(vacío)");
                                return modelData.label + ": " + value;
                            }
                        }
                    }

                    Kirigami.Separator {
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }
}
