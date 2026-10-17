use thiserror::Error;

/// Stable error categories without source values or parser diagnostics.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    #[error("export exceeds the configured import limits")]
    Limit,
    #[error("invalid export")]
    InvalidExport,
    #[error("this export requires a decryptor")]
    DecryptionRequired,
    #[error("export could not be decrypted")]
    Decryption,
    #[error("unsupported export format or version")]
    Unsupported,
    #[error("invalid folder hierarchy or duplicate source identity")]
    Hierarchy,
    #[error("invalid entry data")]
    Content,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    #[error("importer identifier is empty")]
    EmptyIdentifier,
    #[error("importer identifier is already registered")]
    DuplicateIdentifier,
}
