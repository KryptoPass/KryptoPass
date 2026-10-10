import QtQuick
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

Controls.Dialog {
    id: keyErrorDialog
    objectName: "keyErrorDialog"
    required property KeyringController controller
    anchors.centerIn: parent
    width: Math.min(640, parent.width - Kirigami.Units.largeSpacing * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.largeSpacing * 4)
    title: qsTranslate("Main", "Error al cargar claves")
    modal: true
    standardButtons: Controls.Dialog.Close
    contentItem: Controls.ScrollView {
        implicitHeight: Math.min(errorText.implicitHeight, Kirigami.Units.gridUnit * 14)
        Controls.TextArea {
            id: errorText
            text: keyErrorDialog.controller.loadError
            readOnly: true
            selectByMouse: true
            textFormat: TextEdit.PlainText
            wrapMode: TextEdit.Wrap
            Accessible.name: qsTranslate("Main", "Detalles del error de GnuPG")
        }
    }
}
