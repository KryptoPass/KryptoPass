//! Bounded sender for the explicitly negotiated synthetic provisioning exercise.
//! OpenPGP export, human authorization and durable import remain application duties.

use kryptopass_protocol::{MessagePayload, proto};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Maximum protected key object accepted by the laboratory sender.
pub const SYNTHETIC_KEY_MAX_BYTES: usize = 1024 * 1024;
/// Maximum bytes per ordered chunk.
pub const SYNTHETIC_KEY_CHUNK_BYTES: usize = 32 * 1024;
/// Lifetime of one operation, including both human decisions.
pub const SYNTHETIC_KEY_TIMEOUT: Duration = Duration::from_secs(120);

/// Public, secret-free state of a transfer in one authenticated session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntheticTransferPhase {
    /// This session did not negotiate the extension.
    Unsupported,
    /// Both endpoints support the extension; no operation has started.
    Ready,
    /// Waiting for the receiving user's explicit acceptance.
    WaitingAcceptance,
    /// Sending ordered chunks after acceptance.
    Sending,
    /// All bytes were sent; durable import has not been acknowledged.
    WaitingReceipt,
    /// The recipient acknowledged storage and the correct decryption challenge.
    Complete,
    /// The receiving user declined the offer.
    Rejected,
    /// The operation was explicitly cancelled.
    Cancelled,
    /// The operation expired before a successful receipt.
    Expired,
    /// The operation failed before a complete object was sent.
    Failed,
    /// The connection ended after sending an object, without a valid receipt.
    Uncertain,
}

/// Observable metadata; never contains protected key bytes or challenge plaintext.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntheticTransferProgress {
    /// Current operation state.
    pub phase: SyntheticTransferPhase,
    /// Identifier of the operation, if one was started in this session.
    pub transfer_id: Option<[u8; 16]>,
}

/// Bounded errors that do not retain or render message contents.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SyntheticTransferError {
    /// The extension was not negotiated by this session.
    #[error("synthetic transfer is unsupported")]
    Unsupported,
    /// A session permits only one operation, including rejected operations.
    #[error("synthetic transfer already used or busy")]
    Busy,
    /// A message, local object or state transition was invalid.
    #[error("synthetic transfer validation failed")]
    Invalid,
    /// The operation exceeded its fixed lifetime.
    #[error("synthetic transfer expired")]
    Expired,
    /// A callback targets an inactive, replaced or unauthorized session.
    #[error("synthetic transfer session is no longer current")]
    WrongSession,
}

/// Prepared bytes owned by the session task, never by a UI property or a log.
/// Construction does not authorize sending: the owner requires a current trusted
/// session and an explicit local command; chunks also require remote acceptance.
pub struct SyntheticKeyTransferSend {
    offer: proto::SyntheticKeyOffer,
    key: Zeroizing<Vec<u8>>,
    challenge_sha256: [u8; 32],
}

impl SyntheticKeyTransferSend {
    /// Validates local preparation and computes the protected object's digest.
    pub fn new(
        transfer_id: [u8; 16],
        fingerprint: Vec<u8>,
        secret_key: Vec<u8>,
        encrypted_challenge: Vec<u8>,
        challenge_sha256: [u8; 32],
    ) -> Result<Self, SyntheticTransferError> {
        let key = Zeroizing::new(secret_key);
        if transfer_id == [0; 16]
            || fingerprint.len() != 20
            || fingerprint.iter().all(|byte| *byte == 0)
            || key.is_empty()
            || key.len() > SYNTHETIC_KEY_MAX_BYTES
            || encrypted_challenge.is_empty()
            || encrypted_challenge.len() > 16 * 1024
        {
            return Err(SyntheticTransferError::Invalid);
        }
        Ok(Self {
            offer: proto::SyntheticKeyOffer {
                transfer_id: transfer_id.to_vec(),
                openpgp_fingerprint: fingerprint,
                total_size: key.len() as u64,
                sha256_digest: Sha256::digest(key.as_slice()).to_vec(),
                encrypted_challenge,
            },
            key,
            challenge_sha256,
        })
    }
}

/// One typed frame for the unique Noise send owner. Intentionally not Debug.
pub enum SyntheticTransferOutbound {
    /// The offer contains no private key material.
    Offer(proto::SyntheticKeyOffer),
    /// One protected key fragment. The owner wipes its temporary data after send.
    Chunk(proto::SyntheticKeyChunk),
    /// The sender completed the stream; this is not an import receipt.
    Complete(proto::SyntheticKeyComplete),
    /// Cancellation, timeout or failure of this exact transfer.
    Abort(proto::SyntheticKeyAbort),
}

/// A one-operation sender. Session generation and trust are enforced by its owner.
pub struct SyntheticTransferSender {
    progress: SyntheticTransferProgress,
    prepared: Option<SyntheticKeyTransferSend>,
    deadline: Option<Instant>,
    next_chunk: usize,
    used: bool,
}

impl SyntheticTransferSender {
    /// Creates an idle actor after bilateral capability negotiation.
    #[must_use]
    pub fn new(supported: bool) -> Self {
        Self {
            progress: SyntheticTransferProgress {
                phase: if supported {
                    SyntheticTransferPhase::Ready
                } else {
                    SyntheticTransferPhase::Unsupported
                },
                transfer_id: None,
            },
            prepared: None,
            deadline: None,
            next_chunk: 0,
            used: false,
        }
    }

    /// Returns only metadata suitable for observable application state.
    #[must_use]
    pub const fn progress(&self) -> SyntheticTransferProgress {
        self.progress
    }

    /// Starts an explicitly authorized local operation, without emitting key bytes.
    pub fn start(
        &mut self,
        prepared: SyntheticKeyTransferSend,
        now: Instant,
    ) -> Result<SyntheticTransferOutbound, SyntheticTransferError> {
        if self.progress.phase == SyntheticTransferPhase::Unsupported {
            return Err(SyntheticTransferError::Unsupported);
        }
        if self.used || self.progress.phase != SyntheticTransferPhase::Ready {
            return Err(SyntheticTransferError::Busy);
        }
        self.used = true;
        self.progress.transfer_id = Some(
            prepared
                .offer
                .transfer_id
                .as_slice()
                .try_into()
                .map_err(|_| SyntheticTransferError::Invalid)?,
        );
        self.progress.phase = SyntheticTransferPhase::WaitingAcceptance;
        self.deadline = Some(now + SYNTHETIC_KEY_TIMEOUT);
        let offer = prepared.offer.clone();
        self.prepared = Some(prepared);
        Ok(SyntheticTransferOutbound::Offer(offer))
    }

    /// Handles an authenticated decision, receipt or cancellation from the peer.
    pub fn receive(
        &mut self,
        payload: MessagePayload,
        now: Instant,
    ) -> Result<(), SyntheticTransferError> {
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            self.finish_unconfirmed(SyntheticTransferPhase::Expired);
            return Err(SyntheticTransferError::Expired);
        }
        let Some(id) = self.progress.transfer_id else {
            return self.invalid();
        };
        match payload {
            MessagePayload::SyntheticKeyDecision(decision)
                if decision.transfer_id == id
                    && self.progress.phase == SyntheticTransferPhase::WaitingAcceptance =>
            {
                if decision.accepted {
                    self.progress.phase = SyntheticTransferPhase::Sending;
                } else {
                    self.finish(SyntheticTransferPhase::Rejected);
                }
                Ok(())
            }
            MessagePayload::SyntheticKeyReceipt(receipt)
                if receipt.transfer_id == id
                    && self.progress.phase == SyntheticTransferPhase::WaitingReceipt =>
            {
                let valid = self.prepared.as_ref().is_some_and(|prepared| {
                    receipt.openpgp_fingerprint == prepared.offer.openpgp_fingerprint
                        && receipt.challenge_sha256.as_slice() == prepared.challenge_sha256
                });
                if !valid {
                    return self.invalid();
                }
                self.finish(SyntheticTransferPhase::Complete);
                Ok(())
            }
            MessagePayload::SyntheticKeyAbort(abort)
                if abort.transfer_id == id
                    && self.deadline.is_some()
                    && matches!(abort.code, 1..=4) =>
            {
                self.finish_unconfirmed(match abort.code {
                    1 => SyntheticTransferPhase::Cancelled,
                    2 => SyntheticTransferPhase::Expired,
                    3 => SyntheticTransferPhase::Rejected,
                    _ => SyntheticTransferPhase::Failed,
                });
                Ok(())
            }
            _ => self.invalid(),
        }
    }

    /// Produces at most one frame so the owner can interleave keepalives and Sync.
    pub fn next_outbound(
        &mut self,
        now: Instant,
    ) -> Result<Option<SyntheticTransferOutbound>, SyntheticTransferError> {
        if let Some(abort) = self.expire(now) {
            return Ok(Some(abort));
        }
        if self.progress.phase != SyntheticTransferPhase::Sending {
            return Ok(None);
        }
        let prepared = self
            .prepared
            .as_ref()
            .ok_or(SyntheticTransferError::Invalid)?;
        let count = prepared.key.len().div_ceil(SYNTHETIC_KEY_CHUNK_BYTES);
        if self.next_chunk == count {
            self.progress.phase = SyntheticTransferPhase::WaitingReceipt;
            return Ok(Some(SyntheticTransferOutbound::Complete(
                proto::SyntheticKeyComplete {
                    transfer_id: prepared.offer.transfer_id.clone(),
                },
            )));
        }
        let start = self.next_chunk * SYNTHETIC_KEY_CHUNK_BYTES;
        let end = (start + SYNTHETIC_KEY_CHUNK_BYTES).min(prepared.key.len());
        let chunk = proto::SyntheticKeyChunk {
            transfer_id: prepared.offer.transfer_id.clone(),
            chunk_index: self.next_chunk as u32,
            chunk_count: count as u32,
            total_size: prepared.offer.total_size,
            sha256_digest: prepared.offer.sha256_digest.clone(),
            data: prepared.key[start..end].to_vec(),
        };
        self.next_chunk += 1;
        Ok(Some(SyntheticTransferOutbound::Chunk(chunk)))
    }

    /// Explicit cancellation targets the currently active operation only.
    pub fn cancel(
        &mut self,
        transfer_id: [u8; 16],
        now: Instant,
    ) -> Result<SyntheticTransferOutbound, SyntheticTransferError> {
        if self.progress.transfer_id != Some(transfer_id) || self.deadline.is_none() {
            return Err(SyntheticTransferError::WrongSession);
        }
        if let Some(expired) = self.expire(now) {
            return Ok(expired);
        }
        self.finish_unconfirmed(SyntheticTransferPhase::Cancelled);
        Ok(SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
            transfer_id: transfer_id.to_vec(),
            code: 1,
        }))
    }

    /// Expires the whole operation, including time spent in the import screen.
    pub fn expire(&mut self, now: Instant) -> Option<SyntheticTransferOutbound> {
        if self.deadline.is_none_or(|deadline| now < deadline) {
            return None;
        }
        let id = self.progress.transfer_id?;
        self.finish_unconfirmed(SyntheticTransferPhase::Expired);
        Some(SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
            transfer_id: id.to_vec(),
            code: 2,
        }))
    }

    /// Invalidates pending state on transport loss or session replacement.
    pub fn disconnected(&mut self) {
        if self.deadline.is_some() {
            self.finish(
                if matches!(
                    self.progress.phase,
                    SyntheticTransferPhase::WaitingReceipt | SyntheticTransferPhase::Uncertain
                ) {
                    SyntheticTransferPhase::Uncertain
                } else {
                    SyntheticTransferPhase::Failed
                },
            );
        }
    }

    fn invalid(&mut self) -> Result<(), SyntheticTransferError> {
        self.finish(
            if matches!(
                self.progress.phase,
                SyntheticTransferPhase::WaitingReceipt | SyntheticTransferPhase::Uncertain
            ) {
                SyntheticTransferPhase::Uncertain
            } else {
                SyntheticTransferPhase::Failed
            },
        );
        Err(SyntheticTransferError::Invalid)
    }

    fn finish_unconfirmed(&mut self, phase: SyntheticTransferPhase) {
        self.finish(
            if matches!(
                self.progress.phase,
                SyntheticTransferPhase::WaitingReceipt | SyntheticTransferPhase::Uncertain
            ) {
                SyntheticTransferPhase::Uncertain
            } else {
                phase
            },
        );
    }

    fn finish(&mut self, phase: SyntheticTransferPhase) {
        self.progress.phase = phase;
        self.prepared = None;
        self.deadline = None;
    }
}
