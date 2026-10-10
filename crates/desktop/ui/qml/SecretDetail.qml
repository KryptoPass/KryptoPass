pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root

    property var entry: null
    property bool revealed: false
    readonly property string kind: entry && entry.kind ? entry.kind : "note"
    readonly property string copyValue: {
        switch (root.kind) {
        case "card":
            return qsTr("Número de tarjeta: %1\nTitular: %2\nVencimiento: %3\nCVV: %4")
                .arg(root.detail("cardNumber"))
                .arg(root.detail("cardHolder"))
                .arg(root.detail("expiry"))
                .arg(root.detail("cvv"));
        case "identity":
            return qsTr("Nombre: %1\nDocumento: %2\nFecha de nacimiento: %3\nCorreo: %4\nTeléfono: %5")
                .arg(root.detail("fullName"))
                .arg(root.detail("document"))
                .arg(root.detail("birthDate"))
                .arg(root.detail("email"))
                .arg(root.detail("phone"));
        case "login":
            return qsTr("URL: %1\nUsuario: %2\nContraseña: %3")
                .arg(root.detail("url"))
                .arg(root.detail("username"))
                .arg(root.detail("password"));
        default:
            return entry?.value || "";
        }
    }

    signal revealToggled()
    signal copyRequested(string value)

    function detail(name) {
        return entry && entry.details && entry.details[name] ? entry.details[name] : "";
    }

    function hiddenValue(value) {
        if (!value)
            return qsTr("Sin datos");
        return root.revealed ? value : qsTr("Contenido oculto");
    }

    function maskedCardNumber(value) {
        if (!value)
            return qsTr("Sin número");
        if (root.revealed)
            return value;
        const digits = value.replace(/\s+/g, "");
        return "•••• •••• •••• " + digits.slice(-4);
    }

    Layout.fillWidth: true
    spacing: Kirigami.Units.largeSpacing

    RowLayout {
        Layout.fillWidth: true
        Controls.Button {
            text: root.revealed ? qsTr("Ocultar datos") : qsTr("Mostrar datos")
            icon.name: root.revealed ? "view-hidden" : "view-visible"
            onClicked: root.revealToggled()
        }
        Controls.Button {
            text: qsTr("Copiar")
            icon.name: "edit-copy"
            enabled: root.revealed && root.copyValue.length > 0
            onClicked: root.copyRequested(root.copyValue)
        }
        Item {
            Layout.fillWidth: true
        }
    }

    StackLayout {
        id: body
        Layout.fillWidth: true
        currentIndex: root.kind === "card" ? 0 : root.kind === "identity" ? 1 : root.kind === "login" ? 2 : 3

        ColumnLayout {
            Layout.fillWidth: true
            Controls.Frame {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Controls.Label {
                        text: qsTr("Número de tarjeta")
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.maskedCardNumber(root.detail("cardNumber"))
                        font.pixelSize: Kirigami.Theme.defaultFont.pixelSize * 1.25
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.revealed ? root.detail("cardHolder") : qsTr("Titular oculto")
                        font.bold: true
                        elide: Text.ElideRight
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        ColumnLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                text: qsTr("Vencimiento")
                                color: Kirigami.Theme.disabledTextColor
                            }
                            Controls.Label {
                                text: root.hiddenValue(root.detail("expiry"))
                            }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                text: qsTr("CVV")
                                color: Kirigami.Theme.disabledTextColor
                            }
                            Controls.Label {
                                text: root.hiddenValue(root.detail("cvv"))
                            }
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Controls.Frame {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.largeSpacing
                    RowLayout {
                        Layout.fillWidth: true
                        Kirigami.Icon {
                            source: "user-identity"
                            Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                            Layout.preferredHeight: width
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: root.revealed ? root.detail("fullName") : qsTr("Identidad oculta")
                            font.bold: true
                            elide: Text.ElideRight
                        }
                    }
                    Kirigami.FormLayout {
                        Layout.fillWidth: true
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Documento:")
                            text: root.hiddenValue(root.detail("document"))
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Nacimiento:")
                            text: root.hiddenValue(root.detail("birthDate"))
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Correo:")
                            text: root.hiddenValue(root.detail("email"))
                            elide: Text.ElideRight
                        }
                        Controls.Label {
                            Kirigami.FormData.label: qsTr("Teléfono:")
                            text: root.hiddenValue(root.detail("phone"))
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Controls.Frame {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing
                contentItem: Kirigami.FormLayout {
                    Controls.Label {
                        Kirigami.FormData.label: qsTr("Sitio web:")
                        text: root.hiddenValue(root.detail("url"))
                        elide: Text.ElideRight
                    }
                    Controls.Label {
                        Kirigami.FormData.label: qsTr("Usuario:")
                        text: root.hiddenValue(root.detail("username"))
                        elide: Text.ElideRight
                    }
                    Controls.Label {
                        Kirigami.FormData.label: qsTr("Contraseña:")
                        text: root.hiddenValue(root.detail("password"))
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Controls.Frame {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing
                contentItem: ColumnLayout {
                    Controls.Label {
                        text: qsTr("Contenido")
                        color: Kirigami.Theme.disabledTextColor
                    }
                    Controls.TextArea {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 6)
                        readOnly: true
                        selectByMouse: true
                        wrapMode: TextEdit.Wrap
                        text: root.revealed ? root.entry?.value || qsTr("Sin contenido") : qsTr("Contenido oculto")
                    }
                }
            }
        }
    }
}
