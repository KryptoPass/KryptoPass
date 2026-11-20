pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import org.kde.kirigami as Kirigami
import org.kryptopass.desktop

TestCase {
    id: testCase
    name: "EntryIcon"
    width: 160
    height: 160
    visible: true
    when: windowShown

    readonly property real devicePixelRatio: Screen.devicePixelRatio
    readonly property url rasterSource: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAIAAAACAAQAAAADrRVxmAAAAGUlEQVR4nGNgGAWjYBSMglEwCkbBKKAvAAAIgAABVbZk+AAAAABJRU5ErkJggg=="
    readonly property url missingFaviconSource: "image://org.kryptopass.entryicons/favicon/missing.example.invalid"

    Component {
        id: iconComponent
        EntryIcon {}
    }

    function init() {
        failOnWarning(/Binding loop|QML|TypeError|ReferenceError|Unable to assign|Failed to get image|Entry icon unavailable|Error decoding/);
    }

    function imageFor(icon) {
        const image = icon.children.find(item => item instanceof Image);
        verify(image !== undefined, "Raster assets should retain a Qt Quick Image");
        return image;
    }

    function themeIconFor(icon) {
        const image = icon.children.find(item => item instanceof Kirigami.Icon);
        verify(image !== undefined, "Theme icons and optional favicons should use Kirigami.Icon");
        return image;
    }

    function createIcon(properties) {
        const icon = createTemporaryObject(iconComponent, testCase, properties);
        verify(icon !== null);
        verify(waitForPolish(icon.Window.window));
        return icon;
    }

    function verifyPainted(icon) {
        const image = themeIconFor(icon);
        tryVerify(() => image.visible && image.valid && image.paintedWidth > 0 && image.paintedHeight > 0);
        verify(waitForPolish(icon.Window.window));
        return image;
    }

    function test_missing_optional_favicon_draws_the_fallback_without_image_errors() {
        const icon = createIcon({
            entryIcon: {
                kind: "favicon"
            },
            imageSource: missingFaviconSource,
            fallbackIcon: "document-encrypted",
            iconSize: 32
        });
        const raster = imageFor(icon);
        compare(icon.favicon, true);
        compare(raster.source.toString(), "");
        compare(raster.status, Image.Null);
        compare(raster.visible, false);
        const fallback = verifyPainted(icon);
        compare(fallback.source.toString(), missingFaviconSource.toString());
        compare(fallback.fallback, icon.fallbackIcon);
        compare(fallback.placeholder, icon.fallbackIcon);

        const reference = createIcon({
            entryIcon: {
                kind: "theme",
                name: icon.fallbackIcon
            },
            iconSize: 32
        });
        verifyPainted(reference);
        const expected = grabImage(reference);
        tryVerify(() => grabImage(icon).equals(expected), 1000, "An unavailable favicon should visibly render the normal fallback icon");
    }

    function test_theme_icon_takes_precedence_over_an_unrelated_image_source() {
        const icon = createIcon({
            entryIcon: {
                kind: "theme",
                name: "folder"
            },
            imageSource: missingFaviconSource,
            fallbackIcon: "document-encrypted",
            iconSize: 24,
            selected: true
        });
        const theme = verifyPainted(icon);
        compare(icon.favicon, false);
        compare(theme.source.toString(), "folder");
        compare(theme.selected, true);
        compare(imageFor(icon).source.toString(), "");
        compare(imageFor(icon).visible, false);
        icon.entryIcon = {
            kind: "theme",
            name: "document-encrypted"
        };
        tryVerify(() => theme.source.toString() === "document-encrypted");
        verifyPainted(icon);
    }

    function test_favicon_and_raster_transitions_keep_current_sizes_and_visibility() {
        const icon = createIcon({
            entryIcon: {
                kind: "favicon"
            },
            imageSource: missingFaviconSource,
            iconSize: 16
        });
        const raster = imageFor(icon);
        const fallback = verifyPainted(icon);
        for (const size of [32, 24, 16]) {
            icon.iconSize = size;
            tryCompare(fallback, "width", size);
            tryCompare(fallback, "height", size);
            compare(icon.implicitWidth, size);
            compare(icon.implicitHeight, size);
            compare(raster.source.toString(), "");

            icon.imageSource = rasterSource;
            icon.entryIcon = {
                kind: "asset"
            };
            tryCompare(raster, "status", Image.Ready);
            tryCompare(raster, "visible", true);
            compare(fallback.visible, false);
            const expectedPixels = Math.ceil(size * devicePixelRatio);
            tryCompare(raster, "sourceSize", Qt.size(expectedPixels, expectedPixels));
            tryCompare(raster, "width", size);
            tryCompare(raster, "height", size);

            icon.entryIcon = {
                kind: "favicon"
            };
            icon.imageSource = missingFaviconSource;
            tryCompare(raster, "status", Image.Null);
            compare(raster.visible, false);
            verifyPainted(icon);
            compare(fallback.source.toString(), missingFaviconSource.toString());
        }
    }

    function test_raster_size_tracks_icon_size_without_binding_loops() {
        const icon = createTemporaryObject(iconComponent, testCase, {
            imageSource: rasterSource,
            iconSize: 16
        });
        verify(icon !== null);
        const image = icon.children.find(child => child instanceof Image);
        verify(image !== undefined);
        tryCompare(image, "status", Image.Ready);

        for (const size of [16, 32, 24, 16]) {
            icon.iconSize = size;
            const expectedPixels = Math.ceil(size * devicePixelRatio);
            tryCompare(image, "sourceSize", Qt.size(expectedPixels, expectedPixels));
            tryCompare(image, "status", Image.Ready);
            compare(icon.implicitWidth, size);
            compare(icon.implicitHeight, size);
            tryCompare(image, "width", size);
            tryCompare(image, "height", size);
        }
    }
}
