pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls as Controls
import QtTest
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "AboutDialog"
    width: Kirigami.Units.gridUnit * 45
    height: Kirigami.Units.gridUnit * 40
    visible: true
    when: windowShown

    Component {
        id: windowComponent

        Controls.ApplicationWindow {
            id: host

            property var fixtureData
            property alias aboutDialog: about

            visible: true

            AboutDialog {
                id: about
                aboutData: host.fixtureData
                repositoryUrl: "https://github.com/KryptoPass/KryptoPass"
            }
        }
    }

    Component {
        id: spyComponent
        SignalSpy {}
    }

    function init() {
        failOnWarning(/QML|TypeError|ReferenceError|Binding loop|Unable to assign|not defined|is not a function/);
    }

    function aboutFixture() {
        return {
            displayName: "KryptoPass",
            version: "0.1.0",
            shortDescription: "Organize credentials and secure notes with OpenPGP encryption",
            homepage: "https://kryptopass.example.invalid",
            bugAddress: "https://github.com/KryptoPass/KryptoPass/issues",
            componentName: "kryptopass-desktop",
            productName: "KryptoPass",
            desktopFileName: "org.kryptopass.desktop",
            copyrightStatement: "© 2026 KryptoPass contributors",
            otherText: "",
            authors: [
                {
                    name: "KryptoPass contributors",
                    task: "Development and maintenance",
                    emailAddress: "",
                    webAddress: "https://github.com/KryptoPass/KryptoPass/graphs/contributors",
                    ocsUsername: ""
                }
            ],
            credits: [],
            translators: [],
            licenses: [
                {
                    name: "GNU General Public License v3.0 or later",
                    text: "GNU GENERAL PUBLIC LICENSE\nVersion 3, 29 June 2007\n\nEveryone is permitted to copy and distribute verbatim copies of this license document, but changing it is not allowed.",
                    spdx: "GPL-3.0-or-later"
                }
            ],
            releases: []
        };
    }

    function openAbout(widthUnits = 45, fontScale = 1) {
        const window = createTemporaryObject(windowComponent, testCase, {
            width: Kirigami.Units.gridUnit * widthUnits,
            height: Kirigami.Units.gridUnit * 35,
            "font.pointSize": Kirigami.Theme.defaultFont.pointSize * fontScale,
            fixtureData: aboutFixture()
        });
        verify(window !== null);
        tryCompare(window, "visible", true);
        compare(window.aboutDialog.opened, false);
        window.aboutDialog.open();
        tryCompare(window.aboutDialog, "opened", true);
        verify(waitForPolish(window));
        return window;
    }

    function pageFor(window) {
        const page = window.aboutDialog.contentItem;
        verify(page !== null, "The about dialog should contain a scrollable about page");
        compare(page.objectName, "aboutPage");
        return page;
    }

    function named(root, name) {
        const object = findChild(root, name);
        verify(object !== null, "Missing about control: " + name);
        return object;
    }

    function allItems(root) {
        const result = [root];
        for (const child of root.children || [])
            result.push(...allItems(child));
        return result;
    }

    function findText(root, text, clickable = false) {
        const matches = allItems(root).filter(item => item.visible && item.text === text && (!clickable || typeof item.clicked === "function"));
        verify(matches.length > 0, "Missing visible about content: " + text);
        return matches[0];
    }

    function reveal(page, item) {
        const flickable = page.contentItem;
        const top = item.mapToItem(flickable.contentItem, 0, 0).y;
        flickable.contentY = Math.max(0, Math.min(top, flickable.contentHeight - flickable.height));
        const visibleTop = item.mapToItem(flickable, 0, 0).y;
        verify(visibleTop >= -1 && visibleTop < flickable.height, "About content should be reachable by scrolling: " + item.text);
    }

    function verifyFitsWindow(item, window, label) {
        verify(item !== null && item.visible, label + " should be visible");
        verify(item.width > 0 && item.height > 0, label + " should have positive dimensions");
        const topLeft = item.mapToItem(window.contentItem, 0, 0);
        const bottomRight = item.mapToItem(window.contentItem, item.width, item.height);
        verify(topLeft.x >= -1 && topLeft.y >= -1, label + " should start inside the window");
        verify(bottomRight.x <= window.width + 1 && bottomRight.y <= window.height + 1, label + " should fit inside the window");
    }

    function test_about_displays_application_metadata_and_authors() {
        const window = openAbout();
        const dialog = window.aboutDialog;
        const page = pageFor(window);
        const data = window.fixtureData;
        compare(dialog.modal, true);
        compare(dialog.title, qsTr("Acerca de %1").arg(data.displayName));
        const heading = named(page, "applicationName");
        compare(heading.text, data.displayName);
        verify(heading.width > 0 && heading.height > 0);
        reveal(page, heading);
        const version = named(page, "applicationVersion");
        verify(version.text.includes(data.version));
        reveal(page, version);
        const description = named(page, "applicationDescription");
        compare(description.text, data.shortDescription);
        reveal(page, description);
        const copyright = named(page, "applicationCopyright");
        compare(copyright.text, data.copyrightStatement);
        reveal(page, copyright);
        const author = findText(page, data.authors[0].name);
        reveal(page, author);
        verify(author.width > 0 && author.height > 0);
        dialog.close();
        tryCompare(dialog, "visible", false);
    }

    function test_license_action_opens_the_supplied_license_text_data() {
        return [
            {
                tag: "wide",
                widthUnits: 45,
                fontScale: 1
            },
            {
                tag: "narrow-large-font",
                widthUnits: 22,
                fontScale: 1.75
            }
        ];
    }

    function test_license_action_opens_the_supplied_license_text(data) {
        const window = openAbout(data.widthUnits, data.fontScale);
        const dialog = window.aboutDialog;
        const page = pageFor(window);
        const license = window.fixtureData.licenses[0];
        const licenseAction = named(page, "licenseButton");
        verify(licenseAction.text.includes(license.name));
        reveal(page, licenseAction);
        mouseClick(licenseAction);
        const licenseDialog = named(dialog, "licenseDialog");
        tryCompare(licenseDialog, "opened", true);
        const licenseText = findText(licenseDialog.contentItem, license.text);
        verify(licenseText.width > 0 && licenseText.height > 0);
        compare(licenseText.readOnly, true);
        verifyFitsWindow(licenseDialog.contentItem, window, "License content");
        verify(licenseDialog.contentItem.contentWidth <= licenseDialog.contentItem.availableWidth + 1, "License text should not require horizontal scrolling");
        keyClick(Qt.Key_Escape);
        tryCompare(licenseDialog, "visible", false);
        compare(dialog.opened, true);
        mouseClick(licenseAction);
        tryCompare(licenseDialog, "opened", true);
        const closeButton = licenseDialog.standardButton(Controls.Dialog.Close);
        verifyFitsWindow(closeButton, window, "License close button");
        mouseClick(closeButton);
        tryCompare(licenseDialog, "visible", false);
        dialog.close();
        tryCompare(dialog, "visible", false);
    }

    function test_link_buttons_request_their_metadata_urls() {
        const window = openAbout(22, 1.75);
        const dialog = window.aboutDialog;
        const page = pageFor(window);
        const spy = createTemporaryObject(spyComponent, testCase, {
            target: dialog,
            signalName: "linkRequested"
        });
        verify(spy !== null && spy.valid);
        const links = [
            {
                name: "homepageButton",
                url: window.fixtureData.homepage
            },
            {
                name: "sourceCodeButton",
                url: dialog.repositoryUrl.toString()
            },
            {
                name: "bugReportButton",
                url: window.fixtureData.bugAddress
            }
        ];
        for (const link of links) {
            const button = named(page, link.name);
            reveal(page, button);
            verify(button.visible && button.enabled);
            verifyFitsWindow(button, window, link.name);
            mouseClick(button);
            compare(spy.count, links.indexOf(link) + 1);
            compare(spy.signalArguments[spy.count - 1][0].toString(), link.url);
        }
        dialog.close();
        tryCompare(dialog, "visible", false);
    }

    function test_homepage_is_hidden_when_it_duplicates_the_repository() {
        const window = openAbout();
        const dialog = window.aboutDialog;
        dialog.repositoryUrl = window.fixtureData.homepage;
        compare(named(pageFor(window), "homepageButton").visible, false);
        compare(named(pageFor(window), "sourceCodeButton").visible, true);
        dialog.close();
        tryCompare(dialog, "visible", false);
    }

    function test_narrow_dialog_and_large_font_keep_content_inside_the_viewport_data() {
        return [
            {
                tag: "wide",
                widthUnits: 45,
                fontScale: 1
            },
            {
                tag: "narrow",
                widthUnits: 22,
                fontScale: 1
            },
            {
                tag: "narrow-large-font",
                widthUnits: 22,
                fontScale: 1.75
            }
        ];
    }

    function test_narrow_dialog_and_large_font_keep_content_inside_the_viewport(data) {
        const window = openAbout(data.widthUnits, data.fontScale);
        const dialog = window.aboutDialog;
        const page = pageFor(window);
        const flickable = page.contentItem;
        verifyFitsWindow(page, window, "About page");
        verifyFitsWindow(dialog.footer, window, "About footer");
        verify(flickable.contentWidth <= flickable.width + 1, "The about page should not require horizontal scrolling");

        for (const name of ["applicationName", "applicationDescription"]) {
            const label = named(page, name);
            reveal(page, label);
            verify(label.wrapMode === Text.WordWrap || label.wrapMode === Text.Wrap, "About metadata should enable text wrapping");
            verify(label.contentWidth <= label.width + 1, "About metadata should wrap to the available width: " + label.text);
            verify(label.contentHeight <= label.height + 1, "About metadata should retain its full height: " + label.text);
            const left = label.mapToItem(flickable, 0, 0).x;
            verify(left >= -1 && left + label.width <= flickable.width + 1, "About metadata should fit inside the horizontal viewport");
        }
        reveal(page, findText(page, window.fixtureData.authors[0].name));
        const closeButton = dialog.standardButton(Controls.Dialog.Close);
        verifyFitsWindow(closeButton, window, "Close button");
        verify(closeButton.enabled);
        mouseClick(closeButton);
        tryCompare(dialog, "visible", false);
    }

    function test_dialog_can_close_with_keyboard_and_reopen() {
        const window = openAbout(22, 1.75);
        const dialog = window.aboutDialog;
        const closeButton = dialog.standardButton(Controls.Dialog.Close);
        verify(closeButton !== null && closeButton.enabled);
        closeButton.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(closeButton, "activeFocus", true);
        keyClick(Qt.Key_Escape);
        tryCompare(dialog, "visible", false);
        dialog.open();
        tryCompare(dialog, "opened", true);
        compare(named(pageFor(window), "applicationName").text, window.fixtureData.displayName);
        mouseClick(closeButton);
        tryCompare(dialog, "visible", false);
    }
}
