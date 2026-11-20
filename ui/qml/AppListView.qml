pragma ComponentBehavior: Bound
import QtQuick
import org.kde.kirigami as Kirigami

ListView {
    // Selection tracking can retarget as rows are laid out; keep each move short.
    highlightMoveDuration: Math.min(80, Kirigami.Units.shortDuration)
    highlightMoveVelocity: -1
}
