import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

Kirigami.ApplicationWindow {
    id: root

    width: 1280
    height: 838
    minimumWidth: 560
    minimumHeight: 620
    visible: true
    title: qsTr("KryptoPass")

    AppController {
        id: appController
    }

    globalDrawer: Kirigami.GlobalDrawer {
        title: qsTr("KryptoPass")
        actions: [
            Kirigami.Action {
                text: qsTr("Inicio")
                icon.name: "go-home-symbolic"
                onTriggered: {
                    appController.selectSection("home")
                    root.showPassiveNotification(appController.bridgeStatus)
                }
            },
            Kirigami.Action {
                text: qsTr("Bóveda")
                icon.name: "document-encrypt-symbolic"
                onTriggered: {
                    appController.selectSection("vault")
                    root.showPassiveNotification(appController.bridgeStatus)
                }
            },
            Kirigami.Action {
                text: qsTr("Acerca de")
                icon.name: "help-about-symbolic"
                onTriggered: root.showPassiveNotification(qsTr("Demo Kirigami + Rust + CXX-Qt"))
            }
        ]
    }

    pageStack.initialPage: Kirigami.Page {
        title: qsTr("Desbloqueo")

        actions: [
            Kirigami.Action {
                text: qsTr("Comprobar")
                icon.name: "list-add-symbolic"
                onTriggered: {
                    appController.runHealthCheck()
                    root.showPassiveNotification(appController.bridgeStatus)
                }
            }
        ]

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.largeSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 1
                text: qsTr("Hola Kirigami en KryptoPass")
            }

            Controls.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: appController.bridgeStatus
            }

            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Pruebas del puente: %1 · Sección: %2").arg(appController.healthChecks).arg(appController.activeSection)
            }

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                Controls.Button {
                    text: qsTr("Comprobar integración")
                    onClicked: {
                        appController.runHealthCheck()
                        root.showPassiveNotification(appController.bridgeStatus)
                    }
                }

                Controls.Button {
                    text: qsTr("Restablecer")
                    onClicked: appController.resetDemo()
                }
            }

            Kirigami.Card {
                Layout.fillWidth: true
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.smallSpacing

                    Kirigami.Heading {
                        level: 2
                        text: qsTr("Puente Rust/QML")
                    }

                    Controls.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        text: appController.bridgeReady ? qsTr("Rust conectado") : qsTr("Sin conexión")
                    }
                }
            }

            Item {
                Layout.fillHeight: true
            }
        }
    }
}
