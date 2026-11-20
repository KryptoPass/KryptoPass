pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

// Standard KDE "Configure" entry point for the settings that apply immediately:
// the shell persists the idle lock and the secure clipboard persists its timeout.
Controls.Dialog {
    id: root

    property int idleLockSeconds: 0
    property int autoClearSeconds: 0

    signal idleLockSecondsRequested(int seconds)
    signal autoClearSecondsRequested(int seconds)

    readonly property var idleOptions: [
        {
            label: KI18n.i18nc("@item:inlistbox idle lock", "Off"),
            seconds: 0
        },
        {
            label: KI18n.i18nc("@item:inlistbox idle lock", "1 minute"),
            seconds: 60
        },
        {
            label: KI18n.i18nc("@item:inlistbox idle lock", "5 minutes"),
            seconds: 300
        },
        {
            label: KI18n.i18nc("@item:inlistbox idle lock", "15 minutes"),
            seconds: 900
        }
    ]
    readonly property var clipboardOptions: [
        {
            label: KI18n.i18nc("@item:inlistbox clipboard clearing", "Off"),
            seconds: 0
        },
        {
            label: KI18n.i18nc("@item:inlistbox clipboard clearing", "15 seconds"),
            seconds: 15
        },
        {
            label: KI18n.i18nc("@item:inlistbox clipboard clearing", "30 seconds"),
            seconds: 30
        },
        {
            label: KI18n.i18nc("@item:inlistbox clipboard clearing", "60 seconds"),
            seconds: 60
        }
    ]

    function selectIndex(options, seconds) {
        return Math.max(0, options.findIndex(option => option.seconds === seconds));
    }

    function syncSelections() {
        idleLock.currentIndex = selectIndex(idleOptions, idleLockSeconds);
        clipboardClear.currentIndex = selectIndex(clipboardOptions, autoClearSeconds);
    }

    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 28, parent.width - Kirigami.Units.gridUnit * 4)
    modal: true
    title: KI18n.i18nc("@title:window", "Configure %1", Application.displayName)

    onOpened: syncSelections()
    onIdleLockSecondsChanged: syncSelections()
    onAutoClearSecondsChanged: syncSelections()

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Kirigami.FormLayout {
            Layout.fillWidth: true
            Controls.ComboBox {
                id: idleLock
                Kirigami.FormData.label: KI18n.i18nc("@label:listbox", "Lock the vault after inactivity:")
                model: root.idleOptions
                textRole: "label"
                Accessible.name: KI18n.i18nc("@label:listbox", "Lock the vault after inactivity")
                onActivated: root.idleLockSecondsRequested(root.idleOptions[currentIndex].seconds)
            }
            Controls.ComboBox {
                id: clipboardClear
                Kirigami.FormData.label: KI18n.i18nc("@label:listbox", "Clear from clipboard after:")
                model: root.clipboardOptions
                textRole: "label"
                Accessible.name: KI18n.i18nc("@label:listbox", "Clear from clipboard after")
                onActivated: root.autoClearSecondsRequested(root.clipboardOptions[currentIndex].seconds)
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            text: KI18n.i18n("Changes are saved as you choose them.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
    }

    footer: RowLayout {
        Item {
            Layout.fillWidth: true
        }
        Controls.Button {
            text: KI18n.i18nc("@action:button", "Close")
            onClicked: root.close()
        }
    }
}
