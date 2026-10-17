//! Destination policy for the provider-independent entry model.

use kryptopass_core::{self as core, ImportBatch, ImportFolder, ImportItem, SecretContent};
use kryptopass_import::{
    Entry, EntryContent, EntryField, EntryStatistics, EntryWarning, ImportError, ImportedEntries,
    OneTimePassword,
};
use kryptopass_secure_memory::SensitiveText;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Default)]
pub struct AdaptationStatistics {
    pub aliases_as_notes: u32,
    pub additional_entries: u32,
    pub folder_descriptions: u32,
    pub timestamps_as_fields: u32,
}

pub struct PreparedImport {
    pub batch: ImportBatch,
    pub source: EntryStatistics,
    pub warnings: Vec<EntryWarning>,
    pub adaptations: AdaptationStatistics,
}

pub(super) fn prepare(source: ImportedEntries) -> Result<PreparedImport, ImportError> {
    let statistics = source.statistics().clone();
    let (folders, entries, warnings) = source.into_parts();
    let indices: HashMap<_, _> = folders
        .iter()
        .enumerate()
        .map(|(index, folder)| (folder.id, index))
        .collect();
    let mut batch = ImportBatch::default();
    let mut adaptations = AdaptationStatistics::default();
    // Core requires a folder even for providers whose entries have no folder.
    let unfiled = if entries.iter().any(|entry| entry.folder.is_none()) {
        Some(folders.len())
    } else {
        None
    };
    for folder in folders {
        let index = *indices.get(&folder.id).ok_or(ImportError::Hierarchy)?;
        let parent = folder
            .parent
            .map(|id| indices.get(&id).copied().ok_or(ImportError::Hierarchy))
            .transpose()?;
        let name = folder.name.to_string();
        if !folder.description.is_empty() {
            batch.items.push(ImportItem {
                title: format!("Descripción: {name}"),
                content: SecretContent::Note(core::NoteContent {
                    value: folder.description,
                    notes: SensitiveText::new(),
                }),
                folder: index,
                favorite: false,
                trashed: false,
            });
            adaptations.folder_descriptions += 1;
        }
        batch.folders.push(ImportFolder { name, parent });
    }
    if unfiled.is_some() {
        batch.folders.push(ImportFolder {
            name: crate::i18n::text("Sin carpeta"),
            parent: None,
        });
    }
    for entry in entries {
        let folder = match entry.folder {
            Some(id) => *indices.get(&id).ok_or(ImportError::Hierarchy)?,
            None => unfiled.ok_or(ImportError::Hierarchy)?,
        };
        append_entry(&mut batch, entry, folder, &mut adaptations)?;
    }
    batch.validate().map_err(|error| match error {
        core::ImportValidationError::Limit => ImportError::Limit,
        core::ImportValidationError::Hierarchy => ImportError::Hierarchy,
        core::ImportValidationError::Content => ImportError::Content,
    })?;
    Ok(PreparedImport {
        batch,
        source: statistics,
        warnings,
        adaptations,
    })
}

fn append_entry(
    batch: &mut ImportBatch,
    entry: Entry,
    folder: usize,
    adaptations: &mut AdaptationStatistics,
) -> Result<(), ImportError> {
    let Entry {
        title,
        content,
        favorite,
        trashed,
        created_at,
        modified_at,
        mut additional_fields,
        ..
    } = entry;
    if created_at.is_some() || modified_at.is_some() {
        adaptations.timestamps_as_fields += 1;
        for (label, value) in [
            ("Fecha de creación (Unix)", created_at),
            ("Fecha de modificación (Unix)", modified_at),
        ] {
            if let Some(value) = value {
                additional_fields.push(EntryField {
                    label: label.into(),
                    value: value.to_string().into(),
                    concealed: false,
                    multiline: false,
                });
            }
        }
    }
    let content = match content {
        EntryContent::Login(login) => {
            let username = if login.username.is_empty() {
                login.email.clone()
            } else {
                login.username
            };
            if !login.email.is_empty() && login.email != username {
                additional_fields.push(EntryField {
                    label: "Correo electrónico".into(),
                    value: login.email,
                    concealed: false,
                    multiline: false,
                });
            }
            SecretContent::Login(core::LoginContent {
                username,
                password: login.password,
                url: login.urls.first().cloned().unwrap_or_default(),
                urls: login.urls,
                notes: login.notes,
                totp: login.otp.map(otp).transpose()?.flatten(),
                history: Box::default(),
            })
        }
        EntryContent::Note(note) => SecretContent::Note(core::NoteContent {
            value: note.value,
            notes: SensitiveText::new(),
        }),
        EntryContent::Alias(alias) => {
            adaptations.aliases_as_notes += 1;
            let mut value = alias.email;
            if !alias.notes.is_empty() {
                value.push_str("\n\n");
                value.push_str(&alias.notes);
            }
            SecretContent::Note(core::NoteContent {
                value,
                notes: SensitiveText::new(),
            })
        }
        EntryContent::Card(card) => SecretContent::Card(core::CardContent {
            holder: card.holder,
            number: card.number,
            expiry_month: card.expiry_month,
            expiry_year: card.expiry_year,
            cvv: card.cvv,
            pin: card.pin,
            notes: card.notes,
        }),
        EntryContent::Identity(identity) => SecretContent::Identity(core::IdentityContent {
            first_name: identity.first_name,
            last_name: identity.last_name,
            documents: identity
                .documents
                .into_iter()
                .map(|document| core::IdentityDocument {
                    id: Uuid::new_v4().to_string().into(),
                    document_type: document.document_type,
                    number: document.number,
                    country: document.country,
                    expires: document.expires,
                })
                .collect(),
            emails: identity.emails,
            phones: identity.phones,
            address: identity.address,
            notes: identity.notes,
        }),
        EntryContent::Custom(mut custom) => {
            custom.fields.append(&mut additional_fields);
            SecretContent::Custom(core::CustomContent {
                records: records(custom.fields),
                notes: custom.notes,
            })
        }
        EntryContent::Wifi(wifi) => SecretContent::Wifi(core::WifiContent {
            ssid: wifi.ssid,
            password: wifi.password,
            hidden: wifi.hidden,
            notes: wifi.notes,
            security: match wifi.security {
                kryptopass_import::WifiSecurity::Open => core::WifiSecurity::Open,
                kryptopass_import::WifiSecurity::Wep => core::WifiSecurity::Wep,
                kryptopass_import::WifiSecurity::Wpa => core::WifiSecurity::Wpa,
                kryptopass_import::WifiSecurity::Wpa3 => core::WifiSecurity::Wpa3,
            },
        }),
        EntryContent::SshKey(key) => SecretContent::SshKey(core::SshKeyContent {
            private_key: key.private_key,
            public_key: key.public_key,
            fingerprint: key.fingerprint,
            algorithm: key.algorithm,
            user: key.user,
            notes: key.notes,
            passphrase: SensitiveText::new(),
            hosts: Vec::new(),
        }),
        EntryContent::Totp(totp) => SecretContent::Totp(core::TotpContent {
            config: otp(totp.secret)?.ok_or(ImportError::Content)?,
            notes: totp.notes,
        }),
        EntryContent::License(license) => SecretContent::License(core::LicenseContent {
            product: license.product,
            license_key: license.license_key,
            holder: license.holder,
            email: license.email,
            version: license.version,
            purchased: license.purchased,
            expires: license.expires,
            notes: license.notes,
        }),
    };
    // Extra concealed fields remain independently concealed in a custom entry.
    if !additional_fields.is_empty() {
        batch.items.push(ImportItem {
            title: title.to_string(),
            folder,
            favorite,
            trashed,
            content: SecretContent::Custom(core::CustomContent {
                records: records(additional_fields),
                notes: SensitiveText::new(),
            }),
        });
        adaptations.additional_entries += 1;
    }
    batch.items.push(ImportItem {
        title: title.to_string(),
        content,
        folder,
        favorite,
        trashed,
    });
    Ok(())
}

fn otp(value: OneTimePassword) -> Result<Option<core::TotpConfig>, ImportError> {
    let value = match value {
        OneTimePassword::Uri(value) | OneTimePassword::Base32(value) => value,
    };
    core::parse_totp_input(&value).map_err(|_| ImportError::Content)
}

fn records(fields: Vec<EntryField>) -> Vec<core::CustomRecord> {
    fields
        .into_iter()
        .map(|field| core::CustomRecord {
            id: Uuid::new_v4().to_string().into(),
            key: field.label,
            value: field.value,
            multiline: field.multiline,
            concealed: field.concealed,
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure/import_mapping.rs"]
mod tests;
