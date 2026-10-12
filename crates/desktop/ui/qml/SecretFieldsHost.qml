pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop

Item {
    id: root

    property string kind: "login"
    readonly property SecretFields form: formLoader.item as SecretFields
    readonly property bool ready: !!form && formLoader.status === Loader.Ready
    property var cachedFields: ({})
    property var pendingFields: null
    property bool updating: false

    signal edited
    signal generatePasswordRequested

    implicitHeight: formLoader.implicitHeight

    function currentFields() {
        return form ? form.currentFields() : {};
    }

    function activate(nextKind, fields) {
        pendingFields = fields;
        kind = nextKind;
        if (form && pendingFields) {
            form.loadFields(pendingFields);
            pendingFields = null;
        }
    }

    function loadContent(content) {
        clearFields();
        updating = true;
        activate(content.kind, content.fields);
        updating = false;
    }

    function selectType(nextKind) {
        if (nextKind === kind)
            return;
        updating = true;
        if (form)
            cachedFields[kind] = form.currentFields();
        activate(nextKind, cachedFields[nextKind] || {});
        updating = false;
        edited();
    }

    function clearFields() {
        updating = true;
        if (form)
            form.clearFields();
        cachedFields = ({});
        pendingFields = null;
        updating = false;
    }

    function applyGeneratedPassword(value) {
        if (form)
            form.applyGeneratedPassword(value);
    }

    Loader {
        id: formLoader
        width: root.width
        sourceComponent: EntryTypes.find(root.kind)?.editor || null
        onLoaded: {
            const wasUpdating = root.updating;
            root.updating = true;
            root.form.loadFields(root.pendingFields || {});
            root.pendingFields = null;
            root.updating = wasUpdating;
        }
    }

    Connections {
        target: root.form
        function onEdited() {
            if (!root.updating)
                root.edited();
        }
        function onGeneratePasswordRequested() {
            root.generatePasswordRequested();
        }
    }
}
