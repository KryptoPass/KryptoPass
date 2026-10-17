use crate::{ImportError, ImportLimits};
use kryptopass_secure_memory::SensitiveText;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FolderId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryType {
    Login,
    Note,
    Alias,
    Card,
    Identity,
    Custom,
    Wifi,
    SshKey,
    Totp,
    License,
}

/// Source text stays owned by sensitive memory; no payload type exposes Debug or Serialize.
pub struct Entry {
    pub title: SensitiveText,
    pub folder: Option<FolderId>,
    pub content: EntryContent,
    pub favorite: bool,
    pub trashed: bool,
    /// Unix timestamps in seconds, when present in the source.
    pub created_at: Option<u64>,
    pub modified_at: Option<u64>,
    pub additional_fields: Vec<EntryField>,
}

pub struct EntryFolder {
    pub id: FolderId,
    pub parent: Option<FolderId>,
    pub name: SensitiveText,
    pub description: SensitiveText,
}

pub enum EntryContent {
    Login(LoginEntry),
    Note(NoteEntry),
    Alias(AliasEntry),
    Card(CardEntry),
    Identity(IdentityEntry),
    Custom(CustomEntry),
    Wifi(WifiEntry),
    SshKey(SshKeyEntry),
    Totp(TotpEntry),
    License(LicenseEntry),
}

impl EntryContent {
    pub fn entry_type(&self) -> EntryType {
        match self {
            Self::Login(_) => EntryType::Login,
            Self::Note(_) => EntryType::Note,
            Self::Alias(_) => EntryType::Alias,
            Self::Card(_) => EntryType::Card,
            Self::Identity(_) => EntryType::Identity,
            Self::Custom(_) => EntryType::Custom,
            Self::Wifi(_) => EntryType::Wifi,
            Self::SshKey(_) => EntryType::SshKey,
            Self::Totp(_) => EntryType::Totp,
            Self::License(_) => EntryType::License,
        }
    }
}

#[derive(Default)]
pub struct LoginEntry {
    pub username: SensitiveText,
    pub email: SensitiveText,
    pub password: SensitiveText,
    pub urls: Vec<SensitiveText>,
    pub otp: Option<OneTimePassword>,
    pub notes: SensitiveText,
}

/// Preserve a standard URI or a source Base32 secret without generating codes.
pub enum OneTimePassword {
    Uri(SensitiveText),
    Base32(SensitiveText),
}

#[derive(Default)]
pub struct NoteEntry {
    pub value: SensitiveText,
}

#[derive(Default)]
pub struct AliasEntry {
    pub email: SensitiveText,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct CardEntry {
    pub holder: SensitiveText,
    pub number: SensitiveText,
    pub expiry_month: u8,
    pub expiry_year: u16,
    pub cvv: SensitiveText,
    pub pin: SensitiveText,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct IdentityEntry {
    pub first_name: SensitiveText,
    pub last_name: SensitiveText,
    pub documents: Vec<IdentityDocument>,
    pub emails: Vec<SensitiveText>,
    pub phones: Vec<SensitiveText>,
    pub address: SensitiveText,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct IdentityDocument {
    pub document_type: SensitiveText,
    pub number: SensitiveText,
    pub country: SensitiveText,
    pub expires: SensitiveText,
}

#[derive(Default)]
pub struct CustomEntry {
    pub fields: Vec<EntryField>,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct EntryField {
    pub label: SensitiveText,
    pub value: SensitiveText,
    pub concealed: bool,
    pub multiline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiSecurity {
    Open,
    Wep,
    Wpa,
    Wpa3,
}

pub struct WifiEntry {
    pub ssid: SensitiveText,
    pub security: WifiSecurity,
    pub password: SensitiveText,
    pub hidden: bool,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct SshKeyEntry {
    pub private_key: SensitiveText,
    pub public_key: SensitiveText,
    pub fingerprint: SensitiveText,
    pub algorithm: SensitiveText,
    pub user: SensitiveText,
    pub notes: SensitiveText,
}

pub struct TotpEntry {
    pub secret: OneTimePassword,
    pub notes: SensitiveText,
}

#[derive(Default)]
pub struct LicenseEntry {
    pub product: SensitiveText,
    pub license_key: SensitiveText,
    pub holder: SensitiveText,
    pub email: SensitiveText,
    pub version: SensitiveText,
    pub purchased: SensitiveText,
    pub expires: SensitiveText,
    pub notes: SensitiveText,
}

/// Ordinals identify source positions, including entries that were skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryLocation {
    pub collection: u32,
    pub entry: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryWarningKind {
    UnsupportedEntryType,
    SourceFieldsPreserved,
    PasskeysAsReferences,
    AutofillRulesAsReferences,
    AttachmentsNotImported,
    ConflictingPublicKey,
    UnreadableSshKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryWarning {
    pub location: EntryLocation,
    pub kind: EntryWarningKind,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntryStatistics {
    pub collections: u32,
    pub folders: u32,
    pub source_entries: u32,
    pub imported_entries: u32,
    pub skipped_entries: u32,
    pub trashed_entries: u32,
    pub warnings: u32,
    pub entries_by_type: BTreeMap<EntryType, u32>,
}

/// A bounded, structurally valid dataset; destination policy belongs to its consumer.
pub struct ImportedEntries {
    folders: Vec<EntryFolder>,
    entries: Vec<Entry>,
    warnings: Vec<EntryWarning>,
    statistics: EntryStatistics,
}

impl ImportedEntries {
    pub fn new(
        folders: Vec<EntryFolder>,
        entries: Vec<Entry>,
        warnings: Vec<EntryWarning>,
        collections: u32,
        source_entries: u32,
        limits: &ImportLimits,
    ) -> Result<Self, ImportError> {
        let folder_count = u32::try_from(folders.len()).map_err(|_| ImportError::Limit)?;
        let imported_entries = u32::try_from(entries.len()).map_err(|_| ImportError::Limit)?;
        let warning_count = u32::try_from(warnings.len()).map_err(|_| ImportError::Limit)?;
        if folder_count
            .checked_add(source_entries)
            .ok_or(ImportError::Limit)?
            > limits.records
        {
            return Err(ImportError::Limit);
        }
        let skipped_entries = source_entries
            .checked_sub(imported_entries)
            .ok_or(ImportError::Content)?;
        let parents: HashMap<_, _> = folders
            .iter()
            .map(|folder| (folder.id, folder.parent))
            .collect();
        if parents.len() != folders.len() {
            return Err(ImportError::Hierarchy);
        }
        let mut complete = HashSet::new();
        for folder in &folders {
            let mut path = HashSet::new();
            let mut next = Some(folder.id);
            while let Some(id) = next {
                if complete.contains(&id) {
                    break;
                }
                if !path.insert(id) {
                    return Err(ImportError::Hierarchy);
                }
                next = *parents.get(&id).ok_or(ImportError::Hierarchy)?;
            }
            complete.extend(path);
        }
        let mut statistics = EntryStatistics {
            collections,
            folders: folder_count,
            source_entries,
            imported_entries,
            skipped_entries,
            warnings: warning_count,
            ..EntryStatistics::default()
        };
        for entry in &entries {
            if entry.folder.is_some_and(|id| !parents.contains_key(&id)) {
                return Err(ImportError::Hierarchy);
            }
            *statistics
                .entries_by_type
                .entry(entry.content.entry_type())
                .or_default() += 1;
            statistics.trashed_entries += u32::from(entry.trashed);
        }
        for warning in &warnings {
            if warning.location.collection >= collections
                || warning
                    .location
                    .entry
                    .is_some_and(|index| index >= source_entries)
            {
                return Err(ImportError::Content);
            }
        }
        Ok(Self {
            folders,
            entries,
            warnings,
            statistics,
        })
    }

    pub fn folders(&self) -> &[EntryFolder] {
        &self.folders
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn warnings(&self) -> &[EntryWarning] {
        &self.warnings
    }
    pub fn statistics(&self) -> &EntryStatistics {
        &self.statistics
    }

    pub fn into_parts(self) -> (Vec<EntryFolder>, Vec<Entry>, Vec<EntryWarning>) {
        (self.folders, self.entries, self.warnings)
    }
}
