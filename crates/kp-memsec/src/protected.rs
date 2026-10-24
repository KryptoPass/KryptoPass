//! Fixed-capacity secret storage that refuses optional protection fallbacks.

use crate::{ProtectionStatus, SensitiveBytes};
use std::io;

/// A dedicated, locked region admitted before any private bytes are written.
/// Linux additionally requires exclusion from core dumps. This protects this
/// allocation, not borrowed views or copies made by external consumers.
pub struct ProtectedBytes(SensitiveBytes);

impl ProtectedBytes {
    pub fn try_with_capacity(capacity: usize) -> io::Result<Self> {
        let bytes = SensitiveBytes::try_with_capacity(capacity.max(1))?;
        let status = bytes.protection_status();
        if !status.dedicated
            || !status.locked
            || (cfg!(target_os = "linux") && !status.excluded_from_dumps)
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "required memory protection unavailable",
            ));
        }
        Ok(Self(bytes))
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn protection_status(&self) -> ProtectionStatus {
        self.0.protection_status()
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.as_mut_slice()
    }

    /// Never reallocates: an oversized write leaves the previous contents intact.
    pub fn try_extend_from_slice(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.capacity())
        {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "protected region capacity exceeded",
            ));
        }
        self.0.try_extend_from_slice(bytes)
    }

    /// Initializes the complete owned region without growing its reservation.
    pub fn fill_capacity(&mut self) {
        self.0.resize(self.capacity(), 0);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl io::Write for ProtectedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.try_extend_from_slice(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
