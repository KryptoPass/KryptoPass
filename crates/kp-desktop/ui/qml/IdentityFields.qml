pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

SecretFields {
    id: root
    objectName: "identityDocumentsEditor"

    property string firstName: ""
    property string lastName: ""
    property var documents: []
    property var emails: []
    property var phones: []
    property string address: ""
    property string notes: ""
    readonly property int maximumDocuments: 100

    spacing: Kirigami.Units.largeSpacing

    onFirstNameChanged: edited()
    onLastNameChanged: edited()
    onDocumentsChanged: edited()
    onEmailsChanged: edited()
    onPhonesChanged: edited()
    onAddressChanged: edited()
    onNotesChanged: edited()

    function createDocumentId() {
        const bytes = [];
        for (let index = 0; index < 16; ++index)
            bytes.push(Math.floor(Math.random() * 256));
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        const hex = bytes.map(value => (value < 16 ? "0" : "") + value.toString(16));
        return hex.slice(0, 4).join("") + "-" + hex.slice(4, 6).join("") + "-"
            + hex.slice(6, 8).join("") + "-" + hex.slice(8, 10).join("") + "-"
            + hex.slice(10, 16).join("");
    }

    function addDocument() {
        if (documentModel.count >= maximumDocuments)
            return;
        documentModel.append({ documentId: createDocumentId(), documentType: "", number: "", country: "", expires: "" });
        syncDocuments();
        Qt.callLater(function() {
            const document = documentRepeater.itemAt(documentRepeater.count - 1);
            if (document)
                document.focusTypeField();
        });
    }

    function updateDocument(index, key, value) {
        if (index < 0 || index >= documentModel.count)
            return;
        if (["documentType", "number", "country", "expires"].indexOf(key) < 0)
            return;
        documentModel.setProperty(index, key, value);
        syncDocuments();
    }

    function removeDocument(index) {
        if (index < 0 || index >= documentModel.count)
            return;
        documentModel.remove(index);
        syncDocuments();
    }

    function moveDocument(index, delta) {
        const target = index + delta;
        if (index < 0 || index >= documentModel.count || target < 0 || target >= documentModel.count)
            return;
        documentModel.move(index, target, 1);
        syncDocuments();
    }

    function syncDocuments() {
        const next = [];
        for (let index = 0; index < documentModel.count; ++index) {
            const document = documentModel.get(index);
            next.push({
                id: document.documentId,
                documentType: document.documentType,
                number: document.number,
                country: document.country,
                expires: document.expires
            });
        }
        documents = next;
    }

    function loadFields(fields) {
        firstName = String(fields.firstName || "");
        lastName = String(fields.lastName || "");
        const normalizedDocuments = (Array.isArray(fields.documents) ? fields.documents : []).map(document => ({
            id: String(document.id || createDocumentId()),
            documentType: String(document.documentType || ""),
            number: String(document.number || ""),
            country: String(document.country || ""),
            expires: String(document.expires || "")
        }));
        documentModel.clear();
        for (const document of normalizedDocuments) {
            documentModel.append({
                documentId: document.id,
                documentType: document.documentType,
                number: document.number,
                country: document.country,
                expires: document.expires
            });
        }
        documents = normalizedDocuments;
        emails = (Array.isArray(fields.emails) ? fields.emails : []).map(value => String(value || ""));
        phones = (Array.isArray(fields.phones) ? fields.phones : []).map(value => String(value || ""));
        address = String(fields.address || "");
        notes = String(fields.notes || "");
        notesEditor.resetSource(notes);
    }

    function currentFields() {
        return {
            firstName: firstName,
            lastName: lastName,
            documents: documents.map(document => ({
                id: document.id,
                documentType: document.documentType,
                number: document.number,
                country: document.country,
                expires: document.expires
            })),
            emails: emails.slice(),
            phones: phones.slice(),
            address: address,
            notes: notes
        };
    }

    function clearFields() { loadFields({}); }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Kirigami.Units.largeSpacing

        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("Nombre")
            font.weight: Font.DemiBold
        }
        GridLayout {
            Layout.fillWidth: true
            columns: width >= Kirigami.Units.gridUnit * 34 ? 2 : 1
            columnSpacing: Kirigami.Units.largeSpacing
            rowSpacing: Kirigami.Units.largeSpacing
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Nombre")
                value: root.firstName
                onValueEdited: function(value) { root.firstName = value; }
            }
            SecretInput {
                Layout.fillWidth: true
                label: qsTr("Apellidos")
                value: root.lastName
                onValueEdited: function(value) { root.lastName = value; }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Documentos")
                font.weight: Font.DemiBold
                wrapMode: Text.Wrap
            }
            Controls.Label {
                Layout.fillWidth: true
                text: qsTr("Guarda solo los datos que necesites tener a mano.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
            ListModel {
                id: documentModel
            }
            Repeater {
                id: documentRepeater
                model: documentModel
                delegate: Controls.Frame {
                    id: documentFrame
                    required property int index
                    required property string documentId
                    required property string documentType
                    required property string number
                    required property string country
                    required property string expires
                    Layout.fillWidth: true
                    padding: Kirigami.Units.largeSpacing

                    function focusTypeField() { documentTypeField.forceActiveFocus(); }
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            Layout.fillWidth: true
                            Controls.Label {
                                Layout.fillWidth: true
                                text: qsTr("Documento %1").arg(documentFrame.index + 1)
                                font.weight: Font.Medium
                            }
                            Controls.ToolButton {
                                text: qsTr("Mover documento hacia arriba")
                                icon.name: "arrow-up"
                                display: Controls.AbstractButton.IconOnly
                                enabled: documentFrame.index > 0
                                Accessible.name: text
                                onClicked: root.moveDocument(documentFrame.index, -1)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                            Controls.ToolButton {
                                text: qsTr("Mover documento hacia abajo")
                                icon.name: "arrow-down"
                                display: Controls.AbstractButton.IconOnly
                                enabled: documentFrame.index < documentModel.count - 1
                                Accessible.name: text
                                onClicked: root.moveDocument(documentFrame.index, 1)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                            Controls.ToolButton {
                                text: qsTr("Quitar documento %1").arg(documentFrame.index + 1)
                                icon.name: "edit-delete"
                                display: Controls.AbstractButton.IconOnly
                                Accessible.name: text
                                onClicked: root.removeDocument(documentFrame.index)
                                Controls.ToolTip.visible: hovered || visualFocus
                                Controls.ToolTip.text: text
                            }
                        }
                        GridLayout {
                            Layout.fillWidth: true
                            columns: width >= Kirigami.Units.gridUnit * 34 ? 2 : 1
                            columnSpacing: Kirigami.Units.largeSpacing
                            rowSpacing: Kirigami.Units.largeSpacing
                            SecretInput {
                                id: documentTypeField
                                Layout.fillWidth: true
                                label: qsTr("Tipo de documento")
                                value: documentFrame.documentType
                                onValueEdited: function(value) { root.updateDocument(documentFrame.index, "documentType", value); }
                            }
                            SecretInput {
                                Layout.fillWidth: true
                                label: qsTr("Número")
                                value: documentFrame.number
                                concealed: true
                                onValueEdited: function(value) { root.updateDocument(documentFrame.index, "number", value); }
                            }
                            SecretInput {
                                Layout.fillWidth: true
                                label: qsTr("País o región")
                                value: documentFrame.country
                                onValueEdited: function(value) { root.updateDocument(documentFrame.index, "country", value); }
                            }
                            SecretInput {
                                Layout.fillWidth: true
                                label: qsTr("Fecha de vencimiento")
                                value: documentFrame.expires
                                placeholderText: qsTr("Fecha opcional")
                                onValueEdited: function(value) { root.updateDocument(documentFrame.index, "expires", value); }
                            }
                        }
                    }
                }
            }
            Controls.Button {
                text: qsTr("Agregar documento")
                icon.name: "list-add"
                enabled: documentModel.count < root.maximumDocuments
                onClicked: root.addDocument()
            }
        }

        StringListEditor {
            objectName: "identityEmailsEditor"
            Layout.fillWidth: true
            label: qsTr("Correos")
            inputLabel: qsTr("Correo")
            addLabel: qsTr("Agregar correo")
            maximumValues: 100
            values: root.emails
            onValuesEdited: function(values) { root.emails = values; }
        }
        StringListEditor {
            objectName: "identityPhonesEditor"
            Layout.fillWidth: true
            label: qsTr("Teléfonos")
            inputLabel: qsTr("Teléfono")
            addLabel: qsTr("Agregar teléfono")
            maximumValues: 100
            values: root.phones
            onValuesEdited: function(values) { root.phones = values; }
        }
        SecretInput {
            Layout.fillWidth: true
            label: qsTr("Dirección")
            value: root.address
            multiline: true
            onValueEdited: function(value) { root.address = value; }
        }
        NotesEditor {
            id: notesEditor
            Layout.fillWidth: true
            onSourceTextEdited: value => root.notes = value
        }
    }
}
