pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: root

    property var versionDetail: null
    property var versions: []
    property string currentTitle: ""

    readonly property int versionPosition: {
        if (!root.versionDetail)
            return -1;
        return root.versions.findIndex(version => version.index === root.versionDetail.index);
    }
    readonly property int versionCount: root.versions.length
    readonly property var olderVersion: root.versionPosition >= 0 && root.versionPosition < root.versionCount - 1 ? root.versions[root.versionPosition + 1] : null
    readonly property var newerVersion: root.versionPosition > 0 ? root.versions[root.versionPosition - 1] : null
    readonly property bool titleChanged: !!root.versionDetail && root.versionDetail.title !== root.currentTitle
    readonly property string dateText: root.formatTimestamp(root.versionDetail?.at) || qsTranslate("SecretDetail", "Fecha no disponible")

    signal revealRequested
    signal copyRequested(string field)
    signal copyUrlRequested(int index)
    signal openVersionRequested(int index)
    signal versionClosed

    function formatTimestamp(timestamp) {
        const date = new Date(Number(timestamp) * 1000);
        return isNaN(date.getTime()) ? "" : Qt.formatDateTime(date, "dd MMM yyyy, HH:mm");
    }

    function changed(field) {
        return Array.isArray(root.versionDetail?.changedFields) && root.versionDetail.changedFields.includes(field);
    }

    parent: Controls.Overlay.overlay
    x: parent ? (parent.width - width) / 2 : 0
    y: parent ? (parent.height - height) / 2 : 0
    width: Math.min(Kirigami.Units.gridUnit * 34, parent ? parent.width - Kirigami.Units.largeSpacing * 2 : 620)
    height: Math.min(implicitHeight, parent ? parent.height * 0.85 : 720)
    modal: true
    closePolicy: Controls.Popup.CloseOnEscape
    title: qsTranslate("SecretDetail", "Versión anterior")
    onVersionDetailChanged: {
        if (versionDetail) {
            if (!root.opened)
                root.open();
        } else if (root.opened) {
            root.close();
        }
    }
    onClosed: root.versionClosed()

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.smallSpacing
        Accessible.role: Accessible.Dialog
        Accessible.name: root.title

        Controls.Label {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            Layout.topMargin: Kirigami.Units.smallSpacing
            text: qsTranslate("SecretDetail", "%1 · Versión %2 de %3").arg(root.dateText).arg(root.versionPosition + 1).arg(root.versionCount)
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }
        Controls.Label {
            Layout.fillWidth: true
            Layout.leftMargin: Kirigami.Units.largeSpacing
            Layout.rightMargin: Kirigami.Units.largeSpacing
            text: qsTranslate("SecretDetail", "Solo lectura. Consultar o copiar estos datos no modifica el actual.")
            color: Kirigami.Theme.disabledTextColor
            wrapMode: Text.Wrap
        }

        Controls.ScrollView {
            id: versionScroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            leftPadding: Kirigami.Units.largeSpacing
            rightPadding: Kirigami.Units.largeSpacing
            topPadding: Kirigami.Units.smallSpacing
            bottomPadding: Kirigami.Units.smallSpacing
            implicitHeight: versionFields.implicitHeight

            ColumnLayout {
                id: versionFields
                width: Math.max(0, versionScroll.availableWidth - Kirigami.Units.gridUnit)
                spacing: Kirigami.Units.largeSpacing

                DetailField {
                    Layout.fillWidth: true
                    label: qsTranslate("SecretDetail", "Nombre")
                    value: root.versionDetail?.title || qsTranslate("SecretDetail", "Sin nombre")
                    copyLabel: qsTranslate("SecretDetail", "Copiar nombre")
                    copyEnabled: !!root.versionDetail?.title
                    badge: root.titleChanged ? qsTranslate("SecretDetail", "Modificado") : ""
                    onCopyRequested: root.copyRequested("title")
                }

                GridLayout {
                    Layout.fillWidth: true
                    columns: width >= Kirigami.Units.gridUnit * 24 ? 2 : 1
                    columnSpacing: Kirigami.Units.largeSpacing
                    rowSpacing: Kirigami.Units.smallSpacing

                    DetailField {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        label: qsTranslate("SecretDetail", "Correo o usuario")
                        value: root.versionDetail?.username || qsTranslate("SecretDetail", "Sin usuario")
                        copyLabel: qsTranslate("SecretDetail", "Copiar usuario")
                        copyEnabled: !!root.versionDetail?.username
                        badge: root.changed("username") ? qsTranslate("SecretDetail", "Modificado") : ""
                        onCopyRequested: root.copyRequested("username")
                    }
                    DetailField {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        label: qsTranslate("SecretDetail", "Contraseña")
                        value: !root.versionDetail?.hasPassword ? qsTranslate("SecretDetail", "Sin contraseña") : root.versionDetail?.passwordRevealed ? root.versionDetail?.password || "••••••••••••" : "••••••••••••"
                        copyLabel: qsTranslate("SecretDetail", "Copiar contraseña")
                        copyEnabled: !!root.versionDetail?.hasPassword
                        revealable: !!root.versionDetail?.hasPassword
                        revealed: !!root.versionDetail?.passwordRevealed
                        revealLabel: qsTranslate("SecretDetail", "Mostrar contraseña anterior")
                        hideLabel: qsTranslate("SecretDetail", "Ocultar contraseña anterior")
                        badge: root.changed("password") ? qsTranslate("SecretDetail", "Modificado") : ""
                        onRevealRequested: root.revealRequested()
                        onCopyRequested: root.copyRequested("password")
                    }
                }

                WebsiteListCard {
                    Layout.fillWidth: true
                    visible: (root.versionDetail?.urls || []).length > 0
                    urls: root.versionDetail?.urls || []
                    badge: root.changed("urls") ? qsTranslate("SecretDetail", "Modificado") : ""
                    onCopyUrlRequested: function (index) {
                        root.copyUrlRequested(index);
                    }
                }

                DetailField {
                    Layout.fillWidth: true
                    visible: !!root.versionDetail?.hasTotp
                    label: qsTranslate("SecretDetail", "Código de verificación")
                    value: qsTranslate("SecretDetail", "Configuración de verificación guardada")
                    copyLabel: qsTranslate("SecretDetail", "Copiar configuración anterior")
                    copyEnabled: !!root.versionDetail?.hasTotp
                    badge: root.changed("totp") ? qsTranslate("SecretDetail", "Modificado") : ""
                    onCopyRequested: root.copyRequested("totp")
                }

                SecretTextSection {
                    Layout.fillWidth: true
                    visible: !!root.versionDetail?.notes
                    heading: qsTranslate("SecretDetail", "Notas")
                    value: root.versionDetail?.notes || ""
                    documentKey: "version:" + (root.versionDetail?.index ?? -1)
                    accessibleLabel: qsTranslate("SecretDetail", "Notas de la versión anterior")
                    badge: root.changed("notes") ? qsTranslate("SecretDetail", "Modificado") : ""
                    copyLabel: qsTranslate("SecretDetail", "Copiar notas anteriores")
                    onCopyRequested: root.copyRequested("notes")
                }
            }
        }
    }

    footer: Controls.Pane {
        padding: Kirigami.Units.largeSpacing
        background: null
        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                visible: root.versionCount > 1
                spacing: Kirigami.Units.smallSpacing

                Controls.Button {
                    text: qsTranslate("SecretDetail", "Anterior")
                    icon.name: "go-previous"
                    flat: true
                    enabled: !!root.olderVersion
                    onClicked: root.openVersionRequested(root.olderVersion.index)
                }
                Controls.Label {
                    text: qsTranslate("SecretDetail", "Versión %1 de %2").arg(root.versionPosition + 1).arg(root.versionCount)
                    color: Kirigami.Theme.disabledTextColor
                }
                Controls.Button {
                    text: qsTranslate("SecretDetail", "Siguiente")
                    icon.name: "go-next"
                    flat: true
                    enabled: !!root.newerVersion
                    onClicked: root.openVersionRequested(root.newerVersion.index)
                }
            }

            Item {
                Layout.fillWidth: true
            }

            Controls.Button {
                text: qsTranslate("SecretDetail", "Cerrar")
                onClicked: root.close()
            }
        }
    }
}
