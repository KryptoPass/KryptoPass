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
    let mut content = match content {
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
            // A custom secret already holds fields, so extras need no companion.
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
    // Original dates are plain context, so they go to the notes instead of a companion.
    if let Some(line) = original_dates_line(created_at, modified_at) {
        let notes = notes_mut(&mut content);
        if !notes.is_empty() {
            notes.push_str("\n\n");
        }
        notes.push_str(&line);
        adaptations.timestamps_as_fields += 1;
    }
    let main = batch.items.len();
    batch.items.push(ImportItem {
        title: title.to_string(),
        content,
        folder,
        favorite,
        trashed,
    });
    // Extra fields keep their concealment in a custom secret linked to the entry.
    if !additional_fields.is_empty() {
        let companion = batch.items.len();
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
        batch.links.push((main, companion));
        adaptations.additional_entries += 1;
    }
    Ok(())
}

fn notes_mut(content: &mut SecretContent) -> &mut SensitiveText {
    match content {
        SecretContent::Login(content) => &mut content.notes,
        SecretContent::Note(content) => &mut content.notes,
        SecretContent::Custom(content) => &mut content.notes,
        SecretContent::Card(content) => &mut content.notes,
        SecretContent::Totp(content) => &mut content.notes,
        SecretContent::Identity(content) => &mut content.notes,
        SecretContent::Wifi(content) => &mut content.notes,
        SecretContent::License(content) => &mut content.notes,
        SecretContent::SshKey(content) => &mut content.notes,
    }
}

fn original_dates_line(created_at: Option<u64>, modified_at: Option<u64>) -> Option<String> {
    let parts: Vec<_> = [
        ("Creado el %1", created_at),
        ("Modificado el %1", modified_at),
    ]
    .into_iter()
    .filter_map(|(label, value)| {
        value.map(|seconds| crate::i18n::format(label, &[utc_date(seconds)]))
    })
    .collect();
    (!parts.is_empty()).then(|| crate::i18n::format("Fechas originales: %1", &[parts.join(" · ")]))
}

/// Calendar date (UTC) for Unix seconds, using the proleptic Gregorian calendar.
fn utc_date(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX / 2) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
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
