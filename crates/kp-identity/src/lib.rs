//! Device identity keys and the durable trust records used by P2P pairing.
//!
//! Device identity keys are independent of a user's OpenPGP keys. Persistent
//! private key material is loaded only through [`SecureKeyStore`], and trust
//! is granted only by an explicit [`TrustStore::authorize_peer`] call; `kp-store`
//! persists it.

mod identity;
mod secure_store;
mod trust;

pub use identity::{
    DeviceId, DeviceIdentity, IdentityError, InvitationTokenHash, PublicIdentity, SignatureError,
    verify_signature,
};
pub use kryptopass_secure_memory::SensitiveBytes;
pub use secure_store::{SecureKeyStore, SecureStoreError, StoreIfAbsent, SystemSecretKeyStore};
#[cfg(target_os = "linux")]
pub type LinuxSecretServiceKeyStore = SystemSecretKeyStore;
pub use trust::{TrustStore, TrustStoreError};
