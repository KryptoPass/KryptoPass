use crate::ImportError;

pub const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_RECORDS: u32 = 10_000;

/// Resource limits apply before reading archives or requesting decryption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    pub input_bytes: u64,
    pub decoded_bytes: u64,
    /// Combined folder and source-entry count, including skipped entries.
    pub records: u32,
    pub archive_entries: u32,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            input_bytes: MAX_EXPORT_BYTES,
            decoded_bytes: MAX_EXPORT_BYTES,
            records: MAX_RECORDS,
            archive_entries: MAX_RECORDS,
        }
    }
}

impl ImportLimits {
    pub(crate) fn check_bytes(length: usize, maximum: u64) -> Result<(), ImportError> {
        if u64::try_from(length).map_err(|_| ImportError::Limit)? > maximum {
            return Err(ImportError::Limit);
        }
        Ok(())
    }
}
