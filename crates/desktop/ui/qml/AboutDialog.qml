pragma ComponentBehavior: Bound
import QtQml
import org.kryptopass.desktop as Backend

QtObject {
    id: aboutDialog

    required property Backend.ShellController controller
    readonly property bool opened: false

    function open() {
        controller.showAboutDialog();
    }

    function close() {}
}
