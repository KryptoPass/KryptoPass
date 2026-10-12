//! Request and result types for local OpenPGP keyring operations.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub(super) const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub(super) const MAX_USER_ID_BYTES: usize = 255;
pub(super) const MAX_CERTIFIED_USER_IDS: usize = 64;

/// Result returned to the desktop bridge. `public_key` is populated only for
/// `copyPublic`; secret-key bytes are written directly to a local file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct OperationOutcome {
    pub message: String,
    pub public_key: Option<String>,
    pub selected_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
pub(super) enum OperationRequest {
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
        usage: SubkeyUsage,
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
        material: DeleteMaterial,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SubkeyUsage {
    Encrypt,
    Sign,
    Authenticate,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum DeleteMaterial {
    Public,
    Private,
    Both,
}

pub(super) fn parse_request(request_json: &str) -> Result<OperationRequest, String> {
    if request_json.len() > MAX_REQUEST_BYTES {
        return Err("La solicitud OpenPGP supera el tamaño permitido.".into());
    }
    serde_json::from_str::<OperationRequest>(request_json).map_err(|error| {
        crate::i18n::format(
            "La solicitud OpenPGP no es válida: %1",
            &[error.to_string()],
        )
    })
}

pub(super) fn normalize_fingerprint(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    if !matches!(bytes.len(), 40 | 64) || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err("Indica una huella OpenPGP completa de 40 o 64 dígitos hexadecimales.".into());
    }
    Ok(input.to_ascii_uppercase())
}

pub(super) fn validated_user_id(input: &str) -> Result<&str, String> {
    if input.trim().is_empty()
        || input.len() > MAX_USER_ID_BYTES
        || input.chars().any(char::is_control)
        || input.contains('\0')
    {
        return Err("La identidad debe tener entre 1 y 255 bytes y no contener controles.".into());
    }
    Ok(input)
}

/// Converts a requested expiry to seconds while ensuring GnuPG's 32-bit
/// absolute expiration timestamp can represent the resulting date.
pub(super) fn expiry_duration(days: u32) -> Result<Duration, String> {
    let seconds = u64::from(days) * 24 * 60 * 60;
    if days != 0 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                "No se pudo determinar la fecha actual para validar la caducidad.".to_owned()
            })?
            .as_secs();
        if now.saturating_add(seconds) > u64::from(u32::MAX) {
            return Err(
                "La fecha de caducidad supera el límite que puede representar GnuPG.".into(),
            );
        }
    }
    Ok(Duration::from_secs(seconds))
}

pub(super) fn local_path(input: &str) -> Result<std::path::PathBuf, String> {
    if input.is_empty() || input.len() > 4096 || input.contains('\0') {
        return Err("La ruta local está vacía o supera el tamaño permitido.".into());
    }
    Ok(std::path::PathBuf::from(input))
}
