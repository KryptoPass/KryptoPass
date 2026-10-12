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
            value: root.fields ? root.fields.value || qsTranslate("SecretDetail", "Sin contenido") : ""
            accessibleLabel: qsTranslate("SecretDetail", "Contenido de la nota segura")
            minimumContentHeight: Kirigami.Units.gridUnit * 6
        }
        SecretTextSection {
            Layout.fillWidth: true
            visible: !!value
            heading: qsTranslate("SecretDetail", "Notas")
            value: root.fields?.notes || ""
            accessibleLabel: qsTranslate("SecretDetail", "Notas del elemento")
        }
    }
}
