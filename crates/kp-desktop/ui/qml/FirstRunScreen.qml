pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

// First run of this installation: choose or create the vault key, then back it up.
Rectangle {
    id: root

    required property KeyStore keyStore
    property url logo: ""
    signal restoreRequested
    signal handbookRequested(string section)
    // `dedicated` marks a key created here, which can be forgotten on lock safely.
    signal setupFinished(string vaultKey, bool dedicated)

    property int step: 0
    property string chosenKey: ""
    property bool creating: false
    property bool backedUp: false
    property string errorText: ""
    readonly property var ownKeys: keyStore.entries.filter(key => key.statusCode === "valid" && key.canEncrypt && key.hasSecret)
    readonly property bool busy: creating || keyStore.controller.operationBusy

    onStepChanged: errorText = ""

    color: Kirigami.Theme.backgroundColor
    Kirigami.Theme.colorSet: Kirigami.Theme.View
    Kirigami.Theme.inherit: false

    function createVaultKey() {
        if (busy)
            return;
        errorText = "";
        creating = true;
        keyStore.controller.runOperation(JSON.stringify({
            action: "create",
            userid: Application.displayName,
            expiresDays: 0
        }));
    }

    function useExistingKey() {
        const key = ownKeys[existingKeys.currentIndex];
        if (key)
            setupFinished(key.id, false);
    }

    Connections {
        target: root.keyStore.controller
        function onOperationFinished(fingerprint, message) {
            if (!root.creating)
                return;
            root.creating = false;
            root.chosenKey = fingerprint;
            root.step = 2;
        }
        function onOperationFailed(message) {
            if (!root.creating)
                return;
            root.creating = false;
            // The error belongs to this screen; keep it out of the Keys view.
            root.keyStore.controller.clearOperationError();
            if (!message.toLowerCase().includes("cancel"))
                root.errorText = message;
        }
    }

    // Swallows pointer input so nothing behind the cover reacts.
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onWheel: wheel => wheel.accepted = true
    }

    StackLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - Kirigami.Units.gridUnit * 4, Kirigami.Units.gridUnit * 26)
        height: Math.min(parent.height - Kirigami.Units.gridUnit * 2, implicitHeight)
        currentIndex: root.step

        // 0 — welcome
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                Layout.alignment: Qt.AlignHCenter
                source: root.logo.toString() || "security-high"
                fallback: "security-high"
                implicitWidth: Kirigami.Units.iconSizes.enormous
                implicitHeight: Kirigami.Units.iconSizes.enormous
            }
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 1
                text: qsTr("Te damos la bienvenida a %1").arg(Application.displayName)
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Controls.Label {
                    text: qsTr("Contraseñas y notas cifradas con OpenPGP, solo en tu equipo.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
            }
            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Kirigami.Units.largeSpacing
                text: qsTr("Empezar")
                icon.name: "go-next"
                highlighted: true
                onClicked: root.step = 1
            }
            Controls.ToolButton {
                Layout.alignment: Qt.AlignHCenter
                text: qsTr("Restaurar una copia de seguridad…")
                icon.name: "document-revert"
                onClicked: root.restoreRequested()
            }
        }

        // 1 — vault key: two clear alternatives, the dedicated key first.
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                Layout.alignment: Qt.AlignHCenter
                source: "view-certificate"
                implicitWidth: Kirigami.Units.iconSizes.huge
                implicitHeight: Kirigami.Units.iconSizes.huge
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                Kirigami.Heading {
                    level: 1
                    text: qsTr("Clave de la bóveda")
                }
                HelpButton {
                    help: qsTr("Todo lo que guardes se cifra con esta clave. Solo quien tiene su parte privada y conoce su frase puede abrir la bóveda.")
                }
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: root.errorText.length > 0
                type: Kirigami.MessageType.Error
                text: root.errorText
            }

            Controls.ButtonGroup {
                id: keyChoice
            }
            Controls.ItemDelegate {
                id: createChoice
                // The chosen row carries the highlight, so hover never looks like a selection.
                highlighted: checked
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.largeSpacing
                checkable: true
                checked: true
                enabled: !root.busy
                Controls.ButtonGroup.group: keyChoice
                Accessible.name: qsTr("Crear una clave para %1").arg(Application.displayName)
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Controls.RadioButton {
                        Layout.alignment: Qt.AlignTop
                        checked: createChoice.checked
                        focusPolicy: Qt.NoFocus
                        onClicked: createChoice.checked = true
                    }
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignTop
                        source: "view-certificate-add"
                        selected: createChoice.highlighted
                        implicitWidth: Kirigami.Units.iconSizes.medium
                        implicitHeight: Kirigami.Units.iconSizes.medium
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            // A custom contentItem does not inherit the highlighted text color.
                            Controls.Label {
                                text: qsTr("Crear una clave para %1").arg(Application.displayName)
                                font.weight: Font.DemiBold
                                color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                            }
                            Controls.Label {
                                text: qsTr("Recomendada")
                                color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.positiveTextColor
                                font: Kirigami.Theme.smallFont
                            }
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTr("Una clave solo para tus contraseñas. Al bloquear, %1 la cierra por completo sin afectar a nada más.").arg(Application.displayName)
                            wrapMode: Text.Wrap
                            color: createChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                            opacity: createChoice.highlighted ? 0.85 : 1
                        }
                    }
                }
            }
            Controls.ItemDelegate {
                id: existingChoice
                // The chosen row carries the highlight, so hover never looks like a selection.
                highlighted: checked
                Layout.fillWidth: true
                checkable: true
                enabled: !root.busy
                Controls.ButtonGroup.group: keyChoice
                Accessible.name: qsTr("Usar una de mis claves")
                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Controls.RadioButton {
                        Layout.alignment: Qt.AlignTop
                        checked: existingChoice.checked
                        focusPolicy: Qt.NoFocus
                        onClicked: existingChoice.checked = true
                    }
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignTop
                        source: "view-certificate"
                        selected: existingChoice.highlighted
                        implicitWidth: Kirigami.Units.iconSizes.medium
                        implicitHeight: Kirigami.Units.iconSizes.medium
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            text: qsTr("Usar una de mis claves")
                            font.weight: Font.DemiBold
                            color: existingChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            visible: root.ownKeys.length === 0
                            text: qsTr("No hay claves privadas en este equipo.")
                            wrapMode: Text.Wrap
                            color: existingChoice.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.disabledTextColor
                            opacity: existingChoice.highlighted ? 0.85 : 1
                        }
                        Controls.ComboBox {
                            id: existingKeys
                            Layout.fillWidth: true
                            visible: root.ownKeys.length > 0
                            enabled: existingChoice.checked && !root.busy
                            model: root.ownKeys.map(key => (key.title || key.email) + " · " + key.id.slice(-8))
                            Accessible.name: qsTr("Clave existente")
                        }
                        Controls.ToolButton {
                            text: qsTr("Importar una clave privada…")
                            icon.name: "document-import"
                            enabled: !root.busy
                            onClicked: {
                                existingChoice.checked = true;
                                keyOperation.openFor("import");
                            }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.largeSpacing
                Controls.Button {
                    text: qsTr("Atrás")
                    icon.name: "go-previous"
                    enabled: !root.busy
                    onClicked: root.step = 0
                }
                Item {
                    Layout.fillWidth: true
                }
                Controls.Button {
                    text: root.creating ? qsTr("Creando la clave…") : qsTr("Continuar")
                    icon.name: "go-next"
                    highlighted: true
                    enabled: !root.busy && (createChoice.checked || (existingChoice.checked && existingKeys.currentIndex >= 0 && root.ownKeys.length > 0))
                    onClicked: {
                        if (createChoice.checked)
                            root.createVaultKey();
                        else
                            root.useExistingKey();
                    }
                }
            }
        }

        // 2 — back up the new key
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                Layout.alignment: Qt.AlignHCenter
                source: root.backedUp ? "dialog-ok-apply" : "document-save"
                implicitWidth: Kirigami.Units.iconSizes.huge
                implicitHeight: Kirigami.Units.iconSizes.huge
            }
            Kirigami.Heading {
                Layout.fillWidth: true
                level: 1
                text: root.backedUp ? qsTr("Copia guardada") : qsTr("Guarda una copia de tu clave")
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                visible: !root.backedUp
                Controls.Label {
                    text: qsTr("Sin ella no podrás abrir la bóveda si pierdes este equipo.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
            }
            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: Kirigami.Units.largeSpacing
                visible: !root.backedUp
                text: qsTr("Guardar copia de la clave…")
                icon.name: "document-save-as"
                highlighted: true
                enabled: !root.busy
                onClicked: keyOperation.openFor("backupPrivate", root.chosenKey)
            }
            Controls.Button {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: root.backedUp ? Kirigami.Units.largeSpacing : 0
                text: root.backedUp ? qsTr("Abrir la bóveda") : qsTr("Más tarde")
                icon.name: root.backedUp ? "go-next" : ""
                highlighted: root.backedUp
                flat: !root.backedUp
                enabled: !root.busy
                onClicked: root.setupFinished(root.chosenKey, true)
            }
        }
    }

    // One discreet way into the handbook instead of a "?" on every line.
    Controls.ToolButton {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Kirigami.Units.gridUnit
        text: qsTr("¿Necesitas ayuda?")
        icon.name: "help-contents"
        onClicked: root.handbookRequested("first-run")
    }

    KeyOperationDialog {
        id: keyOperation
        keyStore: root.keyStore
        onClosed: {
            if (action === "backupPrivate" && completionMessage)
                root.backedUp = true;
        }
    }
}
