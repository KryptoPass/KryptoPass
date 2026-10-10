pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop as Desktop
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: picker

    property string selectedIcon: "folder"
    property string selectedColor: "transparent"

    readonly property int cellSize: 40
    readonly property int cellSpacing: 12
    readonly property color selectionAccent: Kirigami.Theme.highlightColor

    component SelectionBackground: Rectangle {
        required property Controls.ToolButton control

        radius: 5
        color: control.checked ? Qt.alpha(picker.selectionAccent, 0.14)
                               : control.hovered ? Qt.alpha(Kirigami.Theme.textColor, 0.07) : "transparent"
        border.width: 1
        border.color: control.checked || control.activeFocus ? picker.selectionAccent : "transparent"
    }

    readonly property bool darkTheme: Kirigami.ColorUtils.brightnessForColor(Kirigami.Theme.backgroundColor) === Kirigami.ColorUtils.Dark
    readonly property var colorOptions: Desktop.FolderPalette.options
    readonly property var iconOptions: [
        { value: "folder", name: qsTr("Carpeta") },
        { value: "user-home", name: qsTr("Inicio") },
        { value: "applications-office", name: qsTr("Trabajo") },
        { value: "user-identity", name: qsTr("Personal") },
        { value: "folder-favorites", name: qsTr("Favoritos") },
        { value: "office-address-book", name: qsTr("Contactos") },
        { value: "wallet-open", name: qsTr("Billetera") },
        { value: "view-financial-account-credit-card", name: qsTr("Tarjetas") },
        { value: "view-calendar", name: qsTr("Calendario") },
        { value: "text-x-generic", name: qsTr("Documentos") },
        { value: "applications-education", name: qsTr("Estudios") },
        { value: "headphones", name: qsTr("Música") },
        { value: "mail-message", name: qsTr("Correo") },
        { value: "globe", name: qsTr("Internet") },
        { value: "cloudstatus", name: qsTr("Nube") },
        { value: "network-server", name: qsTr("Servidores") },
        { value: "security-high", name: qsTr("Seguridad") },
        { value: "dialog-password", name: qsTr("Contraseñas") },
        { value: "format-text-code", name: qsTr("Desarrollo") },
        { value: "utilities-terminal", name: qsTr("Terminal") },
        { value: "phone", name: qsTr("Teléfono") },
        { value: "camera-photo", name: qsTr("Fotos") },
        { value: "applications-games", name: qsTr("Juegos") },
        { value: "preferences-system", name: qsTr("Sistema") }
    ]

    signal iconSelected(string iconName)
    signal colorSelected(string colorValue)
    signal revealRequested(real top, real height)

    function revealButton(button: Item) {
        const position = button.mapToItem(picker, 0, 0);
        revealRequested(position.y, button.height);
    }

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
                checked: modelData.value === Desktop.FolderPalette.canonicalColor(picker.selectedColor)
                text: ""
                Accessible.name: modelData.name
                Accessible.description: checked ? qsTr("Color seleccionado") : qsTr("Seleccionar color")
                focusPolicy: Qt.StrongFocus
                onActiveFocusChanged: {
                    if (activeFocus)
                        picker.revealButton(colorButton);
                }
                onClicked: picker.colorSelected(modelData.value)

                contentItem: Item {
                    Rectangle {
                        anchors.centerIn: parent
                        visible: colorButton.modelData.value !== "transparent"
                        width: 24
                        height: width
                        radius: width / 2
                        color: Desktop.FolderPalette.displayColor(colorButton.modelData.value, picker.darkTheme)
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
                        border.color: Kirigami.Theme.textColor

                        Rectangle {
                            anchors.centerIn: parent
                            width: 24
                            height: 1
                            rotation: -45
                            color: Kirigami.Theme.textColor
                        }
                    }
                }

                background: SelectionBackground {
                    control: colorButton
                }

                Controls.ToolTip.visible: hovered || activeFocus
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
                focusPolicy: Qt.StrongFocus
                onActiveFocusChanged: {
                    if (activeFocus)
                        picker.revealButton(iconButton);
                }
                onClicked: picker.iconSelected(modelData.value)

                contentItem: Item {
                    FolderIcon {
                        anchors.centerIn: parent
                        width: 24
                        height: width
                        iconName: iconButton.modelData.value
                        monochrome: true
                    }
                }

                background: SelectionBackground {
                    control: iconButton
                }

                Controls.ToolTip.visible: hovered || activeFocus
                Controls.ToolTip.text: modelData.name
                Controls.ToolTip.delay: Kirigami.Units.toolTipDelay
            }
        }
    }
}
