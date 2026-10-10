import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: editor
    objectName: "entryEditor"
    required property MockStore storeAdapter
    required property real windowWidth
    required property Item focusedItem
    signal saveRequested(string entryId, string folderId, string title, string kind, string format, string value, string notes, string recipients, var details)
    property string entryId: ""
    property bool passwordVisible: false
    readonly property int sectionSpacing: Kirigami.Units.smallSpacing
    readonly property int sectionPadding: Kirigami.Units.smallSpacing
    anchors.centerIn: parent
    width: Math.min(680, editor.windowWidth - Kirigami.Units.gridUnit * 4)
    height: Math.min(implicitHeight, parent.height - Kirigami.Units.gridUnit * 4)
    modal: true
    title: entryId ? qsTranslate("Main", "Editar elemento") : qsTranslate("Main", "Nuevo elemento")
    standardButtons: Controls.Dialog.Save | Controls.Dialog.Cancel
    function entryDetail(entry, name) {
        return entry && entry.details && entry.details[name] ? entry.details[name] : "";
    }
    function currentDetails() {
        switch (kindField.currentValue) {
        case "card":
            return {
                cardNumber: cardNumberField.text,
                cardHolder: cardHolderField.text,
                expiry: cardExpiryField.text,
                cvv: cardCvvField.text
            };
        case "identity":
            return {
                fullName: identityNameField.text,
                document: identityDocumentField.text,
                birthDate: identityBirthDateField.text,
                email: identityEmailField.text,
                phone: identityPhoneField.text
            };
        case "login":
            return {
                url: loginUrlField.text,
                username: loginUsernameField.text,
                password: loginPasswordField.text
            };
        default:
            return {};
        }
    }
    function currentValue() {
        return kindField.currentValue === "note" ? valueField.text : "";
    }
    function openEntry(entry) {
        entryId = entry ? entry.id : "";
        titleField.text = entry ? entry.title : "";
        valueField.text = entry ? entry.value : "";
        notesField.text = entry ? entry.notes : "";
        const kind = entry?.kind || "note";
        kindField.currentIndex = Math.max(0, kindField.model.findIndex(k => k.id === kind));
        formatField.currentIndex = entry?.format === "Binario" ? 1 : 0;
        passwordVisible = false;
        cardNumberField.text = entryDetail(entry, "cardNumber");
        cardHolderField.text = entryDetail(entry, "cardHolder");
        cardExpiryField.text = entryDetail(entry, "expiry");
        cardCvvField.text = entryDetail(entry, "cvv");
        identityNameField.text = entryDetail(entry, "fullName");
        identityDocumentField.text = entryDetail(entry, "document");
        identityBirthDateField.text = entryDetail(entry, "birthDate");
        identityEmailField.text = entryDetail(entry, "email");
        identityPhoneField.text = entryDetail(entry, "phone");
        loginUrlField.text = entryDetail(entry, "url");
        loginUsernameField.text = entryDetail(entry, "username");
        loginPasswordField.text = entryDetail(entry, "password");
        folderField.currentIndex = Math.max(0, folderField.model.findIndex(f => f.id === (entry ? entry.folder : editor.storeAdapter.scope)));
        const recipients = entry ? entry.recipients : "Llave A";
        keyA.checked = recipients.includes("Llave A");
        keyB.checked = recipients.includes("Llave B");
        keyC.checked = recipients.includes("Llave C");
        open();
        titleField.forceActiveFocus();
    }
    function ensureFocusVisible() {
        const flickable = editorScroll.contentItem as Flickable;
        const item = editor.focusedItem;
        if (!item || !flickable)
            return;
        const position = item.mapToItem(flickable.contentItem, 0, 0);
        const top = position.y;
        const bottom = top + item.height;
        if (top < flickable.contentY)
            flickable.contentY = top;
        else if (bottom > flickable.contentY + flickable.height)
            flickable.contentY = Math.max(0, bottom - flickable.height);
    }
    onFocusedItemChanged: {
        if (editor.opened)
            Qt.callLater(editor.ensureFocusVisible);
    }
    onOpened: {
        standardButton(Controls.Dialog.Save).text = entryId ? qsTranslate("Main", "Guardar cambios") : qsTranslate("Main", "Crear elemento");
        standardButton(Controls.Dialog.Cancel).text = qsTranslate("Main", "Cancelar");
        standardButton(Controls.Dialog.Save).enabled = Qt.binding(() => titleField.text.trim().length > 0 && (keyA.checked || keyB.checked || keyC.checked));
        Qt.callLater(ensureFocusVisible);
    }
    onAccepted: editor.saveRequested(entryId, folderField.currentValue, titleField.text, kindField.currentValue, formatField.currentText, currentValue(), notesField.text, [keyA.checked ? "Llave A" : "", keyB.checked ? "Llave B" : "", keyC.checked ? "Llave C" : ""].filter(Boolean).join(", "), currentDetails())
    contentItem: Controls.ScrollView {
        id: editorScroll
        objectName: "entryEditorScroll"
        implicitHeight: editorContent.implicitHeight
        contentWidth: availableWidth
        clip: true
        Controls.Pane {
            id: editorContent
            width: editorScroll.availableWidth
            leftPadding: Kirigami.Units.largeSpacing
            rightPadding: Kirigami.Units.largeSpacing
            topPadding: Kirigami.Units.smallSpacing
            bottomPadding: Kirigami.Units.largeSpacing
            background: null
            implicitHeight: entryForm.implicitHeight + topPadding + bottomPadding
            ColumnLayout {
                id: entryForm
                width: editorContent.availableWidth
                spacing: editor.sectionSpacing
                Controls.Label {
                    Layout.fillWidth: true
                    text: qsTranslate("Main", "Los campos se adaptan al tipo de elemento que estás editando.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
                Controls.GroupBox {
                    Layout.fillWidth: true
                    padding: editor.sectionPadding
                    title: qsTranslate("Main", "Datos del elemento")
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            text: qsTranslate("Main", "Nombre")
                        }
                        Controls.TextField {
                            id: titleField
                            Layout.fillWidth: true
                            Accessible.name: qsTranslate("Main", "Nombre del elemento")
                            placeholderText: qsTranslate("Main", "Ej. Correo personal")
                        }
                        RowLayout {
                            Layout.fillWidth: true
                            ColumnLayout {
                                Layout.fillWidth: true
                                Controls.Label {
                                    text: qsTranslate("Main", "Tipo")
                                }
                                Controls.ComboBox {
                                    id: kindField
                                    Layout.fillWidth: true
                                    Accessible.name: qsTranslate("Main", "Tipo de elemento")
                                    model: editor.storeAdapter.kindOptions
                                    textRole: "name"
                                    valueRole: "id"
                                }
                            }
                            ColumnLayout {
                                Layout.fillWidth: true
                                Controls.Label {
                                    text: qsTranslate("Main", "Carpeta")
                                }
                                Controls.ComboBox {
                                    id: folderField
                                    Layout.fillWidth: true
                                    Accessible.name: qsTranslate("Main", "Carpeta del elemento")
                                    model: [
                                        {
                                            id: "",
                                            name: qsTranslate("Main", "Sin carpeta")
                                        }
                                    ].concat(editor.storeAdapter.folderRows)
                                    textRole: "name"
                                    valueRole: "id"
                                }
                            }
                        }
                    }
                }
                Controls.GroupBox {
                    Layout.fillWidth: true
                    padding: editor.sectionPadding
                    title: kindField.currentValue === "note" ? qsTranslate("Main", "Contenido") : qsTranslate("Main", "Datos de %1").arg(editor.storeAdapter.kindName(kindField.currentValue))
                    contentItem: StackLayout {
                        id: typedFields
                        Layout.fillWidth: true
                        currentIndex: kindField.currentIndex
                        ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                text: qsTranslate("Main", "Número de tarjeta")
                            }
                            Controls.TextField {
                                id: cardNumberField
                                Layout.fillWidth: true
                                placeholderText: qsTranslate("Main", "0000 0000 0000 0000")
                            }
                            Controls.Label {
                                text: qsTranslate("Main", "Titular")
                            }
                            Controls.TextField {
                                id: cardHolderField
                                Layout.fillWidth: true
                                placeholderText: qsTranslate("Main", "NOMBRE APELLIDO")
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "Vencimiento")
                                    }
                                    Controls.TextField {
                                        id: cardExpiryField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "MM/AA")
                                    }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "CVV")
                                    }
                                    Controls.TextField {
                                        id: cardCvvField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "123")
                                    }
                                }
                            }
                        }
                        ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                text: qsTranslate("Main", "Nombre completo")
                            }
                            Controls.TextField {
                                id: identityNameField
                                Layout.fillWidth: true
                                placeholderText: qsTranslate("Main", "Nombre de la persona")
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "Documento")
                                    }
                                    Controls.TextField {
                                        id: identityDocumentField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "Número de documento")
                                    }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "Fecha de nacimiento")
                                    }
                                    Controls.TextField {
                                        id: identityBirthDateField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "AAAA-MM-DD")
                                    }
                                }
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "Correo")
                                    }
                                    Controls.TextField {
                                        id: identityEmailField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "correo@example.test")
                                    }
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    Controls.Label {
                                        text: qsTranslate("Main", "Teléfono")
                                    }
                                    Controls.TextField {
                                        id: identityPhoneField
                                        Layout.fillWidth: true
                                        placeholderText: qsTranslate("Main", "Número de teléfono")
                                    }
                                }
                            }
                        }
                        ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            Controls.Label {
                                text: qsTranslate("Main", "Sitio web")
                            }
                            Controls.TextField {
                                id: loginUrlField
                                Layout.fillWidth: true
                                placeholderText: qsTranslate("Main", "https://ejemplo.test")
                            }
                            Controls.Label {
                                text: qsTranslate("Main", "Usuario")
                            }
                            Controls.TextField {
                                id: loginUsernameField
                                Layout.fillWidth: true
                            }
                            Controls.Label {
                                text: qsTranslate("Main", "Contraseña")
                            }
                            RowLayout {
                                Layout.fillWidth: true
                                Controls.TextField {
                                    id: loginPasswordField
                                    Layout.fillWidth: true
                                    echoMode: editor.passwordVisible ? TextInput.Normal : TextInput.Password
                                }
                                Controls.ToolButton {
                                    checkable: true
                                    checked: editor.passwordVisible
                                    display: Controls.AbstractButton.IconOnly
                                    icon.name: editor.passwordVisible ? "view-hidden" : "view-visible"
                                    Accessible.name: editor.passwordVisible ? qsTranslate("Main", "Ocultar contraseña") : qsTranslate("Main", "Mostrar contraseña")
                                    Controls.ToolTip.visible: hovered
                                    Controls.ToolTip.text: Accessible.name
                                    onClicked: editor.passwordVisible = !editor.passwordVisible
                                }
                            }
                        }
                        ColumnLayout {
                            spacing: Kirigami.Units.smallSpacing
                            RowLayout {
                                Layout.fillWidth: true
                                Controls.Label {
                                    Layout.fillWidth: true
                                    text: qsTranslate("Main", "Formato")
                                }
                                Controls.ComboBox {
                                    id: formatField
                                    Accessible.name: qsTranslate("Main", "Formato del contenido")
                                    model: [qsTranslate("Main", "Texto"), qsTranslate("Main", "Binario")]
                                }
                            }
                            Controls.Label {
                                text: formatField.currentIndex === 1 ? qsTranslate("Main", "Bytes de demostración (hexadecimal)") : qsTranslate("Main", "Contenido de demostración")
                            }
                            Controls.ScrollView {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 160
                                Controls.TextArea {
                                    id: valueField
                                    Accessible.name: qsTranslate("Main", "Contenido de demostración")
                                    wrapMode: TextEdit.Wrap
                                    selectByMouse: true
                                }
                            }
                        }
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: editor.sectionSpacing
                    Controls.Label {
                        text: qsTranslate("Main", "Notas")
                    }
                    Controls.TextArea {
                        id: notesField
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.max(implicitHeight, Kirigami.Units.gridUnit * 4)
                        Accessible.name: qsTranslate("Main", "Notas del elemento")
                        placeholderText: qsTranslate("Main", "Añade contexto opcional para reconocer este elemento.")
                        wrapMode: TextEdit.Wrap
                        selectByMouse: true
                    }
                }
                Controls.GroupBox {
                    Layout.fillWidth: true
                    padding: editor.sectionPadding
                    title: qsTranslate("Main", "Acceso")
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Controls.Label {
                            Layout.fillWidth: true
                            text: qsTranslate("Main", "Selecciona las llaves que podrán abrir este elemento.")
                            color: Kirigami.Theme.disabledTextColor
                            wrapMode: Text.Wrap
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Controls.CheckBox {
                                id: keyA
                                text: qsTranslate("Main", "Llave A")
                            }
                            Controls.CheckBox {
                                id: keyB
                                text: qsTranslate("Main", "Llave B")
                            }
                            Controls.CheckBox {
                                id: keyC
                                text: qsTranslate("Main", "Llave C")
                            }
                        }
                    }
                }
            }
        }
    }
}
