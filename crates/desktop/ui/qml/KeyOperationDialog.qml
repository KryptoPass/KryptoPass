pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import QtQuick.Dialogs as Dialogs
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: dialog

    required property KeyStore keyStore
    property string action: ""
    property string fingerprint: ""
    property string userId: ""
    property string subkeyFingerprint: ""
    property var subkeys: []
    property var userIds: []
    property bool privateMaterialAvailable: false
    property string filePath: ""
    property bool submitted: false
    property bool privateBackupConfirmed: false
    property bool consequenceConfirmed: false
    property Item dialogParent: Controls.Overlay.overlay
    property string operationError: ""
    property string operationWarning: ""
    readonly property var expiryOptions: [
        { label: qsTr("30 días"), days: 30 },
        { label: qsTr("90 días"), days: 90 },
        { label: qsTr("180 días"), days: 180 },
        { label: qsTr("1 año"), days: 365 },
        { label: qsTr("2 años"), days: 730 },
        { label: qsTr("5 años"), days: 1825 },
        { label: qsTr("Sin caducidad"), days: 0 }
    ]
    readonly property var usageOptions: [
        { label: qsTr("Cifrar"), value: "encrypt" },
        { label: qsTr("Firmar"), value: "sign" },
        { label: qsTr("Autenticar"), value: "authenticate" }
    ]
    readonly property var materialOptions: privateMaterialAvailable ? [
        { label: qsTr("Pública y privada"), value: "both" },
        { label: qsTr("Solo privada"), value: "private" }
    ] : [
        { label: qsTr("Solo pública"), value: "public" }
    ]
    readonly property var certifierOptions: keyStore.entries
        .filter(key => key.hasSecret && key.canCertify && key.id.toUpperCase() !== normalizedFingerprint(fingerprint))
        .map(key => ({ label: (key.title || qsTr("Sin nombre")) + " · " + key.id, fingerprint: key.id }))
    readonly property bool controllerBusy: keyStore.controller.busy || keyStore.controller.operationBusy
    readonly property bool destructive: action === "revokeKey" || action === "revokeIdentity"
        || action === "revokeSubkey" || action === "delete"
    readonly property bool needsFile: action === "import" || action === "exportPublic"
        || action === "backupPrivate" || action === "prepareRevocation"
    readonly property bool privateBackupReady: action !== "backupPrivate" || privateBackupConfirmed
    readonly property bool consequenceReady: !destructive || consequenceConfirmed
    readonly property bool canSubmit: !controllerBusy && !submitted && !completionMessage
        && fingerprintReady() && privateBackupReady && consequenceReady && operationInputsReady()
    property string completionMessage: ""

    parent: dialogParent
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 36, parent ? parent.width - Kirigami.Units.gridUnit * 4 : 720)
    height: Math.min(implicitHeight, parent ? parent.height - Kirigami.Units.gridUnit * 4 : 640)
    modal: true
    closePolicy: Controls.Popup.NoAutoClose
    title: titleForAction()

    function normalizedFingerprint(value) {
        return (value || "").replace(/\s+/g, "").toUpperCase();
    }

    function isFullFingerprint(value) {
        return /^(?:[A-F0-9]{40}|[A-F0-9]{64})$/.test(normalizedFingerprint(value));
    }

    function requestFingerprint() {
        return normalizedFingerprint(fingerprint || fingerprintField.text);
    }

    function fingerprintReady() {
        if (action === "create" || action === "import")
            return true;
        return isFullFingerprint(requestFingerprint());
    }

    function selectedExpiryDays() {
        return expiryCombo.currentIndex >= 0 ? expiryOptions[expiryCombo.currentIndex].days : 0;
    }

    function operationInputsReady() {
        if (action === "create")
            return !!userIdField.text.trim();
        if (needsFile)
            return !!filePath;
        if (action === "addIdentity")
            return !!userIdField.text.trim();
        if (action === "primaryIdentity" || action === "revokeIdentity")
            return !!userId;
        if (action === "revokeSubkey")
            return isFullFingerprint(subkeyFingerprint);
        if (action === "addSubkey")
            return usageCombo.currentIndex >= 0;
        if (action === "certify")
            return userIds.length > 0 && certifierCombo.currentIndex >= 0 && certifierOptions.length > 0;
        return true;
    }

    function titleForAction() {
        const titles = {
            create: qsTr("Crear clave OpenPGP"),
            import: qsTr("Importar clave OpenPGP"),
            exportPublic: qsTr("Exportar clave pública"),
            copyPublic: qsTr("Copiar clave pública"),
            backupPrivate: qsTr("Respaldar clave privada"),
            passphrase: qsTr("Cambiar passphrase"),
            expiry: qsTr("Cambiar caducidad"),
            addIdentity: qsTr("Añadir identidad"),
            primaryIdentity: qsTr("Elegir identidad principal"),
            revokeIdentity: qsTr("Revocar identidad"),
            addSubkey: qsTr("Añadir subclave"),
            revokeSubkey: qsTr("Revocar subclave"),
            prepareRevocation: qsTr("Crear certificado de revocación"),
            revokeKey: qsTr("Revocar clave"),
            certify: qsTr("Certificar identidad"),
            refreshExternal: qsTr("Actualizar desde fuente externa"),
            delete: qsTr("Eliminar clave del llavero")
        };
        return titles[action] || qsTr("Operación OpenPGP");
    }

    function operationButtonText() {
        const labels = {
            create: qsTr("Crear clave"),
            import: qsTr("Importar clave"),
            exportPublic: qsTr("Exportar pública"),
            copyPublic: qsTr("Copiar pública"),
            backupPrivate: qsTr("Guardar respaldo privado"),
            passphrase: qsTr("Abrir diálogo seguro"),
            expiry: qsTr("Guardar caducidad"),
            addIdentity: qsTr("Añadir identidad"),
            primaryIdentity: qsTr("Elegir como principal"),
            revokeIdentity: qsTr("Revocar identidad"),
            addSubkey: qsTr("Añadir subclave"),
            revokeSubkey: qsTr("Revocar subclave"),
            prepareRevocation: qsTr("Guardar certificado"),
            revokeKey: qsTr("Revocar clave"),
            certify: qsTr("Certificar identidad"),
            refreshExternal: qsTr("Actualizar desde fuente"),
            delete: qsTr("Eliminar del llavero")
        };
        return labels[action] || qsTr("Continuar");
    }

    function requestObject() {
        const request = { action: action };
        if (action !== "create" && action !== "import")
            request.fingerprint = requestFingerprint();
        if (action === "create") {
            request.userid = userIdField.text.trim();
            request.expiresDays = selectedExpiryDays();
        } else if (action === "import" || action === "exportPublic" || action === "backupPrivate" || action === "prepareRevocation") {
            request.path = filePath;
        } else if (action === "expiry") {
            request.subkeys = subkeys.slice();
            request.expiresDays = selectedExpiryDays();
        } else if (action === "addIdentity") {
            request.userid = userIdField.text.trim();
        } else if (action === "primaryIdentity" || action === "revokeIdentity") {
            request.userid = userId;
        } else if (action === "addSubkey") {
            request.usage = usageOptions[usageCombo.currentIndex].value;
            request.expiresDays = selectedExpiryDays();
        } else if (action === "revokeSubkey") {
            request.subkey = subkeyFingerprint;
        } else if (action === "certify") {
            request.signerFingerprint = certifierOptions[certifierCombo.currentIndex].fingerprint;
            request.userids = userIds.slice();
            request.local = localCertificationCheck.checked;
        } else if (action === "delete") {
            request.material = materialOptions[materialCombo.currentIndex].value;
        }
        return request;
    }

    function openFor(actionName, sourceFingerprint, sourceUserId, sourceSubkey, sourceUserIds) {
        action = actionName;
        fingerprint = normalizedFingerprint(sourceFingerprint || "");
        userId = sourceUserId || "";
        subkeyFingerprint = normalizedFingerprint(sourceSubkey || "");
        subkeys = subkeyFingerprint ? [subkeyFingerprint] : [];
        userIds = sourceUserIds ? sourceUserIds.slice() : (userId ? [userId] : []);
        const sourceKey = keyStore.entries.find(key => key.id.toUpperCase() === normalizedFingerprint(fingerprint));
        privateMaterialAvailable = !!sourceKey?.hasSecret;
        filePath = "";
        submitted = false;
        privateBackupConfirmed = false;
        consequenceConfirmed = false;
        completionMessage = "";
        operationError = "";
        operationWarning = "";
        userIdField.text = "";
        fingerprintField.text = "";
        keyStore.controller.clearOperationError();
        Qt.callLater(function() {
            expiryCombo.currentIndex = 3;
            usageCombo.currentIndex = 0;
            certifierCombo.currentIndex = certifierOptions.length ? 0 : -1;
            materialCombo.currentIndex = 0;
            localCertificationCheck.checked = true;
            if (action === "create" || action === "addIdentity")
                userIdField.forceActiveFocus();
            else if (action === "refreshExternal" && !fingerprint)
                fingerprintField.forceActiveFocus();
        });
        open();
    }

    function submit() {
        if (!canSubmit)
            return;
        keyStore.controller.clearOperationError();
        operationError = "";
        operationWarning = "";
        submitted = true;
        completionMessage = "";
        keyStore.controller.runOperation(JSON.stringify(requestObject()));
    }

    function successMessage(message) {
        if (message)
            return message;
        return qsTr("Operación completada.");
    }

    Shortcut {
        sequences: [StandardKey.Cancel]
        enabled: dialog.opened && !dialog.controllerBusy
        onActivated: dialog.reject()
    }

    Connections {
        target: dialog.keyStore.controller
        function onOperationFinished(finishedFingerprint, message) {
            if (!dialog.submitted)
                return;
            dialog.submitted = false;
            if (finishedFingerprint)
                dialog.keyStore.selectAfterRefresh(finishedFingerprint);
            dialog.operationWarning = String(dialog.keyStore.controller.operationError || "");
            dialog.completionMessage = dialog.successMessage(message);
        }
        function onOperationFailed(message) {
            if (!dialog.submitted)
                return;
            dialog.submitted = false;
            dialog.operationWarning = "";
            dialog.operationError = message || dialog.keyStore.controller.operationError;
        }
        function onPublicKeyReady(armored) {
            if (!dialog.submitted || dialog.action !== "copyPublic" || !armored)
                return;
            publicKeyClipboardSource.text = armored;
            publicKeyClipboardSource.selectAll();
            publicKeyClipboardSource.copy();
            publicKeyClipboardSource.deselect();
            publicKeyClipboardSource.text = "";
        }
    }

    TextEdit {
        id: publicKeyClipboardSource
        visible: false
        readOnly: true
        textFormat: TextEdit.PlainText
    }

    contentItem: Controls.ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        implicitHeight: Math.min(form.implicitHeight + Kirigami.Units.largeSpacing,
                                 dialog.parent ? dialog.parent.height - Kirigami.Units.gridUnit * 10 : 480)

        ColumnLayout {
            id: form
            width: scroll.availableWidth
            spacing: Kirigami.Units.largeSpacing

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.operationError
                type: Kirigami.MessageType.Error
                text: dialog.operationError
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.operationWarning
                type: Kirigami.MessageType.Warning
                text: dialog.operationWarning
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: !!dialog.completionMessage
                type: Kirigami.MessageType.Positive
                text: dialog.completionMessage
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: !!(dialog.fingerprint || fingerprintField.text) && dialog.action !== "create" && dialog.action !== "import"
                text: qsTr("Huella completa: %1").arg(dialog.requestFingerprint())
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!dialog.userId
                text: qsTr("Identidad objetivo: %1").arg(dialog.userId)
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: !!dialog.subkeyFingerprint
                text: qsTr("Subclave objetivo: %1").arg(dialog.subkeyFingerprint)
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "create"
                text: qsTr("GnuPG puede pedir la passphrase en su diálogo seguro. La passphrase no se escribe ni se guarda en KryptoPass.")
                wrapMode: Text.Wrap
            }
            Controls.TextField {
                id: userIdField
                Layout.fillWidth: true
                visible: dialog.action === "create" || dialog.action === "addIdentity"
                enabled: !dialog.controllerBusy
                placeholderText: qsTr("Nombre <correo@example.org>")
                Accessible.name: qsTr("Nombre y correo de la identidad OpenPGP")
                onAccepted: if (dialog.canSubmit) dialog.submit()
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "create"
                text: qsTr("Caducidad inicial de la clave")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: expiryCombo
                Layout.fillWidth: true
                visible: dialog.action === "create" || dialog.action === "expiry" || dialog.action === "addSubkey"
                enabled: !dialog.controllerBusy
                model: dialog.expiryOptions
                textRole: "label"
                currentIndex: 3
                Accessible.name: qsTr("Caducidad")
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "import"
                text: qsTr("Elige un archivo OpenPGP para importar. El llavero solo se modifica con el contenido del archivo; ninguna identidad se acepta por su correo.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "exportPublic"
                text: qsTr("Se exportará la clave pública completa, incluidas sus identidades y subclaves públicas.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "backupPrivate"
                text: qsTr("El archivo contendrá material de clave privada. Conserva la protección actual de GnuPG; si la clave no tiene frase de contraseña, el respaldo tampoco estará protegido por una. Guárdalo en un lugar privado.")
                wrapMode: Text.Wrap
            }
            Controls.CheckBox {
                id: privateBackupCheck
                visible: dialog.action === "backupPrivate"
                enabled: !dialog.controllerBusy
                checked: dialog.privateBackupConfirmed
                text: qsTr("Entiendo que el archivo contiene material privado y conserva la protección actual de la clave")
                onToggled: dialog.privateBackupConfirmed = checked
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "passphrase"
                text: qsTr("GnuPG abrirá su diálogo seguro de pinentry. Escribe la passphrase allí; KryptoPass no la solicita ni la recibe.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "expiry"
                text: dialog.subkeys.length ? qsTr("La nueva caducidad se aplicará solo a la subclave seleccionada.") : qsTr("La nueva caducidad se aplicará a la clave primaria. Elige “Sin caducidad” para quitar la fecha de vencimiento.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "addIdentity"
                text: qsTr("Añade una identidad a esta clave propia. Usa un nombre y correo que quieras publicar en OpenPGP.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "primaryIdentity"
                text: qsTr("La identidad elegida se usará como identidad principal de la clave.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeIdentity"
                text: qsTr("Se revocará esta identidad de forma permanente. La identidad dejará de ser válida para la clave.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "addSubkey"
                text: qsTr("Añade una subclave de uso específico. GnuPG puede abrir pinentry para autorizar el cambio.")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: usageCombo
                Layout.fillWidth: true
                visible: dialog.action === "addSubkey"
                enabled: !dialog.controllerBusy
                model: dialog.usageOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: qsTr("Uso de la nueva subclave")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeSubkey"
                text: qsTr("Se revocará solo la subclave mostrada. La clave primaria no se revocará.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "prepareRevocation"
                text: qsTr("Guarda y custodia el certificado fuera del equipo: quien lo tenga podrá revocar la clave. Crear el certificado no la revoca todavía.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "revokeKey"
                text: qsTr("La revocación de la clave primaria es permanente y afecta a todas sus identidades y subclaves.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                text: qsTr("Elige una clave propia que pueda certificar y revisa la identidad que firmarás. Una certificación no confirma que hayas verificado a su propietario.")
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                text: dialog.userIds.join("\n")
                textFormat: Text.PlainText
                font: Kirigami.Theme.fixedWidthFont
                wrapMode: Text.WrapAnywhere
            }
            Controls.ComboBox {
                id: certifierCombo
                Layout.fillWidth: true
                visible: dialog.action === "certify"
                enabled: !dialog.controllerBusy
                model: dialog.certifierOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: qsTr("Clave propia que certificará esta identidad")
            }
            Controls.CheckBox {
                id: localCertificationCheck
                visible: dialog.action === "certify"
                enabled: !dialog.controllerBusy
                checked: true
                text: qsTr("Mantener la certificación local, sin exportarla")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal"
                text: qsTr("Consulta la fuente externa configurada usando una huella OpenPGP completa. No se busca ni se confía por nombre o correo.")
                wrapMode: Text.Wrap
            }
            Controls.TextField {
                id: fingerprintField
                Layout.fillWidth: true
                visible: dialog.action === "refreshExternal" && !dialog.fingerprint
                enabled: !dialog.controllerBusy
                placeholderText: qsTr("Huella completa de 40 o 64 caracteres hexadecimales")
                font: Kirigami.Theme.fixedWidthFont
                Accessible.name: qsTr("Huella OpenPGP completa para recuperar una clave externa")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.action === "delete"
                text: qsTr("El borrado afecta solo al llavero local y no revoca la clave ni retira copias que otras personas ya tengan. Si borras la parte privada sin respaldo, perderás acceso a los secretos de esta clave.")
                wrapMode: Text.Wrap
            }
            Controls.ComboBox {
                id: materialCombo
                Layout.fillWidth: true
                visible: dialog.action === "delete"
                enabled: !dialog.controllerBusy
                model: dialog.materialOptions
                textRole: "label"
                currentIndex: 0
                Accessible.name: qsTr("Material que se eliminará del llavero")
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.needsFile
                text: dialog.filePath ? dialog.filePath : qsTr("No se ha elegido un archivo.")
                textFormat: Text.PlainText
                color: dialog.filePath ? Kirigami.Theme.textColor : Kirigami.Theme.disabledTextColor
                wrapMode: Text.WrapAnywhere
            }
            Controls.Button {
                Layout.alignment: Qt.AlignLeft
                visible: dialog.needsFile
                enabled: !dialog.controllerBusy
                text: dialog.action === "import" ? qsTr("Elegir archivo OpenPGP…") : qsTr("Elegir destino…")
                icon.name: dialog.action === "import" ? "document-open" : "document-save-as"
                onClicked: fileDialog.open()
            }

            Controls.Label {
                Layout.fillWidth: true
                visible: dialog.destructive
                text: dialog.destructiveDescription()
                wrapMode: Text.Wrap
            }
            Controls.CheckBox {
                id: consequenceCheck
                visible: dialog.destructive
                enabled: !dialog.controllerBusy
                checked: dialog.consequenceConfirmed
                text: dialog.action === "delete"
                    ? qsTr("Entiendo que esto solo elimina material local y no revoca la clave")
                    : qsTr("Entiendo la consecuencia permanente de esta revocación")
                onToggled: dialog.consequenceConfirmed = checked
            }
        }
    }

    function destructiveDescription() {
        if (action === "revokeKey")
            return qsTr("Revocación permanente de la clave primaria %1 y de todas sus identidades y subclaves.").arg(fingerprint);
        if (action === "revokeIdentity")
            return qsTr("Se revocará permanentemente la identidad “%1” de la clave %2.").arg(userId).arg(fingerprint);
        if (action === "revokeSubkey")
            return qsTr("Se revocará permanentemente la subclave %1 de la clave %2.").arg(subkeyFingerprint).arg(fingerprint);
        const material = materialOptions[materialCombo.currentIndex]?.label || qsTr("material seleccionado");
        if (privateMaterialAvailable && materialOptions[materialCombo.currentIndex]?.value === "private")
            return qsTr("Se eliminará la parte privada de la clave %1 y se conservará la parte pública. Sin respaldo no podrás volver a usar esta clave para descifrar o firmar. El borrado local no revoca la clave y una copia de respaldo puede restaurarla.").arg(fingerprint);
        return qsTr("Se eliminará %1 de la clave %2 del llavero local.").arg(material).arg(fingerprint);
    }

    footer: Controls.DialogButtonBox {
        Controls.Button {
            text: qsTr("Cerrar")
            enabled: !dialog.controllerBusy
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.RejectRole
            onClicked: dialog.reject()
        }
        Controls.Button {
            visible: !dialog.completionMessage
            text: dialog.operationButtonText()
            icon.name: dialog.destructive ? "dialog-warning" : "dialog-ok"
            enabled: dialog.canSubmit
            Controls.DialogButtonBox.buttonRole: Controls.DialogButtonBox.ActionRole
            onClicked: dialog.submit()
        }
    }

    Dialogs.FileDialog {
        id: fileDialog
        title: dialog.action === "import" ? qsTr("Seleccionar clave OpenPGP para importar") : qsTr("Elegir destino de archivo OpenPGP")
        fileMode: dialog.action === "import" ? Dialogs.FileDialog.OpenFile : Dialogs.FileDialog.SaveFile
        defaultSuffix: dialog.action === "backupPrivate" ? "gpg" : "asc"
        nameFilters: [qsTr("Archivos OpenPGP (*.asc *.gpg *.pgp *.key)"), qsTr("Todos los archivos (*)")]
        onAccepted: dialog.filePath = selectedFile.toString()
    }

    onRejected: {
        if (!controllerBusy)
            close();
    }
    onClosed: {
        submitted = false;
        operationError = "";
        operationWarning = "";
        keyStore.controller.clearOperationError();
        publicKeyClipboardSource.text = "";
    }
}
