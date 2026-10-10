import QtQuick
import QtQuick.Controls as Controls

Controls.Dialog {
    id: aboutDialog
    anchors.centerIn: parent
    modal: true
    title: qsTranslate("Main", "Acerca de KryptoPass")
    standardButtons: Controls.Dialog.Close
    Controls.Label {
        width: 400
        wrapMode: Text.Wrap
        text: qsTranslate("Main", "Prototipo de interfaz KDE para organizar secretos en carpetas.\n\nLos elementos de demostración y sus destinatarios son ficticios. Su contenido vive en memoria, no se cifra ni se guarda en disco.\n\nLa sección Claves consulta metadatos reales del llavero GnuPG del usuario. Esta vista no modifica las claves ni muestra material privado.")
    }
}
