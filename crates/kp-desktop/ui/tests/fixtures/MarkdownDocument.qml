import QtQml

// Test double for the Rust bridge. It records lifecycle calls and deliberately
// does not render Markdown; renderer behavior is covered by Rust tests.
QtObject {
    property string html: ""
    property string error: ""
    property var spoilerActions: []
    property var revealedActions: []

    property int loadCount: 0
    property int clearCount: 0
    property int hideAllCount: 0
    property int activateLinkCount: 0
    property int setThemeCount: 0
    property string lastLoadedSource: ""
    property string lastActivatedLink: ""
    property var callLog: []

    signal linkRequested(string link)

    function load(source) {
        ++loadCount;
        lastLoadedSource = source;
        html = "";
        error = "";
        spoilerActions = [];
        revealedActions = [];
        callLog = callLog.concat(["load"]);
    }

    function clear() {
        ++clearCount;
        html = "";
        error = "";
        spoilerActions = [];
        revealedActions = [];
        callLog = callLog.concat(["clear"]);
    }

    function hideAll() {
        ++hideAllCount;
        revealedActions = [];
        callLog = callLog.concat(["hideAll"]);
    }

    function activateLink(link) {
        ++activateLinkCount;
        lastActivatedLink = link;
        callLog = callLog.concat(["activateLink"]);
    }

    function setTheme(highlightBackground, highlightForeground, link, codeBackground, border) {
        ++setThemeCount;
        callLog = callLog.concat(["setTheme"]);
    }
}
