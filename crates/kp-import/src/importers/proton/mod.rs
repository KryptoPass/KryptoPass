//! Proton Pass provider. Container rules and wire types stay within this module.

mod container;
mod convert;
mod wire;

use crate::{
    Detection, EncryptionSupport, Entry, EntryFolder, EntryLocation, EntryType, EntryWarningKind,
    FeatureSupport, FolderId, ImportError, ImportFormat, ImportLimits, ImportedEntries, Importer,
    ImporterFeatures, ImporterInfo,
};
use std::collections::{HashMap, HashSet};
use std::io::Write;

pub static PROTON_PASS_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.proton",
    name: "Proton Pass",
    formats: &[
        ImportFormat {
            extension: "json",
            media_type: "application/json",
        },
        ImportFormat {
            extension: "zip",
            media_type: "application/zip",
        },
        ImportFormat {
            extension: "pgp",
            media_type: "application/pgp-encrypted",
        },
    ],
    features: ImporterFeatures {
        entry_types: &[
            EntryType::Login,
            EntryType::Note,
            EntryType::Alias,
            EntryType::Card,
            EntryType::Identity,
            EntryType::Custom,
            EntryType::Wifi,
            EntryType::SshKey,
        ],
        folders: true,
        folder_descriptions: true,
        favorites: true,
        trash: true,
        timestamps: true,
        additional_fields: true,
        passkeys: FeatureSupport::ReferenceOnly,
        autofill_rules: FeatureSupport::ReferenceOnly,
        attachments: FeatureSupport::Unsupported,
        encrypted_exports: EncryptionSupport::RequiresDecryptor,
    },
};

/// Proton's OpenPGP backend writes plaintext into the provider's bounded sink.
/// Errors must not contain parser diagnostics, passwords, or source values.
pub trait OpenPgpDecryptor: Send + Sync {
    fn decrypt(&self, ciphertext: &[u8], plaintext: &mut dyn Write) -> Result<(), ImportError>;
}

#[derive(Default)]
pub struct ProtonPassImporter {
    decryptor: Option<Box<dyn OpenPgpDecryptor>>,
}

impl ProtonPassImporter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_decryptor(decryptor: impl OpenPgpDecryptor + 'static) -> Self {
        Self {
            decryptor: Some(Box::new(decryptor)),
        }
    }
}

impl Importer for ProtonPassImporter {
    fn info(&self) -> &'static ImporterInfo {
        &PROTON_PASS_INFO
    }

    fn decryption_available(&self) -> bool {
        self.decryptor.is_some()
    }

    fn website(&self) -> Option<&'static str> {
        Some("pass.proton.me")
    }

    fn detect(&self, input: &[u8], limits: &ImportLimits) -> Detection {
        container::detect(input, limits)
    }

    fn import(&self, input: &[u8], limits: &ImportLimits) -> Result<ImportedEntries, ImportError> {
        ImportLimits::check_bytes(input.len(), limits.input_bytes)?;
        let json = container::decode(input, self.decryptor.as_deref(), limits)?;
        let export: wire::Export =
            serde_json::from_slice(&json).map_err(|_| ImportError::InvalidExport)?;
        if export.version.is_empty() {
            return Err(ImportError::Unsupported);
        }
        let collections = u32::try_from(export.vaults.len()).map_err(|_| ImportError::Limit)?;
        let mut folders = Vec::new();
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut source_entries = 0u32;
        let mut seen_items = HashSet::new();

        for (vault_index, (vault_id, vault)) in export.vaults.iter().enumerate() {
            if vault_id.is_empty() {
                return Err(ImportError::Hierarchy);
            }
            let collection = u32::try_from(vault_index).map_err(|_| ImportError::Limit)?;
            let new_folders = u32::try_from(vault.folders.len())
                .map_err(|_| ImportError::Limit)?
                .checked_add(1)
                .ok_or(ImportError::Limit)?;
            source_entries = source_entries
                .checked_add(u32::try_from(vault.items.len()).map_err(|_| ImportError::Limit)?)
                .ok_or(ImportError::Limit)?;
            let total = u32::try_from(folders.len())
                .map_err(|_| ImportError::Limit)?
                .checked_add(new_folders)
                .and_then(|count| count.checked_add(source_entries))
                .ok_or(ImportError::Limit)?;
            if total > limits.records {
                return Err(ImportError::Limit);
            }

            let root = FolderId(u32::try_from(folders.len()).map_err(|_| ImportError::Limit)?);
            folders.push(EntryFolder {
                id: root,
                parent: None,
                name: vault.name.as_str().into(),
                description: vault.description.clone(),
            });
            let mut folder_ids = HashMap::new();
            for (offset, folder) in vault.folders.iter().enumerate() {
                let id = FolderId(
                    u32::try_from(
                        folders
                            .len()
                            .checked_add(offset)
                            .ok_or(ImportError::Limit)?,
                    )
                    .map_err(|_| ImportError::Limit)?,
                );
                if folder.folder_id.is_empty()
                    || folder_ids.insert(folder.folder_id.as_str(), id).is_some()
                {
                    return Err(ImportError::Hierarchy);
                }
            }
            for folder in &vault.folders {
                let parent = match folder
                    .parent_folder_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                {
                    Some(id) => *folder_ids.get(id).ok_or(ImportError::Hierarchy)?,
                    None => root,
                };
                folders.push(EntryFolder {
                    id: folder_ids[folder.folder_id.as_str()],
                    parent: Some(parent),
                    name: folder.name.as_str().into(),
                    description: Default::default(),
                });
            }

            for (item_index, item) in vault.items.iter().enumerate() {
                if item.item_id.is_empty()
                    || !seen_items.insert((vault_id.as_str(), item.item_id.as_str()))
                    || (!item.share_id.is_empty() && &item.share_id != vault_id)
                {
                    return Err(ImportError::Hierarchy);
                }
                if !matches!(item.state, 1 | 2) {
                    return Err(ImportError::Unsupported);
                }
                let folder = match item.folder_id.as_deref().filter(|id| !id.is_empty()) {
                    Some(id) => *folder_ids.get(id).ok_or(ImportError::Hierarchy)?,
                    None => root,
                };
                let location = EntryLocation {
                    collection,
                    entry: Some(u32::try_from(item_index).map_err(|_| ImportError::Limit)?),
                };
                if !item.files.is_empty() {
                    convert::warn(
                        &mut warnings,
                        location,
                        EntryWarningKind::AttachmentsNotImported,
                    );
                }
                if let Some(converted) = convert::convert(item, location, &mut warnings)? {
                    entries.push(Entry {
                        title: item.data.metadata.name.as_str().into(),
                        folder: Some(folder),
                        content: converted.content,
                        favorite: item.pinned,
                        trashed: item.state == 2,
                        created_at: (item.create_time != 0).then_some(item.create_time),
                        modified_at: (item.modify_time != 0).then_some(item.modify_time),
                        additional_fields: converted.additional_fields,
                    });
                }
            }
        }
        ImportedEntries::new(
            folders,
            entries,
            warnings,
            collections,
            source_entries,
            limits,
        )
    }
}
