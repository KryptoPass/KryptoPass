//! Extensible password-manager import API with provider-independent entry data.

mod entry;
mod error;
mod importer;
mod limits;
mod registry;

pub mod importers;

pub use entry::{
    AliasEntry, CardEntry, CustomEntry, Entry, EntryContent, EntryField, EntryFolder,
    EntryLocation, EntryStatistics, EntryType, EntryWarning, EntryWarningKind, FolderId,
    IdentityDocument, IdentityEntry, ImportedEntries, LicenseEntry, LoginEntry, NoteEntry,
    OneTimePassword, SshKeyEntry, TotpEntry, WifiEntry, WifiSecurity,
};
pub use error::{ImportError, RegistryError};
pub use importer::{
    Detection, EncryptionSupport, FeatureSupport, ImportFormat, Importer, ImporterFeatures,
    ImporterInfo,
};
pub use importers::available_importers;
pub use limits::{ImportLimits, MAX_EXPORT_BYTES, MAX_RECORDS};
pub use registry::{DetectedImporter, ImporterRegistry, RegisteredImporter};
