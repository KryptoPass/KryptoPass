use crate::{EntryType, ImportError, ImportLimits, ImportedEntries};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSupport {
    Unsupported,
    ReferenceOnly,
    Supported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionSupport {
    Unsupported,
    BuiltIn,
    RequiresDecryptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImporterFeatures {
    pub entry_types: &'static [EntryType],
    pub folders: bool,
    pub folder_descriptions: bool,
    pub favorites: bool,
    pub trash: bool,
    pub timestamps: bool,
    pub additional_fields: bool,
    pub passkeys: FeatureSupport,
    pub autofill_rules: FeatureSupport,
    pub attachments: FeatureSupport,
    pub encrypted_exports: EncryptionSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportFormat {
    pub extension: &'static str,
    pub media_type: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImporterInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub formats: &'static [ImportFormat],
    pub features: ImporterFeatures,
}

/// Detection must not decrypt data, request credentials, or perform I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Detection {
    Unsupported,
    Possible,
    Match,
}

/// Providers decode their own containers and return the common entry model.
///
/// The trait is object-safe so hosts can register providers as `dyn Importer`.
/// Implementations perform no persistence and never select a destination.
pub trait Importer: Send + Sync {
    fn info(&self) -> &'static ImporterInfo;

    fn decryption_available(&self) -> bool {
        self.info().features.encrypted_exports == EncryptionSupport::BuiltIn
    }
    fn detect(&self, input: &[u8], limits: &ImportLimits) -> Detection;
    fn import(&self, input: &[u8], limits: &ImportLimits) -> Result<ImportedEntries, ImportError>;
}
