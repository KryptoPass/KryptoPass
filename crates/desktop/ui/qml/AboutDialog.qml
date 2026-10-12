pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop as Backend

Controls.Dialog {
    id: aboutDialog
    anchors.centerIn: parent
    modal: true
    title: qsTr("Acerca de %1").arg(Qt.application.name)
    width: Math.min(parent.width - Kirigami.Units.largeSpacing * 4, Kirigami.Units.gridUnit * 30)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.largeSpacing * 4)
    standardButtons: Controls.Dialog.Close

    Backend.ShellController { id: appInfo }

    contentItem: Controls.ScrollView {
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: parent.width
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Heading {
                Layout.fillWidth: true
                text: appInfo.appName
                level: 1
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Versión %1").arg(appInfo.appVersion)
            }
            Controls.Label {
                Layout.fillWidth: true
                text: appInfo.appDescription
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("El contenido sensible se cifra antes de guardarse. Los nombres, las carpetas y las huellas de las claves de acceso se guardan sin cifrar.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Licencia: %1\nAutores: %2\n© %3").arg(appInfo.appLicense).arg(appInfo.appAuthors).arg(appInfo.appCopyright)
                wrapMode: Text.Wrap
            }
            Controls.Button {
                text: qsTr("Abrir sitio del proyecto")
                icon.name: "internet-services"
                onClicked: Qt.openUrlExternally(appInfo.appHomepage)
            }
            Controls.Button {
                text: qsTr("Informar de un problema…")
                icon.name: "tools-report-bug"
                onClicked: Qt.openUrlExternally(appInfo.appRepository + "/issues")
            }
        }
    }
}
