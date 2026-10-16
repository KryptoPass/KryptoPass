pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

SecretContentView {
    id: root
    implicitHeight: content.implicitHeight

    readonly property string contentText: root.fields?.value || ""
    readonly property string notesText: root.fields?.notes || ""
    readonly property bool effectivelyVisible: {
        let item = root;
        while (item) {
            if (!item.visible)
                return false;
            item = item.parent;
        }
        return true;
    }

    function clearDocuments() {
        contentView.clearSelection();
        notesView.clearSelection();
        contentDocument.clear();
        notesDocument.clear();
    }

    function reloadDocuments() {
        root.clearDocuments();
        if (!root.effectivelyVisible)
            return;
        const fields = root.fields || {};
        if (fields.value)
            contentDocument.load(fields.value);
        if (fields.notes)
            notesDocument.load(fields.notes);
    }

    onFieldsChanged: reloadDocuments()
    onEntryIdChanged: reloadDocuments()
    onEffectivelyVisibleChanged: reloadDocuments()
    Component.onCompleted: reloadDocuments()
    Component.onDestruction: clearDocuments()

    MarkdownDocument {
        id: contentDocument
        objectName: "markdownDetailDocument"
    }
    MarkdownDocument {
        id: notesDocument
        objectName: "markdownNotesDetailDocument"
    }

    ColumnLayout {
        id: content
        width: root.width
        spacing: Kirigami.Units.largeSpacing

        Controls.Frame {
            Layout.fillWidth: true
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTranslate("SecretDetail", "Contenido")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    objectName: "markdownEmptyState"
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                    visible: root.contentText.length === 0
                    text: qsTranslate("SecretDetail", "Esta nota no tiene contenido.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                    verticalAlignment: Text.AlignVCenter
                }
                MarkdownView {
                    id: contentView
                    objectName: "markdownDetailView"
                    Layout.fillWidth: true
                    visible: root.contentText.length > 0
                    document: contentDocument
                    accessibleLabel: qsTranslate("SecretDetail", "Contenido Markdown de la nota segura")
                    onLinkRequested: link => Qt.openUrlExternally(link)
                }
            }
        }

        Controls.Frame {
            Layout.fillWidth: true
            visible: root.notesText.length > 0
            padding: Kirigami.Units.largeSpacing
            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTranslate("SecretDetail", "Notas adicionales")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                MarkdownView {
                    id: notesView
                    objectName: "markdownNotesDetailView"
                    Layout.fillWidth: true
                    document: notesDocument
                    accessibleLabel: qsTranslate("SecretDetail", "Markdown de las notas adicionales de la nota segura")
                    onLinkRequested: link => Qt.openUrlExternally(link)
                }
            }
        }
    }
}
