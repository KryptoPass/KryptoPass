//! Desktop adapter between the existing OpenPGP capability and the signed
//! ciphertext ledger. The ledger remains the only mutable Sync authority.

#[cfg(test)]
#[path = "../../tests/unit/vault_sync.rs"]
mod tests;

pub(crate) mod mirror;

pub(crate) use kryptopass_vault::{SyncedVault, VaultHeadProjection, VaultSyncError};
