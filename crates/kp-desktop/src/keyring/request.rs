//! The JSON contract QML sends, parsed into validated keyring values.

use std::path::PathBuf;

use kryptopass_keyring::{
    CertificationKind, DeleteScope, Expiry, ExpiryTarget, Fingerprint, InvalidInput, Keyserver,
    SubkeyUsage, UserId,
};
use serde::{Deserialize, Serialize};

const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Result returned to the desktop bridge. `public_key` is populated only for
/// `copyPublic`; secret-key bytes are written directly to a local file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationOutcome {
    pub message: String,
    pub public_key: Option<String>,
    pub selected_fingerprint: Option<String>,
}

/// One keyring operation whose every argument already passed validation.
pub(super) enum Request {
    Create {
        user_id: UserId,
        expiry: Expiry,
    },
    Import {
        path: PathBuf,
    },
    ExportPublic {
        fingerprint: Fingerprint,
        path: PathBuf,
    },
    CopyPublic {
        fingerprint: Fingerprint,
    },
    BackupSecret {
        fingerprint: Fingerprint,
        path: PathBuf,
    },
    ChangePassphrase {
        fingerprint: Fingerprint,
    },
    SetExpiry {
        fingerprint: Fingerprint,
        target: ExpiryTarget,
        expiry: Expiry,
    },
    AddUserId {
        fingerprint: Fingerprint,
        user_id: UserId,
    },
    SetPrimaryUserId {
        fingerprint: Fingerprint,
        user_id: UserId,
    },
    RevokeUserId {
        fingerprint: Fingerprint,
        user_id: UserId,
    },
    AddSubkey {
        fingerprint: Fingerprint,
        usage: SubkeyUsage,
        expiry: Expiry,
    },
    RevokeSubkey {
        fingerprint: Fingerprint,
        subkey: Fingerprint,
    },
    WriteRevocation {
        fingerprint: Fingerprint,
        path: PathBuf,
    },
    RevokeKey {
        fingerprint: Fingerprint,
    },
    Certify {
        target: Fingerprint,
        signer: Fingerprint,
        user_ids: Vec<UserId>,
        kind: CertificationKind,
    },
    Fetch {
        fingerprint: Fingerprint,
        keyserver: Option<Keyserver>,
    },
    Delete {
        fingerprint: Fingerprint,
        scope: DeleteScope,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
enum RawRequest {
    #[serde(rename = "create")]
    Create {
        userid: String,
        #[serde(rename = "expiresDays")]
        expires_days: u32,
    },
    #[serde(rename = "import")]
    Import { path: String },
    #[serde(rename = "exportPublic")]
    ExportPublic { fingerprint: String, path: String },
    #[serde(rename = "copyPublic")]
    CopyPublic { fingerprint: String },
    #[serde(rename = "backupPrivate")]
    BackupPrivate { fingerprint: String, path: String },
    #[serde(rename = "passphrase")]
    Passphrase { fingerprint: String },
    #[serde(rename = "expiry")]
    Expiry {
        fingerprint: String,
        subkeys: Vec<String>,
        #[serde(rename = "expiresDays")]
        expires_days: u32,
    },
    #[serde(rename = "addIdentity")]
    AddIdentity { fingerprint: String, userid: String },
    #[serde(rename = "primaryIdentity")]
    PrimaryIdentity { fingerprint: String, userid: String },
    #[serde(rename = "revokeIdentity")]
    RevokeIdentity { fingerprint: String, userid: String },
    #[serde(rename = "addSubkey")]
    AddSubkey {
        fingerprint: String,
        usage: RawSubkeyUsage,
        #[serde(rename = "expiresDays")]
        expires_days: u32,
    },
    #[serde(rename = "revokeSubkey")]
    RevokeSubkey { fingerprint: String, subkey: String },
    #[serde(rename = "prepareRevocation")]
    PrepareRevocation { fingerprint: String, path: String },
    #[serde(rename = "revokeKey")]
    RevokeKey { fingerprint: String },
    #[serde(rename = "certify")]
    Certify {
        fingerprint: String,
        #[serde(rename = "signerFingerprint")]
        signer_fingerprint: String,
        userids: Vec<String>,
        local: bool,
    },
    #[serde(rename = "refreshExternal")]
    RefreshExternal {
        fingerprint: String,
        #[serde(default)]
        keyserver: String,
    },
    #[serde(rename = "delete")]
    Delete {
        fingerprint: String,
        material: RawDeleteMaterial,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawSubkeyUsage {
    Encrypt,
    Sign,
    Authenticate,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawDeleteMaterial {
    Public,
    Private,
    Both,
}

pub(super) fn parse_request(request_json: &str) -> Result<Request, InvalidRequest> {
    if request_json.len() > MAX_REQUEST_BYTES {
        return Err(InvalidRequest::TooLarge);
    }
    let raw = serde_json::from_str::<RawRequest>(request_json)
        .map_err(|error| InvalidRequest::Malformed(error.to_string()))?;
    validate(raw).map_err(InvalidRequest::Value)
}

/// Why a request never reached the keyring.
pub(super) enum InvalidRequest {
    TooLarge,
    Malformed(String),
    Value(InvalidInput),
}

fn validate(raw: RawRequest) -> Result<Request, InvalidInput> {
    let fingerprint = |value: &str| Fingerprint::parse(value);
    let user_id = |value: &str| UserId::parse(value);
    Ok(match raw {
        RawRequest::Create {
            userid,
            expires_days,
        } => Request::Create {
            user_id: user_id(&userid)?,
            expiry: Expiry::from_days(expires_days),
        },
        RawRequest::Import { path } => Request::Import { path: path.into() },
        RawRequest::ExportPublic {
            fingerprint: value,
            path,
        } => Request::ExportPublic {
            fingerprint: fingerprint(&value)?,
            path: path.into(),
        },
        RawRequest::CopyPublic { fingerprint: value } => Request::CopyPublic {
            fingerprint: fingerprint(&value)?,
        },
        RawRequest::BackupPrivate {
            fingerprint: value,
            path,
        } => Request::BackupSecret {
            fingerprint: fingerprint(&value)?,
            path: path.into(),
        },
        RawRequest::Passphrase { fingerprint: value } => Request::ChangePassphrase {
            fingerprint: fingerprint(&value)?,
        },
        RawRequest::Expiry {
            fingerprint: value,
            subkeys,
            expires_days,
        } => Request::SetExpiry {
            fingerprint: fingerprint(&value)?,
            target: if subkeys.is_empty() {
                ExpiryTarget::Primary
            } else {
                ExpiryTarget::Subkeys(
                    subkeys
                        .iter()
                        .map(|subkey| fingerprint(subkey))
                        .collect::<Result<_, _>>()?,
                )
            },
            expiry: Expiry::from_days(expires_days),
        },
        RawRequest::AddIdentity {
            fingerprint: value,
            userid,
        } => Request::AddUserId {
            fingerprint: fingerprint(&value)?,
            user_id: user_id(&userid)?,
        },
        RawRequest::PrimaryIdentity {
            fingerprint: value,
            userid,
        } => Request::SetPrimaryUserId {
            fingerprint: fingerprint(&value)?,
            user_id: user_id(&userid)?,
        },
        RawRequest::RevokeIdentity {
            fingerprint: value,
            userid,
        } => Request::RevokeUserId {
            fingerprint: fingerprint(&value)?,
            user_id: user_id(&userid)?,
        },
        RawRequest::AddSubkey {
            fingerprint: value,
            usage,
            expires_days,
        } => Request::AddSubkey {
            fingerprint: fingerprint(&value)?,
            usage: match usage {
                RawSubkeyUsage::Encrypt => SubkeyUsage::Encrypt,
                RawSubkeyUsage::Sign => SubkeyUsage::Sign,
                RawSubkeyUsage::Authenticate => SubkeyUsage::Authenticate,
            },
            expiry: Expiry::from_days(expires_days),
        },
        RawRequest::RevokeSubkey {
            fingerprint: value,
            subkey,
        } => Request::RevokeSubkey {
            fingerprint: fingerprint(&value)?,
            subkey: fingerprint(&subkey)?,
        },
        RawRequest::PrepareRevocation {
            fingerprint: value,
            path,
        } => Request::WriteRevocation {
            fingerprint: fingerprint(&value)?,
            path: path.into(),
        },
        RawRequest::RevokeKey { fingerprint: value } => Request::RevokeKey {
            fingerprint: fingerprint(&value)?,
        },
        RawRequest::Certify {
            fingerprint: value,
            signer_fingerprint,
            userids,
            local,
        } => Request::Certify {
            target: fingerprint(&value)?,
            signer: fingerprint(&signer_fingerprint)?,
            user_ids: userids
                .iter()
                .map(|value| user_id(value))
                .collect::<Result<_, _>>()?,
            kind: if local {
                CertificationKind::Local
            } else {
                CertificationKind::Exportable
            },
        },
        RawRequest::RefreshExternal {
            fingerprint: value,
            keyserver,
        } => Request::Fetch {
            fingerprint: fingerprint(&value)?,
            keyserver: match keyserver.as_str() {
                "" => None,
                value => Some(Keyserver::parse(value)?),
            },
        },
        RawRequest::Delete {
            fingerprint: value,
            material,
        } => Request::Delete {
            fingerprint: fingerprint(&value)?,
            scope: match material {
                RawDeleteMaterial::Public => DeleteScope::PublicOnly,
                RawDeleteMaterial::Private => DeleteScope::SecretOnly,
                RawDeleteMaterial::Both => DeleteScope::Everything,
            },
        },
    })
}
