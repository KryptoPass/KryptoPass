import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

// Discreet "?" next to a title: explains on hover; the handbook lives in "¿Necesitas ayuda?".
Controls.ToolButton {
    id: root

    required property string help

    icon.name: "help-contextual"
    display: Controls.AbstractButton.IconOnly
    focusPolicy: Qt.NoFocus
    text: KI18n.i18nc("@action:button", "More Information")
    implicitWidth: Kirigami.Units.iconSizes.smallMedium + Kirigami.Units.smallSpacing * 2
    implicitHeight: implicitWidth
    flat: true
    Accessible.description: help
    // Clicking only toggles the tooltip, for touch and keyboard users.
    checkable: true
    Controls.ToolTip.visible: hovered || checked
    Controls.ToolTip.delay: hovered ? Kirigami.Units.toolTipDelay : 0
    Controls.ToolTip.text: help
    onHoveredChanged: {
        if (!hovered)
            checked = false;
    }
}
