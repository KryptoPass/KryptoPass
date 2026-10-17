pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: root
    title: qsTr("Ayuda de %1").arg(Qt.application.name)
    modal: true
    margins: Kirigami.Units.largeSpacing
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 36, parent.width - Kirigami.Units.largeSpacing * 2)
    height: Math.min(Kirigami.Units.gridUnit * 32, parent.height - Kirigami.Units.largeSpacing * 2)
    standardButtons: Controls.Dialog.Close

    contentItem: Controls.ScrollView {
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            width: parent.width
            spacing: Kirigami.Units.largeSpacing
            Repeater {
                model: [
                    {
                        title: qsTr("Organizar la bóveda"),
                        body: qsTr("En Carpetas, selecciona una vista o carpeta y usa Archivo → Nuevo elemento para guardar credenciales o notas. Selecciona un elemento para consultar su detalle y usa Editar para cambiarlo. La búsqueda filtra la vista actual.")
                    },
                    {
                        title: qsTr("Claves OpenPGP"),
                        body: qsTr("En Ver → Claves puedes consultar el llavero local de GnuPG, crear o importar claves y copiar su huella. Las operaciones de claves se habilitan en esa sección.")
                    },
                    {
                        title: qsTr("Bloqueo y portapapeles"),
                        body: qsTr("Usa Ver → Bloquear bóveda para cerrar el acceso al contenido. La aplicación también se bloquea con la sesión, antes de suspender y tras el tiempo de inactividad configurado. En Preferencias puedes ajustar ese tiempo y la limpieza automática del portapapeles.")
                    },
                    {
                        title: qsTr("Copias de seguridad"),
                        body: qsTr("Archivo → Crear copia de seguridad exporta la bóveda cifrada para los destinatarios que elijas. Conserva las claves privadas necesarias para abrirla. Usa Archivo → Restaurar copia de seguridad para recuperar una copia.")
                    },
                    {
                        title: qsTr("Ventanas estrechas y teclado"),
                        body: qsTr("Cuando hay poco espacio, se muestra un panel a la vez. Usa los botones de la barra para cambiar de panel. Puedes recorrer los controles con Tab y activar los botones con Espacio. F1 abre esta ayuda.")
                    },
                    {
                        title: qsTr("Informar de un problema"),
                        body: qsTr("Ayuda → Informar de un error abre el gestor de incidencias de KryptoPass. Describe los pasos y la versión de la aplicación. Evita incluir contraseñas, claves privadas o contenido de la bóveda.")
                    }
                ]
                ColumnLayout {
                    id: section
                    required property var modelData
                    Layout.fillWidth: true
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 3
                        text: section.modelData.title
                        wrapMode: Text.WordWrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: section.modelData.body
                        textFormat: Text.PlainText
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }
    }
}
