pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kde.ki18n

ColumnLayout {
    id: root

    Accessible.role: Accessible.Grouping
    Accessible.name: KI18n.i18nc("@title", "Linked item")

    property var entries: []
    property var kindOptions: []
    property var folders: []
    property var folderRows: []
    property string entryId: ""
    property string relatedEntryId: ""
    property alias folderFilterId: relatedEntryDialog.folderFilterId

    readonly property var options: {
        const choices = [
            {
                id: "",
                label: KI18n.i18nc("@item:inlistbox", "No link"),
                unavailable: false
            }
        ];
        const eligible = entries.filter(candidate => !candidate.trashed && candidate.id !== entryId && (!candidate.relatedEntryId || candidate.relatedEntryId === entryId));
        eligible.sort((left, right) => left.title.localeCompare(right.title, undefined, {
                sensitivity: "base",
                numeric: true
            }) || left.id.localeCompare(right.id));
        for (const candidate of eligible) {
            choices.push({
                id: candidate.id,
                title: candidate.title,
                kind: candidate.kind,
                folder: candidate.folder || "",
                icon: candidate.icon || null,
                iconSource: candidate.iconSource || "",
                subtitle: KI18n.i18nc("@label two parts of a subtitle", "%1 · %2", kindName(candidate.kind), folderPath(candidate.folder)),
                label: KI18n.i18nc("@label two parts of a subtitle", "%1 · %2", candidate.title, kindName(candidate.kind)),
                unavailable: false
            });
        }

        const current = relatedEntryId ? entries.find(candidate => candidate.id === relatedEntryId) : null;
        if (relatedEntryId && (!current || current.trashed || !eligible.some(candidate => candidate.id === relatedEntryId))) {
            choices.push({
                id: relatedEntryId,
                label: !current ? KI18n.i18n("Linked item unavailable") : current.trashed ? KI18n.i18nc("@item:inlistbox %1 title, %2 item type", "%1 · %2 — In the trash", current.title, kindName(current.kind)) : KI18n.i18nc("@item:inlistbox %1 title, %2 item type", "%1 · %2 — Unavailable", current.title, kindName(current.kind)),
                unavailable: true
            });
        }
        return choices;
    }
    readonly property var selectedOption: options.find(option => option.id === relatedEntryId) || options[0]
    readonly property var filteredOptions: relatedEntryDialog.filteredOptions

    signal selectionChanged(string relatedEntryId)
    signal externalModalRequested(bool opened)

    function kindName(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option ? option.name : KI18n.i18nc("@label item type", "Unknown type");
    }

    function kindIcon(kind) {
        const option = kindOptions.find(candidate => candidate.id === kind);
        return option?.icon || "document-encrypted";
    }

    function folderPath(folderId) {
        const names = [];
        const visited = new Set();
        let id = folderId || "";
        while (id && !visited.has(id)) {
            visited.add(id);
            const folder = folders.find(candidate => candidate.id === id);
            if (!folder)
                break;
            names.unshift(folder.name);
            id = folder.parent || "";
        }
        return names.length ? names.join(" / ") : KI18n.i18nc("@label folder name", "No folder");
    }

    function selectRelatedEntry(id) {
        const option = options.find(candidate => candidate.id === (id || ""));
        if (!option || option.unavailable)
            return false;
        selectionChanged(option.id);
        return true;
    }

    function openPicker() {
        if (!enabled)
            return;
        relatedEntryDialog.resetSelection();
        relatedEntryDialog.open();
    }

    function closePicker() {
        relatedEntryDialog.close();
    }

    onEnabledChanged: {
        if (!enabled)
            closePicker();
    }
    onVisibleChanged: {
        if (!visible)
            closePicker();
    }

    spacing: 16

    Controls.Label {
        Layout.fillWidth: true
        text: KI18n.i18nc("@title", "Linked item")
        font.weight: Font.DemiBold
    }

    RowLayout {
        Layout.fillWidth: true
        visible: !!root.relatedEntryId
        spacing: Kirigami.Units.smallSpacing
        EntryIcon {
            visible: !root.selectedOption.unavailable
            entryIcon: root.selectedOption.icon || null
            imageSource: root.selectedOption.iconSource || ""
            fallbackIcon: root.kindIcon(root.selectedOption.kind || "")
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: width
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                objectName: "relatedEntrySummary"
                Layout.fillWidth: true
                text: root.selectedOption.title || root.selectedOption.label
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !root.selectedOption.unavailable
                text: root.selectedOption.subtitle || ""
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Kirigami.Theme.disabledTextColor
            }
        }
    }

    RowLayout {
        Layout.fillWidth: true
        Controls.Button {
            id: relatedEntryButton
            objectName: "relatedEntryButton"
            text: root.relatedEntryId ? KI18n.i18nc("@action:button", "Change Link…") : KI18n.i18nc("@action:button", "Link Item…")
            icon.name: "link"
            onClicked: root.openPicker()
        }
        Controls.Button {
            objectName: "unlinkRelatedEntryButton"
            visible: !!root.relatedEntryId
            text: KI18n.i18nc("@action:button", "Unlink")
            onClicked: root.selectRelatedEntry("")
        }
        Item {
            Layout.fillWidth: true
        }
    }

    Controls.Label {
        Layout.fillWidth: true
        text: KI18n.i18n("Link a related item. Both items will show this relationship.")
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: Kirigami.Theme.disabledTextColor
    }

    EntryPickerDialog {
        id: relatedEntryDialog
        objectName: "relatedEntryDialog"
        searchObjectName: "relatedEntrySearch"
        folderPickerObjectName: "relatedEntryFolderPicker"
        listObjectName: "relatedEntryList"
        candidateIconObjectName: "relatedEntryCandidateIcon"
        cancelButtonObjectName: "cancelRelatedEntryButton"
        confirmButtonObjectName: "confirmRelatedEntryButton"
        title: KI18n.i18nc("@title:window", "Link Item")
        confirmText: KI18n.i18nc("@action:button", "Link")
        cancelText: KI18n.i18nc("@action:button", "Cancel")
        options: root.options
        kindOptions: root.kindOptions
        folderRows: root.folderRows
        selectedId: root.relatedEntryId
        onSelectionRequested: function (entryId) {
            root.selectRelatedEntry(entryId);
        }
        onModalVisibilityChanged: function (opened) {
            root.externalModalRequested(opened);
            if (!opened && root.visible && root.enabled)
                relatedEntryButton.forceActiveFocus();
        }
    }
}
