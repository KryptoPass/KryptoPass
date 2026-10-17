pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

SecretContentView {
    id: root
    implicitHeight: content.implicitHeight

    ColumnLayout {
        id: content
        width: root.width
        spacing: Kirigami.Units.largeSpacing

        SecretTextSection {
            Layout.fillWidth: true
            heading: qsTranslate("SecretDetail", "Contenido")
            value: root.fields?.value || ""
            documentKey: root.entryId
            documentObjectName: "markdownDetailDocument"
            viewObjectName: "markdownDetailView"
            emptyObjectName: "markdownEmptyState"
            emptyText: qsTranslate("SecretDetail", "Esta nota no tiene contenido.")
            accessibleLabel: qsTranslate("SecretDetail", "Contenido Markdown de la nota segura")
        }

        SecretTextSection {
            objectName: "markdownNotesSection"
            Layout.fillWidth: true
            visible: value.length > 0
            heading: qsTranslate("SecretDetail", "Notas adicionales")
            value: root.fields?.notes || ""
            documentKey: root.entryId
            documentObjectName: "markdownNotesDetailDocument"
            viewObjectName: "markdownNotesDetailView"
            accessibleLabel: qsTranslate("SecretDetail", "Markdown de las notas adicionales de la nota segura")
        }
    }
}
