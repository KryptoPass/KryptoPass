pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

Controls.Dialog {
    id: root
    objectName: "markdownDemo"
    title: qsTr("Markdown experimental")
    modal: true
    standardButtons: Controls.Dialog.Close
    width: Math.min(1000, parent.width - Kirigami.Units.largeSpacing * 4)
    height: Math.min(740, parent.height - Kirigami.Units.largeSpacing * 4)
    property string sample: "# Nota de demostración\n\nEl mantenimiento será el ||viernes a las **18:00**||.\nLa clave ficticia es ||DEMO-1234 y `código`||.\n\nTexto **en negrita**, *cursiva*, ~~tachado~~ y ==resaltado==.\n\n## Lista de tareas\n\n- [x] Preparar la nota\n- [ ] Revisar el contenido\n\n> Los spoilers son una presentación, no un cifrado independiente.\n\n| Función | Estado |\n| --- | --- |\n| Markdown | Experimental |\n| Spoilers | Interactivos |\n\n```rust\nlet ejemplo = \"solo datos ficticios\";\n```\n\n[Enlace de prueba](https://kde.org)\n\n![Imagen no cargada](https://example.test/image.png)"
    property string requestedLink: ""

    MarkdownDocument {
        id: previewDocument
        objectName: "markdownPreviewDocument"
    }
    MarkdownDocument {
        id: secondDocument
        objectName: "markdownSecondDocument"
    }

    function applySource() {
        preview.clearSelection();
        requestedLink = "";
        previewDocument.load(sourceEditor.text);
    }
    onOpened: {
        sourceEditor.text = sample;
        applySource();
    }
    onClosed: {
        previewDocument.clear();
        secondDocument.clear();
        sourceEditor.clear();
        requestedLink = "";
        secondView.close();
    }

    contentItem: ColumnLayout {
        spacing: Kirigami.Units.largeSpacing
        Controls.Label {
            Layout.fillWidth: true
            text: qsTr("CommonMark + tablas, tareas, tachado, ||spoilers|| y ==resaltado==. Datos ficticios; sin cifrado ni persistencia.")
            wrapMode: Text.Wrap
        }
        Controls.SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Controls.Pane {
                Controls.SplitView.preferredWidth: root.width * 0.4
                Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 12
                padding: 0
                ColumnLayout {
                    anchors.fill: parent
                    Kirigami.Heading {
                        level: 3
                        text: qsTr("Fuente Markdown")
                    }
                    Controls.ScrollView {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Controls.TextArea {
                            id: sourceEditor
                            objectName: "markdownSourceEditor"
                            textFormat: TextEdit.PlainText
                            wrapMode: TextEdit.Wrap
                            selectByMouse: true
                            Accessible.name: qsTr("Fuente Markdown de demostración")
                        }
                    }
                }
            }
            Controls.Pane {
                Controls.SplitView.fillWidth: true
                Controls.SplitView.minimumWidth: Kirigami.Units.gridUnit * 15
                padding: 0
                ColumnLayout {
                    anchors.fill: parent
                    Kirigami.Heading {
                        level: 3
                        text: qsTr("Vista previa")
                    }
                    Controls.ScrollView {
                        id: previewScroll
                        objectName: "markdownPreviewScroll"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        contentWidth: availableWidth
                        clip: true
                        MarkdownView {
                            id: preview
                            width: previewScroll.availableWidth
                            document: previewDocument
                            onLinkRequested: link => root.requestedLink = link
                        }
                    }
                }
            }
        }
        Flow {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Controls.Button {
                text: qsTr("Actualizar vista")
                icon.name: "view-refresh"
                onClicked: root.applySource()
            }
            Controls.Button {
                text: qsTr("Ocultar spoilers")
                icon.name: "view-hidden"
                onClicked: previewDocument.hideAll()
            }
            Controls.Button {
                text: qsTr("Copiar selección")
                icon.name: "edit-copy"
                enabled: preview.selectedText.length > 0
                onClicked: preview.copySelection()
            }
            Controls.Button {
                text: qsTr("Segunda vista…")
                icon.name: "window-new"
                onClicked: {
                    secondDocument.load(sourceEditor.text);
                    secondView.open();
                }
            }
            Controls.Button {
                text: qsTr("Limpiar documento")
                icon.name: "edit-clear"
                onClicked: {
                    previewDocument.clear();
                    secondDocument.clear();
                    preview.clearSelection();
                    sourceEditor.clear();
                    root.requestedLink = "";
                }
            }
        }
        Controls.Label {
            Layout.fillWidth: true
            text: root.requestedLink ? qsTr("Enlace solicitado (no se abre): %1").arg(root.requestedLink) : qsTr("Los enlaces no se abren automáticamente. HTML e imágenes omitidos.")
            elide: Text.ElideRight
        }
    }
    Controls.Dialog {
        id: secondView
        objectName: "markdownSecondView"
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        title: qsTr("Segunda vista · estado independiente")
        modal: true
        width: Math.min(640, parent.width - Kirigami.Units.largeSpacing * 4)
        height: Math.min(600, parent.height - Kirigami.Units.largeSpacing * 4)
        standardButtons: Controls.Dialog.Close
        onClosed: secondDocument.clear()
        contentItem: Controls.ScrollView {
            id: secondScroll
            contentWidth: availableWidth
            clip: true
            MarkdownView {
                width: secondScroll.availableWidth
                document: secondDocument
                onLinkRequested: link => root.requestedLink = link
            }
        }
    }
}
