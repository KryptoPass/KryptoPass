pragma Singleton
pragma ComponentBehavior: Bound
import QtQuick
import org.kryptopass.desktop

QtObject {
    id: types

    readonly property var options: [
        {
            id: "login",
            name: qsTranslate("SecretStore", "Inicio de sesión"),
            icon: "user-identity",
            editor: loginEditor,
            detail: loginDetail
        },
        {
            id: "note",
            name: qsTranslate("SecretStore", "Nota"),
            icon: "document-edit",
            editor: noteEditor,
            detail: noteDetail
        }
    ]

    function find(kind) {
        return options.find(option => option.id === kind) || null;
    }

    readonly property Component loginEditor: Component {
        LoginFields {}
    }
    readonly property Component noteEditor: Component {
        NoteFields {}
    }
    readonly property Component loginDetail: Component {
        LoginDetail {}
    }
    readonly property Component noteDetail: Component {
        NoteDetail {}
    }
}
