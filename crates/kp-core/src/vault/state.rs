//! The state of a version as it travels: title, folder, flags, template and
//! icon, as JSON encrypted to the version's recipients.
//!
//! Recipients are not part of it: they travel in the signed header, so a
//! device can route and tell whether it can read a version without opening
//! it. The format is the contract's (`docs/sync/contrato.md`, §2.1).

use kp_template::TemplateId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::VaultError;
use super::record::ItemState;
use crate::SensitiveBytes;
use crate::domain::entry_icon::EntryIcon;
use crate::domain::{Folder, FolderAppearance, FolderId, KeyFingerprint, RecipientSet};

const SCHEMA_VERSION: u32 = 1;
/// Largest encoded state; a title is bounded far below it.
const MAX_STATE_BYTES: usize = 64 * 1024;

/// What a state holds besides recipients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedState {
    pub folder: Option<FolderId>,
    pub title: String,
    pub template: TemplateId,
    pub icon: Option<EntryIcon>,
    pub favorite: bool,
    pub trashed: bool,
}

impl OpenedState {
    /// The full state, with the recipients the version names.
    pub fn with_recipients(self, recipients: RecipientSet) -> ItemState {
        ItemState {
            folder: self.folder,
            title: self.title,
            template: self.template,
            recipients,
            icon: self.icon,
            favorite: self.favorite,
            trashed: self.trashed,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    title: String,
    folder: Option<Uuid>,
    favorite: bool,
    trashed: bool,
    template: TemplateId,
    icon: Option<IconDoc>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
enum IconDoc {
    Theme(String),
    Image(Uuid),
    Favicon(String),
}

/// The plaintext of a state, ready to encrypt.
pub fn seal_state(state: &ItemState) -> Result<SensitiveBytes, VaultError> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        title: state.title.clone(),
        folder: state.folder.map(|folder| folder.0),
        favorite: state.favorite,
        trashed: state.trashed,
        template: state.template.clone(),
        icon: state.icon.as_ref().map(|icon| match icon {
            EntryIcon::Theme { name } => IconDoc::Theme(name.clone()),
            EntryIcon::Image { asset_id } => IconDoc::Image(*asset_id),
            EntryIcon::Favicon { host } => IconDoc::Favicon(host.clone()),
        }),
    };
    let mut bytes = SensitiveBytes::new();
    serde_json::to_writer(bytes.bounded_writer(MAX_STATE_BYTES), &envelope)
        .map_err(|_| VaultError::ContentTooLarge)?;
    Ok(bytes)
}

/// Reads a decrypted state. Anything malformed is corrupt content: the
/// signature already vouched for who wrote it.
pub fn open_state(plaintext: &[u8]) -> Result<OpenedState, VaultError> {
    if plaintext.len() > MAX_STATE_BYTES {
        return Err(VaultError::CorruptContent);
    }
    let envelope: Envelope =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptContent)?;
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(VaultError::TemplateTooNew);
    }
    let icon = envelope.icon.map(|icon| match icon {
        IconDoc::Theme(name) => EntryIcon::Theme { name },
        IconDoc::Image(asset_id) => EntryIcon::Image { asset_id },
        IconDoc::Favicon(host) => EntryIcon::Favicon { host },
    });
    if icon.as_ref().is_some_and(|icon| icon.validate().is_err()) {
        return Err(VaultError::CorruptContent);
    }
    Ok(OpenedState {
        folder: envelope.folder.map(FolderId),
        title: envelope.title,
        template: envelope.template,
        icon,
        favorite: envelope.favorite,
        trashed: envelope.trashed,
    })
}

impl<S: super::Store, C: super::Cipher> super::Vault<S, C> {
    /// A version's state encrypted to its recipients. Encrypting only uses
    /// public keys, so moving or renaming never asks for a passphrase.
    pub(super) fn seal(&self, state: &ItemState) -> Result<Vec<u8>, VaultError> {
        let plaintext = seal_state(state)?;
        Ok(self
            .cipher
            .encrypt(plaintext.as_slice(), &state.recipients)?)
    }
}

/// What a folder version holds. Sibling order is not part of it: each device
/// keeps its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedFolder {
    pub name: String,
    /// The parent folder as its writer saw it; it may not be known here.
    pub parent: Option<FolderId>,
    pub appearance: FolderAppearance,
    pub default_recipients: Option<RecipientSet>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FolderEnvelope {
    schema_version: u32,
    name: String,
    parent: Option<Uuid>,
    icon: String,
    color: String,
    default_recipients: Vec<String>,
}

/// The plaintext of a folder's state, ready to encrypt.
pub fn seal_folder(folder: &Folder) -> Result<SensitiveBytes, VaultError> {
    let envelope = FolderEnvelope {
        schema_version: SCHEMA_VERSION,
        name: folder.name.clone(),
        parent: folder.parent_id.map(|parent| parent.0),
        icon: folder.appearance.icon().to_owned(),
        color: folder.appearance.color().to_owned(),
        default_recipients: folder
            .default_recipients
            .as_ref()
            .map(|set| {
                set.as_slice()
                    .iter()
                    .map(|key| key.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut bytes = SensitiveBytes::new();
    serde_json::to_writer(bytes.bounded_writer(MAX_STATE_BYTES), &envelope)
        .map_err(|_| VaultError::ContentTooLarge)?;
    Ok(bytes)
}

/// Reads a decrypted folder state.
pub fn open_folder(plaintext: &[u8]) -> Result<OpenedFolder, VaultError> {
    if plaintext.len() > MAX_STATE_BYTES {
        return Err(VaultError::CorruptContent);
    }
    let envelope: FolderEnvelope =
        serde_json::from_slice(plaintext).map_err(|_| VaultError::CorruptContent)?;
    if envelope.schema_version != SCHEMA_VERSION {
        return Err(VaultError::TemplateTooNew);
    }
    let appearance = FolderAppearance::new(envelope.icon, envelope.color)
        .map_err(|_| VaultError::CorruptContent)?;
    let default_recipients = if envelope.default_recipients.is_empty() {
        None
    } else {
        let keys = envelope
            .default_recipients
            .iter()
            .map(|key| KeyFingerprint::parse(key))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| VaultError::CorruptContent)?;
        Some(RecipientSet::new(keys).map_err(|_| VaultError::CorruptContent)?)
    };
    Ok(OpenedFolder {
        name: envelope.name,
        parent: envelope.parent.map(FolderId),
        appearance,
        default_recipients,
    })
}

impl<S: super::Store, C: super::Cipher> super::Vault<S, C> {
    /// A folder's state encrypted to `recipients`.
    pub(super) fn seal_folder(
        &self,
        folder: &Folder,
        recipients: &RecipientSet,
    ) -> Result<Vec<u8>, VaultError> {
        let plaintext = seal_folder(folder)?;
        Ok(self.cipher.encrypt(plaintext.as_slice(), recipients)?)
    }
}
