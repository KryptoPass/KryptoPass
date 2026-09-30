import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.kryptopass.desktop
import "components"

ApplicationWindow {
    id: root

    width: 1180
    height: 760
    minimumWidth: 840
    minimumHeight: 620
    visible: true
    title: qsTr("KryptoPass Desktop")
    color: "#f4f5f8"

    readonly property bool compactNavigation: width < 980

    AppController {
        id: appController
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            Layout.fillHeight: true
            Layout.preferredWidth: root.compactNavigation ? 88 : 248
            color: "#ffffff"
            border.width: 0

            Behavior on Layout.preferredWidth {
                NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 20
                spacing: 0

                Row {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 64
                    spacing: 12

                    Rectangle {
                        width: 42
                        height: 42
                        radius: 13
                        gradient: Gradient {
                            GradientStop { position: 0.0; color: "#7868ed" }
                            GradientStop { position: 1.0; color: "#4e3bbf" }
                        }

                        Text {
                            anchors.centerIn: parent
                            text: "K"
                            color: "white"
                            font.pixelSize: 21
                            font.weight: Font.Bold
                        }
                    }

                    Column {
                        visible: !root.compactNavigation
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1

                        Text {
                            text: "KryptoPass"
                            color: "#232735"
                            font.pixelSize: 17
                            font.weight: Font.Bold
                        }

                        Text {
                            text: "DESKTOP"
                            color: "#969baa"
                            font.pixelSize: 9
                            font.letterSpacing: 1.6
                            font.weight: Font.DemiBold
                        }
                    }
                }

                Text {
                    visible: !root.compactNavigation
                    Layout.fillWidth: true
                    Layout.topMargin: 22
                    Layout.bottomMargin: 10
                    text: qsTr("ESPACIO")
                    color: "#a4a8b4"
                    font.pixelSize: 10
                    font.letterSpacing: 1.4
                    font.weight: Font.Bold
                }

                NavItem {
                    Layout.fillWidth: true
                    label: qsTr("Inicio")
                    iconText: "⌂"
                    compact: root.compactNavigation
                    selected: appController.activeSection === "home"
                    onActivated: appController.selectSection("home")
                }

                NavItem {
                    Layout.fillWidth: true
                    label: qsTr("Bóveda")
                    iconText: "◇"
                    compact: root.compactNavigation
                    selected: appController.activeSection === "vault"
                    onActivated: appController.selectSection("vault")
                }

                NavItem {
                    Layout.fillWidth: true
                    label: qsTr("Generador")
                    iconText: "✦"
                    compact: root.compactNavigation
                    selected: appController.activeSection === "generator"
                    onActivated: appController.selectSection("generator")
                }

                NavItem {
                    Layout.fillWidth: true
                    label: qsTr("Auditoría")
                    iconText: "✓"
                    compact: root.compactNavigation
                    selected: appController.activeSection === "audit"
                    onActivated: appController.selectSection("audit")
                }

                Item { Layout.fillHeight: true }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: root.compactNavigation ? 52 : 86
                    radius: 16
                    color: "#f4f2ff"

                    Column {
                        visible: !root.compactNavigation
                        anchors.fill: parent
                        anchors.margins: 14
                        spacing: 5

                        Text {
                            text: qsTr("Base técnica")
                            color: "#4f4290"
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                        }

                        Text {
                            width: parent.width
                            text: qsTr("Rust  ·  CXX-Qt  ·  QML")
                            color: "#8177b2"
                            font.pixelSize: 11
                            wrapMode: Text.WordWrap
                        }
                    }

                    Text {
                        visible: root.compactNavigation
                        anchors.centerIn: parent
                        text: "R"
                        color: "#6c5ce7"
                        font.pixelSize: 18
                        font.weight: Font.Bold
                    }
                }
            }
        }

        Rectangle {
            Layout.preferredWidth: 1
            Layout.fillHeight: true
            color: "#e6e7ec"
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            Flickable {
                anchors.fill: parent
                contentWidth: width
                contentHeight: Math.max(height, contentColumn.implicitHeight + 80)
                clip: true
                boundsBehavior: Flickable.StopAtBounds

                ScrollBar.vertical: ScrollBar {
                    policy: ScrollBar.AsNeeded
                }

                Column {
                    id: contentColumn

                    width: Math.min(parent.width - 72, 1040)
                    anchors.horizontalCenter: parent.horizontalCenter
                    topPadding: 38
                    spacing: 28

                    RowLayout {
                        width: parent.width
                        spacing: 18

                        Column {
                            Layout.fillWidth: true
                            spacing: 4

                            Text {
                                text: qsTr("KryptoPass Desktop")
                                color: "#232735"
                                font.pixelSize: 25
                                font.weight: Font.Bold
                            }

                            Text {
                                text: qsTr("Fundación nativa para una bóveda de credenciales moderna")
                                color: "#747b8b"
                                font.pixelSize: 13
                            }
                        }

                        StatusChip {
                            text: appController.bridgeReady
                                ? qsTr("Rust conectado")
                                : qsTr("Sin conexión")
                            healthy: appController.bridgeReady
                        }
                    }

                    Rectangle {
                        width: parent.width
                        height: 272
                        radius: 28
                        clip: true
                        gradient: Gradient {
                            GradientStop { position: 0.0; color: "#2d2750" }
                            GradientStop { position: 0.55; color: "#443687" }
                            GradientStop { position: 1.0; color: "#6652d0" }
                        }

                        Rectangle {
                            width: 330
                            height: 330
                            radius: 165
                            x: parent.width - 240
                            y: -150
                            color: "#31d6a2"
                            opacity: 0.12
                        }

                        Rectangle {
                            width: 180
                            height: 180
                            radius: 90
                            x: parent.width - 80
                            y: 165
                            color: "#ffffff"
                            opacity: 0.07
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 38
                            anchors.rightMargin: 38
                            spacing: 30

                            Column {
                                Layout.fillWidth: true
                                spacing: 15

                                Text {
                                    width: parent.width
                                    text: qsTr("Tu aplicación de escritorio ya tiene pulso.")
                                    color: "#ffffff"
                                    font.pixelSize: 31
                                    font.weight: Font.Bold
                                    lineHeight: 1.08
                                    wrapMode: Text.WordWrap
                                }

                                Text {
                                    width: parent.width
                                    text: qsTr("La interfaz QML está enlazada con un QObject implementado en Rust. La base está lista para sumar el modelo de bóveda y el cifrado.")
                                    color: "#d8d4f2"
                                    font.pixelSize: 14
                                    lineHeight: 1.35
                                    wrapMode: Text.WordWrap
                                }

                                Row {
                                    spacing: 12

                                    KpButton {
                                        text: qsTr("Comprobar integración")
                                        accentColor: "#31c48d"
                                        foregroundColor: "#172c25"
                                        onClicked: appController.runHealthCheck()
                                    }

                                    KpButton {
                                        text: qsTr("Restablecer")
                                        outlined: true
                                        accentColor: "#d6d0ff"
                                        foregroundColor: "#ffffff"
                                        onClicked: appController.resetDemo()
                                    }
                                }
                            }

                            Rectangle {
                                Layout.preferredWidth: 210
                                Layout.preferredHeight: 190
                                radius: 24
                                color: "#ffffff"
                                opacity: 0.96

                                Column {
                                    anchors.centerIn: parent
                                    spacing: 9

                                    Text {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        text: appController.healthChecks
                                        color: "#443687"
                                        font.pixelSize: 54
                                        font.weight: Font.Bold
                                    }

                                    Text {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        text: qsTr("PRUEBAS DEL PUENTE")
                                        color: "#8a849e"
                                        font.pixelSize: 10
                                        font.letterSpacing: 1.3
                                        font.weight: Font.Bold
                                    }

                                    Rectangle {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        width: 118
                                        height: 6
                                        radius: 3
                                        color: "#ece9f8"

                                        Rectangle {
                                            width: parent.width * Math.min(1, appController.healthChecks / 5)
                                            height: parent.height
                                            radius: parent.radius
                                            color: "#31c48d"

                                            Behavior on width {
                                                NumberAnimation {
                                                    duration: 280
                                                    easing.type: Easing.OutCubic
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    RowLayout {
                        width: parent.width
                        spacing: 16

                        InfoCard {
                            Layout.fillWidth: true
                            eyebrow: qsTr("Siguiente etapa")
                            title: qsTr("Bóveda local")
                            description: qsTr("Modelo cifrado, persistencia segura y bloqueo de sesión.")
                            iconText: "◇"
                            accentColor: "#6c5ce7"
                        }

                        InfoCard {
                            Layout.fillWidth: true
                            eyebrow: qsTr("Siguiente etapa")
                            title: qsTr("Generador")
                            description: qsTr("Contraseñas configurables creadas desde lógica Rust.")
                            iconText: "✦"
                            accentColor: "#168f68"
                        }

                        InfoCard {
                            Layout.fillWidth: true
                            eyebrow: qsTr("Siguiente etapa")
                            title: qsTr("Auditoría")
                            description: qsTr("Alertas de reutilización, fortaleza y credenciales antiguas.")
                            iconText: "✓"
                            accentColor: "#d97727"
                        }
                    }

                    Rectangle {
                        width: parent.width
                        height: 62
                        radius: 16
                        color: "#ffffff"
                        border.width: 1
                        border.color: "#e7e8ee"

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 18
                            anchors.rightMargin: 18
                            spacing: 12

                            Rectangle {
                                Layout.preferredWidth: 10
                                Layout.preferredHeight: 10
                                radius: 5
                                color: "#31c48d"
                            }

                            Text {
                                Layout.fillWidth: true
                                text: appController.bridgeStatus
                                color: "#555d6d"
                                font.pixelSize: 13
                                elide: Text.ElideRight
                            }

                            Text {
                                text: qsTr("SECCIÓN: %1").arg(appController.activeSection.toUpperCase())
                                color: "#9a9fac"
                                font.pixelSize: 10
                                font.letterSpacing: 1.0
                                font.weight: Font.Bold
                            }
                        }
                    }
                }
            }
        }
    }
}

