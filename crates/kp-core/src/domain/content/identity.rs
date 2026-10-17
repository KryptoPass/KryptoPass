//! Personal identity payloads.

use std::collections::HashSet;

use crate::SensitiveText;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;

const MAX_DOCUMENTS: usize = 100;
const MAX_CONTACTS: usize = 100;
const MAX_FIELD_BYTES: usize = 16 * 1024;
const MAX_CONTACT_BYTES: usize = 2 * 1024;
const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Personal identity fields kept inside the encrypted payload.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityContent {
    pub first_name: SensitiveText,
    pub last_name: SensitiveText,
    pub documents: Vec<IdentityDocument>,
    pub emails: Vec<SensitiveText>,
    pub phones: Vec<SensitiveText>,
    pub address: SensitiveText,
    pub notes: SensitiveText,
}

/// A document number and its optional issuer and expiration details.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityDocument {
    pub id: SensitiveText,
    pub document_type: SensitiveText,
    pub number: SensitiveText,
    pub country: SensitiveText,
    pub expires: SensitiveText,
}

impl IdentityContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure_field_size("identity first name", &self.first_name, MAX_FIELD_BYTES)?;
        ensure_field_size("identity last name", &self.last_name, MAX_FIELD_BYTES)?;
        ensure!(
            self.documents.len() <= MAX_DOCUMENTS,
            "too many identity documents"
        );
        let mut document_ids = HashSet::with_capacity(self.documents.len());
        for document in &self.documents {
            let id = Uuid::parse_str(&document.id).context("invalid identity document ID")?;
            ensure!(!id.is_nil(), "invalid identity document ID");
            ensure!(
                document.id.as_bytes() == id.hyphenated().encode_lower(&mut [0; 36]).as_bytes(),
                "identity document ID is not canonical"
            );
            ensure!(document_ids.insert(id), "duplicate identity document ID");
            ensure_field_size("identity document type", &document.document_type, 256)?;
            ensure_field_size(
                "identity document number",
                &document.number,
                MAX_FIELD_BYTES,
            )?;
            ensure_field_size("identity document country", &document.country, 256)?;
            ensure_field_size("identity document expiration", &document.expires, 256)?;
        }
        validate_contacts("identity email", &self.emails)?;
        validate_contacts("identity phone", &self.phones)?;
        ensure_field_size("identity address", &self.address, MAX_FIELD_BYTES)?;
        ensure_field_size("identity notes", &self.notes, MAX_NOTES_BYTES)
    }
}

fn validate_contacts(name: &str, contacts: &[SensitiveText]) -> Result<()> {
    ensure!(contacts.len() <= MAX_CONTACTS, "too many identity contacts");
    for contact in contacts {
        ensure_field_size(name, contact, MAX_CONTACT_BYTES)?;
    }
    Ok(())
}
