pragma ComponentBehavior: Bound
import QtQuick
import QtTest
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

    Component {
        id: iconComponent
        EntryIcon {}
    }

    function init() {
        failOnWarning(/Binding loop/);
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
