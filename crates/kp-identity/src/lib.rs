//! Device identity keys and the durable trust records used by P2P pairing.
//!
//! Device identity keys are independent of a user's OpenPGP keys. Persistent
//! private key material is loaded only through [`SecureKeyStore`], and trust
//! is granted only by an explicit [`SqliteTrustStore::authorize_peer`] call.

mod identity;
mod secure_store;
mod trust_store;

pub use identity::{
    DeviceId, DeviceIdentity, IdentityError, InvitationTokenHash, PublicIdentity, SignatureError,
    verify_signature,
};
pub use kryptopass_secure_memory::SensitiveBytes;
pub use secure_store::{SecureKeyStore, SecureStoreError, StoreIfAbsent, SystemSecretKeyStore};
#[cfg(target_os = "linux")]
pub type LinuxSecretServiceKeyStore = SystemSecretKeyStore;
pub use trust_store::{SqliteTrustStore, TrustStoreError};
