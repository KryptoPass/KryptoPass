pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

ColumnLayout {
    id: root
    required property var selectedKey
    property bool expanded: false
    property string displayedId: ""
    readonly property var identities: (selectedKey?.identities || []).slice().sort((a, b) => Number(a.revoked || a.invalid) - Number(b.revoked || b.invalid) || a.id.localeCompare(b.id))
    readonly property var signatures: (selectedKey?.signatures || []).slice().sort((a, b) => a.signer.localeCompare(b.signer) || a.keyId.localeCompare(b.keyId) || a.identity.localeCompare(b.identity))
    spacing: 0
    onSelectedKeyChanged: {
        const id = selectedKey?.id || "";
        if (id !== displayedId) {
            tabs.currentIndex = 0;
            expanded = false;
            displayedId = id;
        }
    }
    function copyFingerprint() {
        clipboardSource.selectAll();
        clipboardSource.copy();
        clipboardSource.deselect();
    }
    function groupedFingerprint(value) {
        return (value || "").replace(/(.{4})(?=.)/g, "$1 ");
    }
    function expiration(value) {
        return value ? new Date(value * 1000).toLocaleDateString(Qt.locale(), Locale.ShortFormat) : qsTr("Sin caducidad");
    }
    TextEdit {
        id: clipboardSource
        visible: false
        readOnly: true
        textFormat: TextEdit.PlainText
        text: root.selectedKey?.id || ""
    }
    Controls.TabBar {
        id: tabs
        Layout.fillWidth: true
        visible: !!root.selectedKey
        Controls.TabButton {
            text: qsTr("Identidad")
        }
        Controls.TabButton {
            text: qsTr("Firmas")
        }
    }
    Controls.ScrollView {
        id: detailScroll
        Layout.fillWidth: true
        Layout.fillHeight: true
        contentWidth: availableWidth
        clip: true
        ColumnLayout {
            width: detailScroll.availableWidth
            spacing: Kirigami.Units.largeSpacing
            visible: !!root.selectedKey
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                Layout.bottomMargin: 0
                spacing: Kirigami.Units.largeSpacing
                KeyAvatar {
                    Layout.preferredWidth: size
                    Layout.preferredHeight: size
                    Layout.alignment: Qt.AlignTop
                    size: Kirigami.Units.iconSizes.huge
                    source: root.selectedKey?.photoSource || ""
                    fallbackIcon: "user-identity"
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        level: 2
                        text: root.selectedKey?.title || ""
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        text: root.selectedKey?.email || qsTr("Sin correo electrónico")
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: Kirigami.Theme.disabledTextColor
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: tabs.currentIndex === 0
                spacing: Kirigami.Units.largeSpacing
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Controls.Label {
                        Kirigami.FormData.label: qsTr("Tipo:")
                        Layout.fillWidth: true
                        text: root.selectedKey?.hasSecret ? qsTr("Clave propia (parte privada disponible)") : qsTr("Destinatario (solo clave pública)")
                        wrapMode: Text.Wrap
                    }
                    KeyStatus {
                        Kirigami.FormData.label: qsTr("Estado:")
                        Layout.fillWidth: true
                        code: root.selectedKey?.statusCode || ""
                        text: root.selectedKey?.status || ""
                    }
                    Controls.Label {
                        Kirigami.FormData.label: qsTr("Caducidad:")
                        text: root.expiration(root.selectedKey?.subkeys[0]?.expiresAt)
                    }
                }
                Kirigami.Heading {
                    level: 3
                    text: qsTr("Identidades (%1)").arg(root.identities.length)
                }
                Repeater {
                    model: root.identities
                    delegate: ColumnLayout {
                        id: identityDelegate
                        required property var modelData
                        Layout.fillWidth: true
                        Controls.Label {
                            Layout.fillWidth: true
                            text: identityDelegate.modelData.id
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                        }
                        IdentityValidity {
                            Layout.fillWidth: true
                            code: identityDelegate.modelData.validityCode || "unknown"
                            text: identityDelegate.modelData.validity || ""
                        }
                    }
                }
                Controls.Label {
                    text: qsTr("Huella de la clave")
                }
                Controls.TextArea {
                    id: fingerprint
                    Layout.fillWidth: true
                    readOnly: true
                    selectByMouse: true
                    textFormat: TextEdit.PlainText
                    wrapMode: TextEdit.Wrap
                    font: Kirigami.Theme.fixedWidthFont
                    text: root.groupedFingerprint(root.selectedKey?.id)
                    Accessible.name: qsTr("Huella completa de la clave")
                }
                Kirigami.Separator {
                    Layout.fillWidth: true
                    weight: Kirigami.Separator.Weight.Light
                }
                Controls.ToolButton {
                    text: qsTr("Detalles criptográficos")
                    icon.name: root.expanded ? "arrow-down" : "arrow-right"
                    checkable: true
                    checked: root.expanded
                    onClicked: root.expanded = !root.expanded
                    Accessible.description: qsTr("Mostrar u ocultar la clave principal y sus subclaves")
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    visible: root.expanded
                    spacing: Kirigami.Units.largeSpacing
                    Repeater {
                        model: root.selectedKey?.subkeys || []
                        delegate: ColumnLayout {
                            id: subkeyDelegate
                            required property var modelData
                            required property int index
                            Layout.fillWidth: true
                            Controls.Label {
                                text: subkeyDelegate.index === 0 ? qsTr("Clave principal") : qsTr("Subclave %1").arg(subkeyDelegate.index)
                                font.bold: true
                            }
                            Kirigami.FormLayout {
                                Layout.fillWidth: true
                                Controls.Label {
                                    Kirigami.FormData.label: qsTr("Algoritmo:")
                                    text: subkeyDelegate.modelData.algorithm
                                    textFormat: Text.PlainText
                                }
                                Controls.Label {
                                    Kirigami.FormData.label: qsTr("Usos:")
                                    Layout.fillWidth: true
                                    text: subkeyDelegate.modelData.capabilities.join(", ") || qsTr("Sin usos disponibles")
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                }
                                KeyStatus {
                                    Kirigami.FormData.label: qsTr("Estado:")
                                    Layout.fillWidth: true
                                    code: subkeyDelegate.modelData.statusCode || ""
                                    text: subkeyDelegate.modelData.status
                                }
                                Controls.Label {
                                    Kirigami.FormData.label: qsTr("Caducidad:")
                                    text: root.expiration(subkeyDelegate.modelData.expiresAt)
                                }
                            }
                            Controls.Label {
                                Layout.fillWidth: true
                                text: root.groupedFingerprint(subkeyDelegate.modelData.fingerprint)
                                font: Kirigami.Theme.fixedWidthFont
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                            Kirigami.Separator {
                                Layout.fillWidth: true
                                weight: Kirigami.Separator.Weight.Light
                                visible: subkeyDelegate.index < (root.selectedKey?.subkeys.length || 0) - 1
                            }
                        }
                    }
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                visible: tabs.currentIndex === 1
                spacing: Kirigami.Units.largeSpacing
                Kirigami.Heading {
                    Layout.fillWidth: true
                    level: 3
                    text: qsTr("Certificaciones (%1)").arg(root.signatures.length)
                }
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTr("Certificaciones de terceros registradas en el anillo local. Una firma no implica que hayas verificado la identidad de su propietario.")
                    wrapMode: Text.Wrap
                }
                Controls.Label {
                    Layout.fillWidth: true
                    visible: !root.signatures.length
                    text: qsTr("No hay firmas de terceros registradas para esta clave.")
                    wrapMode: Text.Wrap
                    color: Kirigami.Theme.disabledTextColor
                }
                Repeater {
                    model: root.signatures
                    delegate: ColumnLayout {
                        id: signatureDelegate
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Controls.Label {
                            Layout.fillWidth: true
                            text: signatureDelegate.modelData.signer || qsTr("Firmante sin identidad disponible")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            font.bold: true
                        }
                        Kirigami.FormLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                Kirigami.FormData.label: qsTr("ID del firmante:")
                                Layout.fillWidth: true
                                text: signatureDelegate.modelData.keyId || qsTr("No disponible")
                                textFormat: Text.PlainText
                                font: Kirigami.Theme.fixedWidthFont
                                wrapMode: Text.Wrap
                            }
                            Controls.Label {
                                Kirigami.FormData.label: qsTr("Estado:")
                                Layout.fillWidth: true
                                text: signatureDelegate.modelData.status
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                            Controls.Label {
                                Kirigami.FormData.label: qsTr("Identidad firmada:")
                                Layout.fillWidth: true
                                text: signatureDelegate.modelData.identity
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                        }
                        Kirigami.Separator {
                            Layout.fillWidth: true
                            weight: Kirigami.Separator.Weight.Light
                            visible: signatureDelegate.index < root.signatures.length - 1
                        }
                    }
                }
            }
        }
    }
}
