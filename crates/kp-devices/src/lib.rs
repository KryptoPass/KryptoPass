//! Trusted-device directory and active authenticated-session ownership.
//!
//! This crate contains no UI, persistence, GnuPG, or carrier implementation.
//! Applications feed it validated trusted identities and retain the returned
//! session handle for the lifetime of a connection.

/// Phone → Desktop vault-identity receiver (extension version 3).
pub mod approvals;
pub mod synthetic_receive;
/// Explicit synthetic provisioning sender, independent of UI and OpenPGP engines.
pub mod synthetic_transfer;
pub use synthetic_receive::{
    INBOUND_KEY_TIMEOUT, INBOUND_KEY_TRANSFER_VERSION, InboundKeyMaterial, InboundTransferFailure,
    InboundTransferPhase, InboundTransferProgress,
};
use synthetic_receive::{InboundMaterialSlot, SyntheticTransferReceiver};
pub use synthetic_transfer::{
    KEY_TRANSFER_VERSION, KeyTransferPurpose, SYNTHETIC_KEY_CHUNK_BYTES, SYNTHETIC_KEY_MAX_BYTES,
    SYNTHETIC_KEY_TIMEOUT, SyntheticKeyTransferSend, SyntheticTransferError,
    SyntheticTransferOutbound, SyntheticTransferPhase, SyntheticTransferProgress,
    SyntheticTransferSender,
};

mod close;
mod directory;
mod handle;
mod hello;
mod outbound;
mod rate_limit;
mod session;
mod start;
mod sync_worker;

use close::*;
pub use directory::*;
pub use handle::*;
use hello::*;
use outbound::*;
use rate_limit::*;
use session::*;
pub use start::*;
use sync_worker::*;

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError as StdTrySendError, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use zeroize::Zeroize;

use kryptopass_identity::{DeviceId, PublicIdentity};
use kryptopass_pairing::{EstablishedSession, PairingError};
use kryptopass_protocol::{Channel, MessagePayload, proto};
use kryptopass_sync::{
    Fingerprint, SyncOutbound, SyncProgress, SyncSession, SyncSessionError, SyncStore, TrustSource,
};
use kryptopass_transport::PeerTransport;
pub use kryptopass_transport::{ConnectionFamily, ConnectionInfo, ConnectionRoute};
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::{Instant, sleep_until};

/// Maximum number of trusted peers accepted by the public read model.
pub const MAX_TRUSTED_DEVICES: usize = 1024;

/// Active-session extension version understood by this implementation.
pub const ACTIVE_SESSION_VERSION: u32 = 1;

/// Session capability: the peer may declare its OpenPGP keys in `VaultKeys`.
const VAULT_KEYS_VERSION: u32 = 1;

/// Interval between keepalive requests when the previous request was answered.
pub const SESSION_PING_INTERVAL: Duration = Duration::from_secs(3);

/// Maximum time allowed for the outstanding keepalive request.
pub const SESSION_PING_TIMEOUT: Duration = Duration::from_secs(6);

/// Maximum time allowed for the initial authenticated session hello.
pub const SESSION_HELLO_TIMEOUT: Duration = Duration::from_secs(30);

const SESSION_COMMAND_CAPACITY: usize = 1;

const SESSION_EVENT_CAPACITY: usize = 16;

const MAX_CONTROL_MESSAGES_PER_SECOND: usize = 32;

const MAX_DATA_MESSAGES_PER_SECOND: usize = 4096;

const MAX_SYNC_CONTROL_MESSAGES_PER_SECOND: usize = 2048;

const SESSION_CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(10);

const SYNC_WORKER_INPUT_CAPACITY: usize = 4;

const SYNC_WORKER_OUTPUT_CAPACITY: usize = 2;

/// Options advertised when a pairing outcome becomes an active session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionOptions {
    /// Local LAN resume port carried inside the authenticated session hello;
    /// zero means that this endpoint does not accept LAN resume connections.
    pub reconnect_port: u16,
}

/// Explicit opt-in binding of a Sync store, a trust source and the keys this
/// device syncs with the peer.
///
/// Supplying this value is the only way to advertise Sync from a session. It
/// does not create trust or inspect an OpenPGP key; the keys only route what
/// the peer is offered.
pub struct SyncSessionBinding {
    store: Box<dyn SyncStore + Send>,
    trust: Box<dyn TrustSource + Send>,
    keys: Vec<Fingerprint>,
    automatic_rounds: bool,
}

impl SyncSessionBinding {
    #[must_use]
    pub fn new(
        store: Box<dyn SyncStore + Send>,
        trust: Box<dyn TrustSource + Send>,
        keys: Vec<Fingerprint>,
    ) -> Self {
        Self {
            store,
            trust,
            keys,
            automatic_rounds: false,
        }
    }

    /// Opts this binding into automatic follow-up rounds when both peers
    /// advertise continuous Sync support in their authenticated session hello.
    #[must_use]
    pub fn with_automatic_rounds(mut self) -> Self {
        self.automatic_rounds = true;
        self
    }
}

impl fmt::Debug for SyncSessionBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncSessionBinding")
            .field("keys", &self.keys.len())
            .finish_non_exhaustive()
    }
}
