pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: picker

    property string selectedIcon: "folder"
    property string selectedColor: "transparent"

    readonly property int cellSize: 40
    readonly property int cellSpacing: 12

    readonly property var colorOptions: [
        { value: "transparent", name: qsTr("Sin color") },
        { value: "#a775ff", name: qsTr("Violeta") },
        { value: "#f28c8c", name: qsTr("Coral") },
        { value: "#f4d66d", name: qsTr("Amarillo") },
        { value: "#87c796", name: qsTr("Verde") },
        { value: "#82a9ee", name: qsTr("Azul") },
        { value: "#e78bd6", name: qsTr("Rosa") },
        { value: "#cf5b72", name: qsTr("Frambuesa") },
        { value: "#e1a25b", name: qsTr("Naranja") },
        { value: "#d8d8da", name: qsTr("Gris") },
        { value: "#80d5dc", name: qsTr("Turquesa") }
    ]
    readonly property var iconOptions: [
        { value: "folder", name: qsTr("Carpeta") },
        { value: "user-home", name: qsTr("Inicio") },
        { value: "applications-office", name: qsTr("Trabajo") },
        { value: "wallet-open", name: qsTr("Billetera") },
        { value: "folder-favorites", name: qsTr("Favoritos") },
        { value: "preferences-system", name: qsTr("Sistema") },
        { value: "office-address-book", name: qsTr("Contactos") },
        { value: "view-calendar", name: qsTr("Calendario") },
        { value: "text-x-generic", name: qsTr("Documento") },
        { value: "applications-games", name: qsTr("Entretenimiento") }
    ]

    signal iconSelected(string iconName)
    signal colorSelected(string colorValue)

    spacing: 8
    opacity: enabled ? 1 : 0.45

    Controls.Label {
        text: qsTr("Color")
        font.weight: Font.DemiBold
    }

    GridLayout {
        Layout.alignment: Qt.AlignLeft
        columns: 6
        rowSpacing: picker.cellSpacing
        columnSpacing: picker.cellSpacing

        Repeater {
            model: picker.colorOptions

            delegate: Controls.ToolButton {
                id: colorButton
                required property var modelData

                Layout.preferredWidth: picker.cellSize
                Layout.preferredHeight: picker.cellSize
                Layout.minimumWidth: picker.cellSize
                Layout.maximumWidth: picker.cellSize
                Layout.minimumHeight: picker.cellSize
                Layout.maximumHeight: picker.cellSize
                checkable: true
                autoExclusive: true
                checked: modelData.value === picker.selectedColor
                text: ""
                Accessible.name: modelData.name
                Accessible.description: checked ? qsTr("Color seleccionado") : qsTr("Seleccionar color")
                onClicked: picker.colorSelected(modelData.value)

                contentItem: Item {
                    Rectangle {
                        anchors.centerIn: parent
                        visible: colorButton.modelData.value !== "transparent"
                        width: 24
                        height: width
                        radius: width / 2
                        color: colorButton.modelData.value
                        border.width: 1
                        border.color: Qt.alpha(Kirigami.Theme.textColor, 0.22)
                    }
                    Rectangle {
                        anchors.centerIn: parent
                        width: 24
                        height: width
                        radius: width / 2
                        visible: colorButton.modelData.value === "transparent"
                        color: "transparent"
                        border.width: 1
                        border.color: colorButton.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor

                        Rectangle {
                            anchors.centerIn: parent
                            width: 24
                            height: 1
                            rotation: -45
                            color: colorButton.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                        }
                    }
                }

                background: Rectangle {
                    radius: 5
                    color: colorButton.checked ? Qt.alpha(Kirigami.Theme.highlightColor, 0.14)
                                               : colorButton.hovered ? Qt.alpha(Kirigami.Theme.textColor, 0.07) : "transparent"
                    border.width: 1
                    border.color: colorButton.checked || colorButton.activeFocus ? Kirigami.Theme.highlightColor : "transparent"
                }

                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: modelData.name
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }

    Controls.Label {
        Layout.topMargin: 12
        text: qsTr("Icono")
        font.weight: Font.DemiBold
    }

    GridLayout {
        Layout.alignment: Qt.AlignLeft
        columns: 6
        rowSpacing: picker.cellSpacing
        columnSpacing: picker.cellSpacing

        Repeater {
            model: picker.iconOptions

            delegate: Controls.ToolButton {
                id: iconButton
                required property var modelData

                Layout.preferredWidth: picker.cellSize
                Layout.preferredHeight: picker.cellSize
                Layout.minimumWidth: picker.cellSize
                Layout.maximumWidth: picker.cellSize
                Layout.minimumHeight: picker.cellSize
                Layout.maximumHeight: picker.cellSize
                checkable: true
                autoExclusive: true
                checked: modelData.value === picker.selectedIcon
                text: ""
                Accessible.name: modelData.name
                Accessible.description: checked ? qsTr("Icono seleccionado") : qsTr("Seleccionar icono")
                onClicked: picker.iconSelected(modelData.value)

                contentItem: Item {
                    FolderIcon {
                        anchors.centerIn: parent
                        width: 24
                        height: width
                        iconName: iconButton.modelData.value
                        monochrome: true
                        foregroundColor: iconButton.checked ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                    }
                }

                background: Rectangle {
                    radius: 5
                    color: iconButton.checked ? Qt.alpha(Kirigami.Theme.highlightColor, 0.14)
                                              : iconButton.hovered ? Qt.alpha(Kirigami.Theme.textColor, 0.07) : "transparent"
                    border.width: 1
                    border.color: iconButton.checked || iconButton.activeFocus ? Kirigami.Theme.highlightColor : "transparent"
                }

                Controls.ToolTip.visible: hovered
                Controls.ToolTip.text: modelData.name
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
}
