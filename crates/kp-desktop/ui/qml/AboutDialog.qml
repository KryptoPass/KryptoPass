pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as Controls
import org.kde.kirigami as Kirigami

Controls.Dialog {
    id: root

    required property var aboutData
    property url repositoryUrl: ""
    signal linkRequested(url target)

    title: qsTr("Acerca de %1").arg(aboutData.displayName)
    modal: true
    focus: true
    parent: Controls.Overlay.overlay
    anchors.centerIn: parent
    margins: Kirigami.Units.largeSpacing
    width: Math.min(Kirigami.Units.gridUnit * 36, parent.width - margins * 2)
    height: Math.min(implicitHeight, parent.height - margins * 2)
    padding: Kirigami.Units.largeSpacing * 2
    standardButtons: Controls.Dialog.Close
    onClosed: licenseDialog.close()

    component InformationCard: Controls.Pane {
        padding: Kirigami.Units.largeSpacing
        background: Rectangle {
            color: Kirigami.Theme.alternateBackgroundColor
            radius: Kirigami.Units.cornerRadius
        }
    }

    component LinkRow: Controls.ItemDelegate {
        id: linkRow
        Layout.fillWidth: true
        Accessible.role: Accessible.Link
        contentItem: RowLayout {
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Icon {
                source: linkRow.icon.name
                Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                Layout.preferredHeight: width
                Accessible.ignored: true
            }
            Controls.Label {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: linkRow.text
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            Kirigami.Icon {
                source: "open-link-symbolic"
                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                Layout.preferredHeight: width
                Accessible.ignored: true
            }
        }
    }

    contentItem: Controls.ScrollView {
        id: aboutScroll
        objectName: "aboutPage"
        implicitHeight: aboutContent.implicitHeight
        contentWidth: availableWidth
        clip: true

        ColumnLayout {
            id: aboutContent
            width: aboutScroll.availableWidth
            spacing: Kirigami.Units.largeSpacing * 2

            GridLayout {
                id: identityHeader
                Layout.fillWidth: true
                readonly property bool stacked: width < Kirigami.Units.gridUnit * 25
                columns: stacked ? 1 : 2
                columnSpacing: Kirigami.Units.largeSpacing * 2
                rowSpacing: Kirigami.Units.largeSpacing
                Kirigami.Icon {
                    source: root.aboutData.programLogo || root.aboutData.desktopFileName
                    Layout.preferredWidth: Kirigami.Units.iconSizes.huge
                    Layout.preferredHeight: width
                    Layout.alignment: identityHeader.stacked ? Qt.AlignHCenter : Qt.AlignTop
                    Accessible.ignored: true
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    spacing: Kirigami.Units.smallSpacing
                    Kirigami.Heading {
                        objectName: "applicationName"
                        Layout.fillWidth: true
                        level: 1
                        text: root.aboutData.displayName
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        horizontalAlignment: identityHeader.stacked ? Text.AlignHCenter : Text.AlignLeft
                    }
                    Controls.Label {
                        objectName: "applicationVersion"
                        Layout.fillWidth: true
                        text: qsTr("Versión %1").arg(root.aboutData.version)
                        color: Kirigami.Theme.disabledTextColor
                        wrapMode: Text.Wrap
                        horizontalAlignment: identityHeader.stacked ? Text.AlignHCenter : Text.AlignLeft
                    }
                    Controls.Label {
                        objectName: "applicationDescription"
                        Layout.fillWidth: true
                        Layout.topMargin: Kirigami.Units.smallSpacing
                        text: root.aboutData.shortDescription
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        horizontalAlignment: identityHeader.stacked ? Text.AlignHCenter : Text.AlignLeft
                    }
                }
            }

            InformationCard {
                Layout.fillWidth: true
                contentItem: ColumnLayout {
                    spacing: 0
                    LinkRow {
                        objectName: "homepageButton"
                        visible: root.aboutData.homepage.length > 0 && root.aboutData.homepage !== root.repositoryUrl.toString()
                        text: qsTr("Sitio web")
                        icon.name: "globe"
                        onClicked: root.linkRequested(root.aboutData.homepage)
                    }
                    LinkRow {
                        objectName: "sourceCodeButton"
                        visible: root.repositoryUrl.toString().length > 0
                        text: qsTr("Código fuente")
                        icon.name: "code-context"
                        onClicked: root.linkRequested(root.repositoryUrl)
                    }
                    LinkRow {
                        objectName: "bugReportButton"
                        visible: root.aboutData.bugAddress.length > 0
                        text: qsTr("Informar de un error")
                        icon.name: "tools-report-bug"
                        onClicked: root.linkRequested(root.aboutData.bugAddress)
                    }
                }
            }

            InformationCard {
                Layout.fillWidth: true
                contentItem: ColumnLayout {
                    spacing: Kirigami.Units.largeSpacing
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 3
                            text: qsTr("Creado por")
                        }
                        Repeater {
                            model: root.aboutData.authors
                            Controls.Label {
                                required property var modelData
                                Layout.fillWidth: true
                                text: modelData.name
                                textFormat: Text.PlainText
                                wrapMode: Text.Wrap
                            }
                        }
                    }
                    Kirigami.Separator {
                        Layout.fillWidth: true
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            level: 3
                            text: qsTr("Licencia")
                        }
                        Repeater {
                            model: root.aboutData.licenses
                            Controls.ItemDelegate {
                                id: licenseButton
                                required property var modelData
                                objectName: "licenseButton"
                                Layout.fillWidth: true
                                text: modelData.name
                                icon.name: "text-x-copying"
                                onClicked: {
                                    licenseDialog.title = modelData.name;
                                    licenseDialog.licenseText = modelData.text;
                                    licenseDialog.open();
                                }
                                contentItem: Controls.Label {
                                    text: licenseButton.text
                                    textFormat: Text.PlainText
                                    wrapMode: Text.Wrap
                                    color: Kirigami.Theme.linkColor
                                }
                            }
                        }
                    }
                }
            }
            Controls.Label {
                objectName: "applicationCopyright"
                Layout.fillWidth: true
                text: root.aboutData.copyrightStatement
                textFormat: Text.PlainText
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }
        }
    }

    Controls.Dialog {
        id: licenseDialog
        objectName: "licenseDialog"
        property string licenseText: ""
        modal: true
        focus: true
        parent: Controls.Overlay.overlay
        anchors.centerIn: parent
        margins: Kirigami.Units.largeSpacing
        width: Math.min(Kirigami.Units.gridUnit * 40, parent.width - margins * 2)
        height: Math.min(Kirigami.Units.gridUnit * 32, parent.height - margins * 2)
        standardButtons: Controls.Dialog.Close
        contentItem: Controls.ScrollView {
            contentWidth: availableWidth
            clip: true
            Controls.TextArea {
                objectName: "licenseText"
                text: licenseDialog.licenseText
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                background: null
                Accessible.name: licenseDialog.title
            }
        }
    }
}
