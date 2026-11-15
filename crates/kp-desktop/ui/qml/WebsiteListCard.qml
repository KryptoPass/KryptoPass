pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Frame {
    id: root

    required property var urls
    property string heading: qsTranslate("SecretDetail", "Sitios web")
    property string badge: ""

    readonly property var websiteUrls: Array.isArray(urls) ? urls.map(value => String(value || "")) : []

    signal copyUrlRequested(int index)

    function safeWebUrl(value) {
        const candidate = String(value || "").trim();
        if (!candidate || /[\u0000-\u0020]/.test(candidate))
            return "";
        const match = /^https?:\/\/([^/?#\s]+)(?:[/?#][^\s]*)?$/i.exec(candidate);
        return match && match[1] ? candidate : "";
    }

    padding: Kirigami.Units.largeSpacing
    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing
            Controls.Label {
                text: root.heading
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            FieldBadge {
                Layout.alignment: Qt.AlignVCenter
                text: root.badge
            }
            Item {
                Layout.fillWidth: true
            }
        }
        Repeater {
            model: root.websiteUrls
            delegate: ColumnLayout {
                id: websiteDelegate
                required property string modelData
                required property int index
                readonly property string webUrl: root.safeWebUrl(modelData)
                readonly property string domain: webUrl ? webUrl.replace(/^https?:\/\//i, "").split(/[/?#]/)[0] : qsTranslate("SecretDetail", "Enlace no válido")
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            Layout.fillWidth: true
                            text: websiteDelegate.domain
                            textFormat: Text.PlainText
                            elide: Text.ElideMiddle
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            text: websiteDelegate.modelData
                            textFormat: Text.PlainText
                            color: Kirigami.Theme.disabledTextColor
                            elide: Text.ElideMiddle
                            Accessible.name: websiteDelegate.modelData
                        }
                    }
                    Controls.ToolButton {
                        text: qsTranslate("SecretDetail", "Abrir sitio web")
                        icon.name: "external-link-symbolic"
                        display: Controls.AbstractButton.IconOnly
                        enabled: !!websiteDelegate.webUrl
                        Accessible.name: qsTranslate("SecretDetail", "Abrir %1 en el navegador").arg(websiteDelegate.domain)
                        Accessible.description: websiteDelegate.modelData
                        onClicked: Qt.openUrlExternally(websiteDelegate.webUrl)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: websiteDelegate.modelData
                    }
                    Controls.ToolButton {
                        text: qsTranslate("SecretDetail", "Copiar sitio web")
                        icon.name: "edit-copy"
                        display: Controls.AbstractButton.IconOnly
                        enabled: !!websiteDelegate.modelData
                        Accessible.name: qsTranslate("SecretDetail", "Copiar %1").arg(websiteDelegate.domain)
                        Accessible.description: websiteDelegate.modelData
                        onClicked: root.copyUrlRequested(websiteDelegate.index)
                        Controls.ToolTip.visible: hovered || visualFocus
                        Controls.ToolTip.text: text
                    }
                }
                Kirigami.Separator {
                    Layout.fillWidth: true
                    visible: websiteDelegate.index < root.websiteUrls.length - 1
                }
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: root.websiteUrls.length === 0
            text: qsTranslate("SecretDetail", "No hay sitios web guardados.")
            wrapMode: Text.Wrap
        }
    }
}
