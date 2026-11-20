pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls

Controls.Action {
    id: root
    required property var controller
    required property string actionId
    readonly property var definition: JSON.parse(controller.actionsJson)[actionId] || {}
    readonly property var sequences: definition.shortcuts || []
    text: definition.text || ""
    icon.name: definition.icon || ""
    shortcut: sequences[0] || ""

    // Qt Quick Action exposes one shortcut; KDE also allows alternative sequences.
    readonly property Shortcut alternatives: Shortcut {
        sequences: root.sequences.slice(1)
        enabled: root.enabled
        onActivated: root.trigger()
    }

    onEnabledChanged: root.controller.setActionEnabled(root.actionId, root.enabled)
    Component.onCompleted: root.controller.setActionEnabled(root.actionId, root.enabled)
}
