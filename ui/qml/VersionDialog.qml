pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

Controls.Dialog {
    id: root

    property var versionDetail: null
    property var versions: []
    property string currentTitle: ""

    readonly property int versionPosition: {
        if (!root.versionDetail)
            return -1;
        return root.versions.findIndex(version => version.index === root.versionDetail.index);
    }
    readonly property int versionCount: root.versions.length
    readonly property var olderVersion: root.versionPosition >= 0 && root.versionPosition < root.versionCount - 1 ? root.versions[root.versionPosition + 1] : null
    readonly property var newerVersion: root.versionPosition > 0 ? root.versions[root.versionPosition - 1] : null
    readonly property bool titleChanged: !!root.versionDetail && root.versionDetail.title !== root.currentTitle
    readonly property string dateText: root.formatTimestamp(root.versionDetail?.at) || KI18n.i18n("Date unavailable")

    signal revealRequested
    signal copyRequested(string field)
    signal copyUrlRequested(int index)
    signal openVersionRequested(int index)
    signal versionClosed

    function formatTimestamp(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : Qt.formatDateTime(date, "dd MMM yyyy, HH:mm");
    }

    function changed(field) {
        return Array.isArray(root.versionDetail?.changedFields) && root.versionDetail.changedFields.includes(field);
    }

    parent: Controls.Overlay.overlay
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0
    width: Math.min(Kirigami.Units.gridUnit * 34, parent ? parent.width - Kirigami.Units.largeSpacing * 2 : 620)
    height: Math.min(implicitHeight, parent ? parent.height * 0.85 : 720)
    modal: true
    closePolicy: Controls.Popup.CloseOnEscape
    title: KI18n.i18nc("@title", "Previous Version")
    onVersionDetailChanged: {
        if (versionDetail) {
            if (!root.opened)
                root.open();
        } else if (root.opened) {
            root.close();
        }
    }
    onClosed: root.versionClosed()

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Accessible.role: Accessible.Dialog
        Accessible.name: root.title

        Controls.Label {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            Layout.topMargin: Kirigami.Units.smallSpacing
            text: KI18n.i18n("%1 · Version %2 of %3", root.dateText, root.versionPosition + 1, root.versionCount)
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        Controls.Label {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            text: KI18n.i18n("Read only. Viewing or copying this data does not change the current one.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.ScrollView {
            id: versionScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            leftPadding: Kirigami.Units.largeSpacing
            rightPadding: Kirigami.Units.largeSpacing
            topPadding: Kirigami.Units.smallSpacing
            bottomPadding: Kirigami.Units.smallSpacing
            implicitHeight: versionFields.implicitHeight

            ColumnLayout {
                id: versionFields
                width: Math.max(0, versionScroll.availableWidth - Kirigami.Units.gridUnit)
                spacing: Kirigami.Units.largeSpacing

                DetailField {
                    Layout.fillWidth: true
                    label: KI18n.i18nc("@label", "Name")
                    value: root.versionDetail?.title || KI18n.i18n("No name")
                    copyLabel: KI18n.i18nc("@action:button", "Copy Name")
                    copyEnabled: !!root.versionDetail?.title
                    badge: root.titleChanged ? KI18n.i18nc("@label badge", "Changed") : ""
                    onCopyRequested: root.copyRequested("title")
                }

                GridLayout {
                    Layout.fillWidth: true
                    columns: width >= Kirigami.Units.gridUnit * 24 ? 2 : 1
                    columnSpacing: Kirigami.Units.largeSpacing
                    rowSpacing: Kirigami.Units.smallSpacing

                    DetailField {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        label: KI18n.i18nc("@label:textbox", "Email or username")
                        value: root.versionDetail?.username || KI18n.i18n("No username")
                        copyLabel: KI18n.i18nc("@action:button", "Copy Username")
                        copyEnabled: !!root.versionDetail?.username
                        badge: root.changed("username") ? KI18n.i18nc("@label badge", "Changed") : ""
                        onCopyRequested: root.copyRequested("username")
                    }
                    DetailField {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        label: KI18n.i18nc("@label", "Password")
                        value: !root.versionDetail?.hasPassword ? KI18n.i18n("No password") : root.versionDetail?.passwordRevealed ? root.versionDetail?.password || "••••••••••••" : "••••••••••••"
                        copyLabel: KI18n.i18nc("@action:button", "Copy Password")
                        copyEnabled: !!root.versionDetail?.hasPassword
                        revealable: !!root.versionDetail?.hasPassword
                        revealed: !!root.versionDetail?.passwordRevealed
                        revealLabel: KI18n.i18nc("@action:button", "Show Previous Password")
                        hideLabel: KI18n.i18nc("@action:button", "Hide Previous Password")
                        badge: root.changed("password") ? KI18n.i18nc("@label badge", "Changed") : ""
                        onRevealRequested: root.revealRequested()
                        onCopyRequested: root.copyRequested("password")
                    }
                }

                WebsiteListCard {
                    Layout.fillWidth: true
                    visible: (root.versionDetail?.urls || []).length > 0
                    urls: root.versionDetail?.urls || []
                    badge: root.changed("urls") ? KI18n.i18nc("@label badge", "Changed") : ""
                    onCopyUrlRequested: function (index) {
                        root.copyUrlRequested(index);
                    }
                }

                DetailField {
                    Layout.fillWidth: true
                    visible: !!root.versionDetail?.hasTotp
                    label: KI18n.i18nc("@label", "Verification code")
                    value: KI18n.i18n("Verification settings saved")
                    copyLabel: KI18n.i18nc("@action:button", "Copy Previous Settings")
                    copyEnabled: !!root.versionDetail?.hasTotp
                    badge: root.changed("totp") ? KI18n.i18nc("@label badge", "Changed") : ""
                    onCopyRequested: root.copyRequested("totp")
                }

                SecretTextSection {
                    Layout.fillWidth: true
                    visible: !!root.versionDetail?.notes
                    heading: KI18n.i18nc("@title:group", "Notes")
                    value: root.versionDetail?.notes || ""
                    documentKey: "version:" + (root.versionDetail?.index ?? -1)
                    accessibleLabel: KI18n.i18n("Notes of the previous version")
                    badge: root.changed("notes") ? KI18n.i18nc("@label badge", "Changed") : ""
                    copyLabel: KI18n.i18nc("@action:button", "Copy Previous Notes")
                    onCopyRequested: root.copyRequested("notes")
                }
            }
        }
    }

    footer: Controls.Pane {
        padding: Kirigami.Units.largeSpacing
        background: null
        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                visible: root.versionCount > 1
                spacing: Kirigami.Units.smallSpacing

                Controls.Button {
                    text: KI18n.i18nc("@action:button", "Previous")
                    icon.name: "go-previous"
                    flat: true
                    enabled: !!root.olderVersion
                    onClicked: root.openVersionRequested(root.olderVersion.index)
                }
                Controls.Label {
                    text: KI18n.i18n("Version %1 of %2", root.versionPosition + 1, root.versionCount)
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Button {
                    text: KI18n.i18nc("@action:button", "Next")
                    icon.name: "go-next"
                    flat: true
                    enabled: !!root.newerVersion
                    onClicked: root.openVersionRequested(root.newerVersion.index)
                }
            }

            Item {
                Layout.fillWidth: true
            }

            Controls.Button {
                text: KI18n.i18nc("@action:button", "Close")
                onClicked: root.close()
            }
        }
    }
}
