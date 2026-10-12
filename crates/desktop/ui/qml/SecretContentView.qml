pragma ComponentBehavior: Bound
import QtQuick

Item {
    property var fields: null
    property string entryId: ""
    property bool revealed: false
    property var versionDetail: null

    signal revealToggled
    signal copyUsernameRequested
    signal copyPasswordRequested
    signal copyTotpRequested(bool next)
    signal versionOpenRequested(int index)
    signal versionCloseRequested
    signal versionCopyRequested(string field)
    signal activityExportRequested
}
