//! Durable trust in other devices and single-use pairing invitations.
//!
//! This crate defines what trust storage must do; `kp-store` keeps it in a
//! private SQLite file. Trust is granted only by an explicit
//! [`TrustStore::authorize_peer`] call and is never transitive.

use crate::{DeviceId, InvitationTokenHash, PublicIdentity, SignatureError};
use thiserror::Error;

/// Trust and invitation state for one local device identity.
pub trait TrustStore {
    /// The identity this store is bound to.
    fn local_device_id(&self) -> DeviceId;

    /// Persists explicit authorization for an authenticated peer.
    ///
    /// Call only after the pairing engine has authenticated the remote signing
    /// key and the local user has approved the peer. It never replaces the
    /// keys of an existing id, and a revoked peer stays revoked.
    fn authorize_peer(
        &mut self,
        peer: &PublicIdentity,
        approved_at_ms: i64,
    ) -> Result<(), TrustStoreError>;

    /// An authorized peer; unknown and revoked ids fail closed.
    fn lookup_trusted_peer(&self, device_id: &DeviceId) -> Result<PublicIdentity, TrustStoreError>;

    /// Every authorized peer in device-id order, never silently truncated.
    fn list_trusted_peers(&self) -> Result<Vec<PublicIdentity>, TrustStoreError>;

    /// Permanently revokes an authorized peer, keeping its key history.
    fn revoke_peer(
        &mut self,
        device_id: &DeviceId,
        revoked_at_ms: i64,
    ) -> Result<(), TrustStoreError>;

    /// Removes an active authorization so the peer can be paired again. A
    /// revoked peer is never removed.
    fn forget_authorized_peer(&mut self, device_id: &DeviceId) -> Result<(), TrustStoreError>;

    /// Stores a one-time invitation digest, never the raw token. Expired
    /// invitations are pruned in the same transaction. Times are Unix epoch
    /// milliseconds.
    fn register_invitation(
        &mut self,
        invite_id: [u8; 16],
        raw_token: &[u8; 32],
        expires_at_ms: i64,
        now_ms: i64,
    ) -> Result<InvitationTokenHash, TrustStoreError>;

    /// Consumes a registered invitation once and before it expires.
    fn consume_invitation(
        &mut self,
        invite_id: &[u8; 16],
        token_hash: &InvitationTokenHash,
        now_ms: i64,
    ) -> Result<(), TrustStoreError>;
}

/// Errors returned by trust storage.
#[derive(Debug, Error)]
pub enum TrustStoreError {
    /// This platform has no hardened profile storage implementation.
    #[error("private trust storage is unsupported on this platform")]
    UnsupportedPlatform,
    /// The profile path or its permissions are unsafe.
    #[error("profile directory or database file has unsafe permissions")]
    InvalidProfile,
    /// New storage cannot replace an existing file.
    #[error("trust database already exists")]
    DatabaseExists,
    /// The expected storage does not exist.
    #[error("trust database is missing")]
    MissingDatabase,
    /// Storage reports corruption or failed its integrity check.
    #[error("trust database is corrupt")]
    CorruptDatabase,
    /// The schema, application id or schema version is unsupported.
    #[error("trust database schema is unsupported")]
    SchemaMismatch,
    /// The storage belongs to another local device identity.
    #[error("trust database is bound to a different device identity")]
    IdentityMismatch,
    /// Attempted to authorize or revoke the local device itself.
    #[error("a device cannot be trusted or revoked as its own peer")]
    SelfTrust,
    /// A peer id was reused with different public keys.
    #[error("peer keys conflict with an existing device identifier")]
    KeyConflict,
    /// A peer was explicitly revoked and cannot be silently reauthorized.
    #[error("peer is revoked")]
    PeerRevoked,
    /// The peer is absent or is not currently authorized.
    #[error("peer is unknown or revoked")]
    UnknownOrRevokedPeer,
    /// The configured peer count limit has been reached.
    #[error("trusted peer limit reached")]
    PeerLimitReached,
    /// A peer listing exceeded the maximum number of authorized peers.
    #[error("trusted peer list exceeds the configured limit")]
    TrustedPeerListTooLarge,
    /// The invitation id or digest has already been registered.
    #[error("invitation is already registered")]
    InvitationAlreadyRegistered,
    /// The configured active invitation limit has been reached.
    #[error("invitation ledger limit reached")]
    InvitationLimitReached,
    /// The invitation is unknown, expired, or already consumed.
    #[error("invitation is unavailable")]
    InvitationUnavailable,
    /// An invitation id or token is the prohibited all-zero value.
    #[error("invalid invitation identifier or token")]
    InvalidInvitation,
    /// A timestamp is negative or the invitation expiry is not in the future.
    #[error("invalid timestamp")]
    InvalidTimestamp,
    /// Public identity fields do not form a valid identity.
    #[error(transparent)]
    InvalidIdentity(SignatureError),
    /// The storage backend failed; stored values are never included.
    #[error("trust storage operation failed")]
    Storage,
}
