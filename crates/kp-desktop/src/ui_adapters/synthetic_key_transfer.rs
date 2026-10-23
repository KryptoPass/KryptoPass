//! Debug-only preparation for the explicit synthetic OpenPGP key transfer.
//!
//! This module keeps fixture bytes in the backend and returns only the
//! send-owner DTO. It contains no QML properties and never reads the user's
//! normal GnuPG keyring.

use kryptopass_devices::{SyntheticKeyTransferSend, SyntheticTransferError};

#[cfg(debug_assertions)]
pub(crate) fn fingerprint() -> String {
    crate::infrastructure::openpgp::synthetic_transfer_fingerprint().unwrap_or_default()
}

#[cfg(not(debug_assertions))]
pub(crate) fn fingerprint() -> String {
    String::new()
}

#[cfg(debug_assertions)]
pub(crate) fn prepare(
    expected_fingerprint: &str,
) -> Result<([u8; 16], SyntheticKeyTransferSend), SyntheticTransferError> {
    use crate::infrastructure::openpgp;
    use uuid::Uuid;

    let mut material = openpgp::prepare_synthetic_transfer(expected_fingerprint)
        .map_err(|_| SyntheticTransferError::Invalid)?;
    let transfer_id = *Uuid::new_v4().as_bytes();
    let secret_key = std::mem::take(&mut *material.protected_secret_key);
    let prepared = SyntheticKeyTransferSend::new(
        transfer_id,
        material.fingerprint,
        secret_key,
        material.encrypted_challenge,
        material.challenge_sha256,
    )?;
    Ok((transfer_id, prepared))
}

#[cfg(not(debug_assertions))]
pub(crate) fn prepare(
    _expected_fingerprint: &str,
) -> Result<([u8; 16], SyntheticKeyTransferSend), SyntheticTransferError> {
    Err(SyntheticTransferError::Unsupported)
}
