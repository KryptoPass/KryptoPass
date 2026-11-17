pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: root

    property var detail: null
    property var recipients: []
    property var recipientKeys: []
    property bool revealed: false
    property var versionDetail: null
    property bool historyOpen: false
    property bool historyExpanded: false
    property string historyEntryId: ""
    property Item versionReturnItem: null
    readonly property SecretContentView contentView: contentLoader.item as SecretContentView
    readonly property var historyItems: Array.isArray(root.detail?.content?.fields?.history?.events) ? root.detail.content.fields.history.events : []
    readonly property var visibleHistoryItems: root.historyItems.slice(0, root.historyExpanded ? 20 : 5)
    readonly property var versions: Array.isArray(root.detail?.content?.fields?.history?.versions) ? root.detail.content.fields.history.versions : []
    readonly property bool hasHistory: !!root.detail?.content?.fields?.history
    readonly property bool wideLayout: root.width >= Kirigami.Units.gridUnit * 56
    readonly property bool bottomTwoColumns: root.wideLayout && root.hasHistory && root.recipients.length > 0

    signal revealToggled
    signal copyUsernameRequested
    signal copyPasswordRequested
    signal copyTotpRequested(bool next)
    signal copyFieldRequested(string field)
    signal copyUrlRequested(int index)
    signal revealFieldRequested(string field)
    signal qrRequested
    signal versionOpenRequested(int index)
    signal versionCloseRequested
    signal versionCopyRequested(string field)
    signal versionCopyUrlRequested(int index)
    signal versionRevealRequested
    signal activityExportRequested

    spacing: Kirigami.Units.largeSpacing

    function eventText(kind) {
        switch (kind) {
        case "create":
            return KI18n.i18nc("@item activity event", "Login created");
        case "loginUse":
            return KI18n.i18nc("@item activity event", "Login used");
        case "passwordRotated":
            return KI18n.i18nc("@item activity event", "Password updated");
        case "edited":
            return KI18n.i18nc("@item activity event", "Data edited");
        case "copyUsername":
            return KI18n.i18nc("@item activity event", "Username copied");
        case "copyPassword":
            return KI18n.i18nc("@item activity event", "Password copied");
        case "copyTotp":
            return KI18n.i18nc("@item activity event", "Verification code copied");
        case "copyNextTotp":
            return KI18n.i18nc("@item activity event", "Next verification code copied");
        default:
            return KI18n.i18nc("@item activity event", "Activity recorded");
        }
    }

    function formatEventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleDateString(Qt.locale(), Locale.ShortFormat) + " " + date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    function eventIcon(kind) {
        switch (kind) {
        case "create":
            return "document-new";
        case "passwordRotated":
            return "view-refresh";
        case "edited":
            return "document-edit";
        case "loginUse":
            return "dialog-password";
        case "copyUsername":
        case "copyPassword":
        case "copyTotp":
        case "copyNextTotp":
            return "edit-copy";
        default:
            return "documentinfo";
        }
    }

    function eventDate(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? KI18n.i18n("Date unavailable") : date.toLocaleDateString(Qt.locale(), Locale.LongFormat);
    }

    function eventTime(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : date.toLocaleTimeString(Qt.locale(), Locale.LongFormat);
    }

    onDetailChanged: {
        const nextId = root.detail?.id || "";
        if (nextId !== root.historyEntryId) {
            root.historyEntryId = nextId;
            root.historyOpen = false;
            root.historyExpanded = false;
        }
    }

    Loader {
        id: contentLoader
        Layout.fillWidth: true
        sourceComponent: EntryTypes.find(root.detail?.content?.kind)?.detail || null
    }

    Binding {
        target: root.contentView
        property: "fields"
        value: root.detail?.content?.fields || null
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "entryId"
        value: root.detail?.id || ""
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "revealed"
        value: root.revealed
        when: !!root.contentView
    }
    Binding {
        target: root.contentView
        property: "versionDetail"
        value: root.versionDetail
        when: !!root.contentView
    }

    Connections {
        target: root.contentView
        function onRevealToggled() {
            root.revealToggled();
        }
        function onCopyUsernameRequested() {
            root.copyUsernameRequested();
        }
        function onCopyPasswordRequested() {
            root.copyPasswordRequested();
        }
        function onCopyTotpRequested(next) {
            root.copyTotpRequested(next);
        }
        function onCopyFieldRequested(field) {
            root.copyFieldRequested(field);
        }
        function onCopyUrlRequested(index) {
            root.copyUrlRequested(index);
        }
        function onRevealFieldRequested(field) {
            root.revealFieldRequested(field);
        }
        function onQrRequested() {
            root.qrRequested();
        }
        function onVersionOpenRequested(index) {
            root.versionOpenRequested(index);
        }
        function onVersionCloseRequested() {
            root.versionCloseRequested();
        }
        function onVersionCopyRequested(field) {
            root.versionCopyRequested(field);
        }
        function onActivityExportRequested() {
            root.activityExportRequested();
        }
    }

    Kirigami.InlineMessage {
        Layout.fillWidth: true
        visible: !!root.detail && !EntryTypes.find(root.detail.content?.kind)
        type: Kirigami.MessageType.Error
        text: KI18n.i18n("This item type cannot be shown.")
    }

    GridLayout {
        Layout.fillWidth: true
        columns: root.bottomTwoColumns ? 2 : 1
        columnSpacing: Kirigami.Units.largeSpacing
        rowSpacing: Kirigami.Units.largeSpacing

        Controls.Frame {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.recipients.length > 0
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    text: KI18n.i18nc("@title:group", "Who has access")
                    color: Kirigami.Theme.disabledTextColor
                }
                Repeater {
                    model: root.recipients
                    delegate: ColumnLayout {
                        id: recipientRow
                        required property string modelData
                        readonly property var recipientKey: root.recipientKeys.find(key => key.id.toUpperCase() === recipientRow.modelData.toUpperCase()) || null
                        readonly property string recipientName: recipientKey?.contactName || recipientKey?.title || recipientKey?.email || KI18n.i18nc("@label recipient", "Key unavailable")
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 2
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: recipientRow.recipientName
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    visible: !!recipientRow.recipientKey?.email
                                    text: recipientRow.recipientKey?.email || ""
                                    textFormat: Text.PlainText
                                    color: Kirigami.Theme.disabledTextColor
                                    wrapMode: Text.WrapAnywhere
                                }
                            }
                            Controls.ToolButton {
                                id: recipientDetails
                                text: checked ? KI18n.i18nc("@action:button", "Hide Key Details") : KI18n.i18nc("@action:button", "View Key Details")
                                icon.name: checked ? "arrow-up" : "arrow-down"
                                display: Controls.AbstractButton.IconOnly
                                checkable: true
                                Accessible.name: KI18n.i18nc("@label label and value", "%1: %2", text, recipientRow.recipientName)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: recipientDetails.checked
                            text: KI18n.i18n("OpenPGP fingerprint: %1", recipientRow.modelData)
                            textFormat: Text.PlainText
                            font.family: Kirigami.Theme.fixedWidthFont.family
                            wrapMode: Text.WrapAnywhere
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: root.hasHistory
            spacing: Kirigami.Units.largeSpacing

            Controls.Frame {
                Layout.fillWidth: true
                visible: root.historyItems.length > 0
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing

                    Controls.ToolButton {
                        id: activityToggle
                        Layout.fillWidth: true
                        leftPadding: 0
                        rightPadding: 0
                        readonly property string sectionTitle: KI18n.i18nc("@title:group", "Activity")
                        text: ""
                        display: Controls.AbstractButton.IconOnly
                        focusPolicy: Qt.StrongFocus
                        checkable: true
                        checked: root.historyOpen
                        Accessible.name: KI18n.i18nc("@title:group", "Activity")
                        Accessible.description: root.historyOpen ? KI18n.i18n("Hide the %1 events", root.historyItems.length) : KI18n.i18n("Show the %1 events", root.historyItems.length)
                        onToggled: root.historyOpen = checked
                        contentItem: RowLayout {
                            spacing: Kirigami.Units.largeSpacing
                            Controls.Label {
                                Layout.fillWidth: true
                                text: activityToggle.sectionTitle
                                color: Kirigami.Theme.disabledTextColor
                                wrapMode: Text.Wrap
                            }
                            Controls.Label {
                                text: root.historyItems.length === 1 ? KI18n.i18n("1 event") : KI18n.i18n("%1 events", root.historyItems.length)
                                color: Kirigami.Theme.disabledTextColor
                            }
                            Kirigami.Icon {
                                source: root.historyOpen ? "arrow-up" : "arrow-down"
                                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                            }
                        }
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: !root.historyOpen
                        text: root.historyItems.length ? KI18n.i18nc("@label two parts of a subtitle", "%1 · %2", root.eventText(root.historyItems[0].kind), root.formatEventTime(root.historyItems[0].at)) : ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: root.historyOpen
                        spacing: Kirigami.Units.smallSpacing

                        Repeater {
                            model: root.historyOpen ? root.visibleHistoryItems : []
                            delegate: ColumnLayout {
                                id: historyItemRow
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                spacing: Kirigami.Units.smallSpacing
                                readonly property bool startsDay: index === 0 || root.eventDate(modelData.at) !== root.eventDate(root.visibleHistoryItems[index - 1].at)

                                Controls.Label {
                                    Layout.fillWidth: true
                                    Layout.topMargin: historyItemRow.index > 0 ? Kirigami.Units.largeSpacing : Kirigami.Units.smallSpacing
                                    visible: historyItemRow.startsDay
                                    text: root.eventDate(historyItemRow.modelData.at)
                                    wrapMode: Text.Wrap
                                    textFormat: Text.PlainText
                                    font.weight: Font.Medium
                                }

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: Kirigami.Units.smallSpacing
                                    Kirigami.Icon {
                                        source: root.eventIcon(historyItemRow.modelData.kind)
                                        Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                        Layout.preferredHeight: Kirigami.Units.iconSizes.small
                                        color: Kirigami.Theme.disabledTextColor
                                    }
                                    Controls.Label {
                                        Layout.fillWidth: true
                                        Layout.minimumWidth: 0
                                        text: root.eventText(historyItemRow.modelData.kind)
                                        textFormat: Text.PlainText
                                        wrapMode: Text.Wrap
                                    }
                                    Controls.Label {
                                        text: root.eventTime(historyItemRow.modelData.at)
                                        textFormat: Text.PlainText
                                        color: Kirigami.Theme.disabledTextColor
                                        Accessible.name: root.formatEventTime(historyItemRow.modelData.at)
                                    }
                                }
                                Kirigami.Separator {
                                    Layout.fillWidth: true
                                    visible: historyItemRow.index < root.visibleHistoryItems.length - 1
                                }
                            }
                        }
                        Controls.Button {
                            visible: root.historyItems.length > 5
                            text: root.historyExpanded ? KI18n.i18nc("@action:button", "Show Only the Last 5") : KI18n.i18nc("@action:button", "Show the Last %1", Math.min(20, root.historyItems.length))
                            flat: true
                            onClicked: root.historyExpanded = !root.historyExpanded
                        }
                        Controls.Button {
                            text: KI18n.i18nc("@action:button", "Show All Events…")
                            icon.name: "document-open"
                            flat: true
                            Controls.ToolTip.visible: hovered || visualFocus
                            Controls.ToolTip.text: KI18n.i18nc("@info:tooltip", "Create and open a text file with the kept events")
                            onClicked: root.activityExportRequested()
                        }
                    }
                }
            }

            Controls.Frame {
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: root.hasHistory
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing
                    Controls.Label {
                        Layout.fillWidth: true
                        text: KI18n.i18nc("@title:group", "Previous versions")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        visible: root.versions.length === 0
                        text: KI18n.i18n("There are no previous versions yet. They are kept when you update this login's data.")
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                    }
                    Repeater {
                        model: root.versions
                        delegate: RowLayout {
                            id: versionRow
                            required property var modelData
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 0
                                spacing: 0
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: KI18n.i18nc("@title", "Previous Version")
                                    wrapMode: Text.Wrap
                                }
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: root.formatEventTime(versionRow.modelData.at)
                                    color: Kirigami.Theme.disabledTextColor
                                    wrapMode: Text.Wrap
                                }
                            }
                            Controls.Button {
                                id: versionOpenButton
                                text: KI18n.i18nc("@action:button previous version", "View")
                                Accessible.name: KI18n.i18n("View the previous version from %1", root.formatEventTime(versionRow.modelData.at))
                                onClicked: {
                                    root.versionReturnItem = versionOpenButton;
                                    root.versionOpenRequested(versionRow.modelData.index);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    VersionDialog {
        id: versionDialog
        versionDetail: root.versionDetail
        versions: root.versions
        currentTitle: root.detail?.title || ""
        onRevealRequested: root.versionRevealRequested()
        onCopyRequested: function (field) {
            root.versionCopyRequested(field);
        }
        onCopyUrlRequested: function (index) {
            root.versionCopyUrlRequested(index);
        }
        onOpenVersionRequested: function (index) {
            root.versionOpenRequested(index);
        }
        onVersionClosed: {
            root.versionCloseRequested();
            Qt.callLater(function () {
                if (root.versionReturnItem)
                    root.versionReturnItem.forceActiveFocus();
            });
        }
    }
}
