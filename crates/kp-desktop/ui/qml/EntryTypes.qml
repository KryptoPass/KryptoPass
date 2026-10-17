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
        },
        {
            id: "custom",
            name: qsTranslate("SecretStore", "Secreto personalizado"),
            icon: "view-list-details",
            editor: customEditor,
            detail: customDetail
        },
        {
            id: "card",
            name: qsTranslate("SecretStore", "Tarjeta de pago"),
            icon: "wallet-open",
            editor: cardEditor,
            detail: cardDetail
        },
        {
            id: "totp",
            name: qsTranslate("SecretStore", "Autenticador TOTP"),
            icon: "view-barcode-qr",
            editor: totpEditor,
            detail: totpDetail
        },
        {
            id: "identity",
            name: qsTranslate("SecretStore", "Identidad"),
            icon: "user-identity",
            editor: identityEditor,
            detail: identityDetail
        },
        {
            id: "wifi",
            name: qsTranslate("SecretStore", "Red Wi-Fi"),
            icon: "network-wireless",
            editor: wifiEditor,
            detail: wifiDetail
        },
        {
            id: "license",
            name: qsTranslate("SecretStore", "Licencia de software"),
            icon: "license",
            editor: licenseEditor,
            detail: licenseDetail
        },
        {
            id: "ssh-key",
            name: qsTranslate("SecretStore", "Clave SSH"),
            icon: "dialog-password",
            editor: sshKeyEditor,
            detail: sshKeyDetail
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
    readonly property Component customEditor: Component {
        CustomFields {}
    }
    readonly property Component cardEditor: Component {
        CardFields {}
    }
    readonly property Component totpEditor: Component {
        TotpFields {}
    }
    readonly property Component identityEditor: Component {
        IdentityFields {}
    }
    readonly property Component wifiEditor: Component {
        WifiFields {}
    }
    readonly property Component licenseEditor: Component {
        LicenseFields {}
    }
    readonly property Component sshKeyEditor: Component {
        SshKeyFields {}
    }
    readonly property Component loginDetail: Component {
        LoginDetail {}
    }
    readonly property Component noteDetail: Component {
        NoteDetail {}
    }
    readonly property Component customDetail: Component {
        StructuredSecretDetail { objectName: "customDetail" }
    }
    readonly property Component cardDetail: Component {
        StructuredSecretDetail { objectName: "cardDetail" }
    }
    readonly property Component totpDetail: Component {
        StructuredSecretDetail {
            objectName: "totpDetail"
            showTotp: true
            showQr: true
        }
    }
    readonly property Component identityDetail: Component {
        StructuredSecretDetail { objectName: "identityDetail" }
    }
    readonly property Component wifiDetail: Component {
        StructuredSecretDetail {
            objectName: "wifiDetail"
            showQr: true
        }
    }
    readonly property Component licenseDetail: Component {
        StructuredSecretDetail { objectName: "licenseDetail" }
    }
    readonly property Component sshKeyDetail: Component {
        StructuredSecretDetail { objectName: "sshKeyDetail" }
    }
}
