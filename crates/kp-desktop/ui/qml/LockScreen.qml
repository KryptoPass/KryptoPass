pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami

// Covers the vault while it is locked so names, folders and counts stay hidden.
Rectangle {
    id: root

    required property KeyStore keyStore

    property bool unlockEnabled: true
    property bool unlocking: false
    property bool canReviewKey: false
    property string primaryFingerprint: ""
    property string errorMessage: ""
    property string localError: ""
    property bool importing: false
    property string reviewFingerprint: ""
    property bool reviewImportedFromFile: false

    readonly property bool keyBusy: importing || keyStore.controller.busy || keyStore.controller.operationBusy
    readonly property var reviewKey: reviewFingerprint ? (keyStore.entries.find(key => key.id.toUpperCase() === reviewFingerprint.toUpperCase()) || null) : null

    signal unlockRequested
    signal restoreRequested
    signal quitRequested

    function openReview(fingerprint, imported) {
        if (!fingerprint)
            return;
        localError = "";
        reviewImportedFromFile = imported;
        reviewFingerprint = fingerprint;
    }

    function closeReview() {
        reviewFingerprint = "";
        reviewImportedFromFile = false;
        localError = "";
    }

    function importKey(url) {
        if (keyBusy)
            return;
        localError = "";
        importing = true;
        keyStore.controller.clearOperationError();
        keyStore.controller.runOperation(JSON.stringify({
            action: "import",
            path: url.toString()
        }));
    }

    Connections {
        target: root.keyStore.controller
        function onOperationFinished(fingerprint, message) {
            if (!root.importing)
                return;
            root.importing = false;
            if (fingerprint)
                root.openReview(fingerprint, true);
            else
                root.localError = qsTr("La importación no devolvió ninguna clave.");
        }
        function onOperationFailed(message) {
            if (!root.importing)
                return;
            root.importing = false;
            root.keyStore.controller.clearOperationError();
            if (!message.toLowerCase().includes("cancel"))
                root.localError = message;
        }
    }

    color: Kirigami.Theme.backgroundColor
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    onVisibleChanged: {
        if (visible)
            Qt.callLater(unlockButton.forceActiveFocus);
    }

    // Swallows pointer input so nothing behind the cover reacts.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    ColumnLayout {
        visible: root.reviewFingerprint.length === 0
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 26)
        spacing: Kirigami.Units.largeSpacing

        Kirigami.Icon {
            Layout.alignment: Qt.AlignHCenter
            source: "object-locked"
            implicitWidth: Kirigami.Units.iconSizes.enormous
            implicitHeight: Kirigami.Units.iconSizes.enormous
            color: Kirigami.Theme.disabledTextColor
        }
        Kirigami.Heading {
            Layout.fillWidth: true
            level: 1
            text: qsTr("%1 está bloqueado").arg(Application.displayName)
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
        }
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            type: Kirigami.MessageType.Warning
            text: root.localError || root.errorMessage
        }
        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            visible: root.importing
            spacing: Kirigami.Units.smallSpacing
            Controls.BusyIndicator {
                running: root.importing
                implicitWidth: Kirigami.Units.iconSizes.smallMedium
                implicitHeight: implicitWidth
                Accessible.name: qsTr("Importando clave")
            }
            Controls.Label {
                text: qsTr("Importando clave…")
                color: Kirigami.Theme.disabledTextColor
            }
        }
        Controls.Button {
            id: unlockButton
            Layout.alignment: Qt.AlignHCenter
            Layout.topMargin: Kirigami.Units.largeSpacing
            text: root.unlocking ? qsTr("Desbloqueando…") : qsTr("Desbloquear")
            icon.name: "object-unlocked"
            highlighted: true
            enabled: root.unlockEnabled && !root.unlocking && !root.importing
            onClicked: {
                root.localError = "";
                root.unlockRequested();
            }
            Keys.onReturnPressed: {
                root.localError = "";
                root.unlockRequested();
            }
            Keys.onEnterPressed: {
                root.localError = "";
                root.unlockRequested();
            }
        }
        Flow {
            Layout.alignment: Qt.AlignHCenter
            Layout.fillWidth: true
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            spacing: Kirigami.Units.smallSpacing

            Controls.Button {
                visible: root.canReviewKey
                text: qsTr("Revisar clave…")
                icon.name: "view-certificate"
                enabled: !root.keyBusy
                onClicked: root.openReview(root.primaryFingerprint, false)
            }
            Controls.Button {
                text: qsTr("Importar clave privada…")
                icon.name: "document-import"
                enabled: !root.keyBusy
                onClicked: importDialog.open()
            }
            Controls.Button {
                text: qsTr("Restaurar copia de seguridad…")
                icon.name: "document-revert"
                enabled: !root.keyBusy
                onClicked: root.restoreRequested()
            }
        }
        Controls.ToolButton {
            Layout.alignment: Qt.AlignHCenter
            visible: (root.localError.length > 0 || root.errorMessage.length > 0) && !root.unlocking
            text: qsTr("Salir")
            icon.name: "application-exit"
            enabled: !root.keyBusy
            onClicked: root.quitRequested()
        }
    }

    KeyReviewPage {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 29)
        visible: root.reviewFingerprint.length > 0
        title: root.reviewImportedFromFile ? qsTr("Revisa la clave importada") : qsTr("Revisa la clave de la bóveda")
        subtitle: root.reviewImportedFromFile ? qsTr("Verifica la huella digital y marca la clave como propia para desbloquear la bóveda.") : qsTr("Marca la clave como propia para desbloquear la bóveda.")
        selectedKey: root.reviewKey
        keyStore: root.keyStore
        importedFromFile: root.reviewImportedFromFile
        onBackRequested: root.closeReview()
        onAccepted: {
            root.closeReview();
            root.unlockRequested();
        }
    }

    Dialogs.FileDialog {
        id: importDialog
        title: qsTr("Seleccionar clave OpenPGP para importar")
        fileMode: Dialogs.FileDialog.OpenFile
        nameFilters: [qsTr("Archivos OpenPGP (*.asc *.gpg *.pgp *.key)"), qsTr("Todos los archivos (*)")]
        onAccepted: root.importKey(selectedFile)
    }
}
