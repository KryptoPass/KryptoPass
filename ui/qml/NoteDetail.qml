pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.ki18n

SecretContentView {
    id: root
    implicitHeight: content.implicitHeight

    ColumnLayout {
        id: content
        width: root.width
        spacing: Kirigami.Units.largeSpacing

        SecretTextSection {
            Layout.fillWidth: true
            heading: KI18n.i18nc("@title:group", "Content")
            value: root.fields?.value || ""
            documentKey: root.entryId
            documentObjectName: "markdownDetailDocument"
            viewObjectName: "markdownDetailView"
            emptyObjectName: "markdownEmptyState"
            emptyText: KI18n.i18n("This note has no content.")
            accessibleLabel: KI18n.i18n("Markdown content of the secure note")
        }

        SecretTextSection {
            objectName: "markdownNotesSection"
            Layout.fillWidth: true
            visible: value.length > 0
            heading: KI18n.i18nc("@title:group", "Additional notes")
            value: root.fields?.notes || ""
            documentKey: root.entryId
            documentObjectName: "markdownNotesDetailDocument"
            viewObjectName: "markdownNotesDetailView"
            accessibleLabel: KI18n.i18n("Markdown of the secure note's additional notes")
        }
    }
}
