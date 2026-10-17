//! Password-manager adapters produce a validated plan before any persistence.
//!
//! The host owns file selection and export decryption; core owns import policy. Plaintext
//! plans deliberately implement neither Debug nor Serialize; only counts and
//! issue categories should cross the UI boundary.

pub mod container;
pub mod proton;

use kryptopass_core::ImportBatch;
pub use kryptopass_core::MAX_IMPORT_RECORDS as MAX_RECORDS;
use thiserror::Error;

pub const MAX_EXPORT_BYTES: usize = 64 * 1024 * 1024;

/// Errors contain categories only, never source values or parser diagnostics.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    #[error("export exceeds the import limit")]
    Limit,
    #[error("invalid export or unsupported container")]
    InvalidExport,
    #[error("export could not be decrypted")]
    Decryption,
    #[error("unsupported export structure")]
    Unsupported,
    #[error("invalid folder hierarchy or duplicate source identity")]
    Hierarchy,
    #[error("content cannot be represented by the destination")]
    Content,
}

/// Extension point: new providers implement their own wire format translation.
pub trait ExportImporter {
    fn parse(&self, json: &[u8]) -> Result<ImportPlan, ImportError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    UnsupportedItem,
    AliasAsNote,
    ExtraFieldsInNotes,
    SourceFieldsPreserved,
    PasskeysInNotes,
    AutofillRulesInNotes,
    AttachmentsNotImported,
    TimestampsNotImported,
    VaultDescriptionNotImported,
}

/// Indices identify source positions without exposing names or credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportIssue {
    pub vault: usize,
    pub item: Option<usize>,
    pub kind: IssueKind,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    pub vaults: usize,
    pub folders: usize,
    pub source_items: usize,
    pub logins: usize,
    pub notes: usize,
    pub other_items: usize,
    pub skipped: usize,
    pub trashed: usize,
    pub warnings: usize,
}

/// Provider result; the batch is passed to core's ImportReceiver by the host.
pub struct ImportPlan {
    pub batch: ImportBatch,
    pub issues: Vec<ImportIssue>,
    pub summary: ImportSummary,
}
