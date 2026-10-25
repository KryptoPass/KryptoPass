//! Bounded receiver for the phone → Desktop vault-identity transfer
//! (extension version 3, `key_purpose = 1`). Human authorization, OpenPGP
//! validation, GnuPG import and challenge decryption remain application duties;
//! this actor only enforces the wire contract and keeps the received bytes in
//! zeroizing memory until the application takes them.

use crate::synthetic_transfer::{
    SYNTHETIC_KEY_CHUNK_BYTES, SYNTHETIC_KEY_MAX_BYTES, SyntheticTransferOutbound,
};
use kryptopass_protocol::proto;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};
use zeroize::Zeroizing;

/// First negotiated version that permits the phone → Desktop direction.
pub const INBOUND_KEY_TRANSFER_VERSION: u32 = 3;
/// Lifetime of one inbound operation from the offer, including the human
/// decision, GnuPG import and pinentry.
pub const INBOUND_KEY_TIMEOUT: Duration = Duration::from_secs(300);
/// Largest encrypted challenge accepted in an offer.
const MAX_ENCRYPTED_CHALLENGE_BYTES: usize = 16 * 1024;
/// Only the sender's vault identity may travel phone → Desktop.
const VAULT_IDENTITY_PURPOSE: u32 = 1;
const ABORT_EXPIRED: i32 = 2;
const ABORT_FAILED: i32 = 4;

/// Public, secret-free state of the inbound operation in one session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboundTransferPhase {
    /// A valid offer waits for the local user's decision.
    AwaitingDecision,
    /// The user accepted; ordered chunks are arriving.
    Receiving,
    /// Size and digest matched; the application validates, imports and decrypts.
    Importing,
    /// The receipt was sent after import and challenge decryption.
    Complete,
    /// The local user (or local policy) declined the offer.
    Rejected,
    /// The phone cancelled the operation.
    Cancelled,
    /// The operation exceeded its lifetime.
    Expired,
    /// A protocol, digest, validation, import or decryption step failed.
    Failed,
}

impl InboundTransferPhase {
    /// True while the operation still holds the session's single transfer slot.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::AwaitingDecision | Self::Receiving | Self::Importing
        )
    }
}

/// Bounded failure class for logs and the UI; never contains payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboundTransferFailure {
    /// A chunk or completion violated order, size or identity.
    Protocol,
    /// The assembled bytes did not match the offered SHA-256.
    Digest,
    /// Local validation, GnuPG import or challenge decryption failed.
    Local,
    /// The phone aborted with this wire code.
    Remote(i32),
    /// The session ended before a receipt was sent.
    Disconnected,
}

/// Observable metadata of the inbound operation. The fingerprint is public
/// key metadata; no key bytes, challenge or user IDs appear here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InboundTransferProgress {
    /// Current phase.
    pub phase: InboundTransferPhase,
    /// Identifier chosen by the phone.
    pub transfer_id: [u8; 16],
    /// Offered v4 primary-key fingerprint.
    pub fingerprint: [u8; 20],
    /// Offered size of the protected key block.
    pub total_size: u64,
    /// Bytes received so far.
    pub received: u64,
    /// Failure class once the phase is terminal and unsuccessful.
    pub failure: Option<InboundTransferFailure>,
}

/// The protected key block and challenge handed to the application for
/// import. Intentionally not `Debug`; the key is wiped on drop.
pub struct InboundKeyMaterial {
    transfer_id: [u8; 16],
    fingerprint: [u8; 20],
    key: Zeroizing<Vec<u8>>,
    encrypted_challenge: Vec<u8>,
}

impl InboundKeyMaterial {
    /// Identifier of the operation that produced these bytes.
    #[must_use]
    pub const fn transfer_id(&self) -> [u8; 16] {
        self.transfer_id
    }
    /// Offered v4 primary-key fingerprint.
    #[must_use]
    pub const fn fingerprint(&self) -> [u8; 20] {
        self.fingerprint
    }
    /// Passphrase-protected OpenPGP secret key packets, exactly as sent.
    #[must_use]
    pub fn protected_key(&self) -> &[u8] {
        self.key.as_slice()
    }
    /// OpenPGP message encrypting 32 random bytes to the offered key.
    #[must_use]
    pub fn encrypted_challenge(&self) -> &[u8] {
        &self.encrypted_challenge
    }
}

/// Shared one-slot handoff between the session task and the application.
pub(crate) type InboundMaterialSlot = Arc<Mutex<Option<InboundKeyMaterial>>>;

/// Why an offer was answered with an abort instead of opening an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OfferRefusal {
    /// The session did not negotiate version 3.
    Unsupported,
    /// Purpose, sizes, id or fingerprint were invalid.
    Invalid,
    /// Another operation (either direction) holds the slot.
    Busy,
}

struct ActiveInbound {
    transfer_id: [u8; 16],
    fingerprint: [u8; 20],
    total_size: usize,
    digest: [u8; 32],
    encrypted_challenge: Vec<u8>,
    assembly: Zeroizing<Vec<u8>>,
    next_chunk: u32,
    chunk_count: u32,
    deadline: Instant,
}

/// One-operation-at-a-time receiver. Session generation and trust are
/// enforced by its owner; a new operation may start after a terminal state.
pub(crate) struct SyntheticTransferReceiver {
    version: u32,
    active: Option<ActiveInbound>,
    progress: Option<InboundTransferProgress>,
    material: InboundMaterialSlot,
}

impl SyntheticTransferReceiver {
    pub(crate) fn new(version: u32, material: InboundMaterialSlot) -> Self {
        Self {
            version,
            active: None,
            progress: None,
            material,
        }
    }

    pub(crate) const fn progress(&self) -> Option<InboundTransferProgress> {
        self.progress
    }

    pub(crate) fn is_active(&self) -> bool {
        self.progress
            .is_some_and(|progress| progress.phase.is_active())
    }

    /// True for the active operation or the last terminal one with this id.
    pub(crate) fn knows(&self, transfer_id: &[u8]) -> bool {
        self.progress
            .is_some_and(|progress| progress.transfer_id.as_slice() == transfer_id)
    }

    /// Validates an offer. `Ok(())` opens an operation awaiting the user's
    /// decision; a refusal must be answered with `Abort{code 4}` by the owner.
    pub(crate) fn offer(
        &mut self,
        offer: &proto::SyntheticKeyOffer,
        other_direction_busy: bool,
        now: Instant,
    ) -> Result<(), OfferRefusal> {
        if self.version < INBOUND_KEY_TRANSFER_VERSION {
            tracing::warn!(
                version = self.version,
                "Inbound key transfer: offer refused, version 3 not negotiated"
            );
            return Err(OfferRefusal::Unsupported);
        }
        let reason = if offer.key_purpose != VAULT_IDENTITY_PURPOSE {
            Some("key_purpose is not the vault identity")
        } else if offer.transfer_id.len() != 16 || offer.transfer_id.iter().all(|byte| *byte == 0) {
            Some("transfer_id invalid")
        } else if offer.openpgp_fingerprint.len() != 20
            || offer.openpgp_fingerprint.iter().all(|byte| *byte == 0)
        {
            Some("fingerprint is not a v4 fingerprint")
        } else if offer.sha256_digest.len() != 32 {
            Some("digest length invalid")
        } else if offer.total_size == 0 || offer.total_size > SYNTHETIC_KEY_MAX_BYTES as u64 {
            Some("total_size out of range")
        } else if offer.encrypted_challenge.is_empty()
            || offer.encrypted_challenge.len() > MAX_ENCRYPTED_CHALLENGE_BYTES
        {
            Some("encrypted_challenge size out of range")
        } else {
            None
        };
        if let Some(reason) = reason {
            tracing::warn!(
                reason,
                key_purpose = offer.key_purpose,
                total_size = offer.total_size,
                challenge_bytes = offer.encrypted_challenge.len(),
                "Inbound key transfer: offer refused"
            );
            return Err(OfferRefusal::Invalid);
        }
        if other_direction_busy || self.is_active() {
            tracing::warn!(
                outbound_busy = other_direction_busy,
                inbound_busy = self.is_active(),
                "Inbound key transfer: offer refused, another operation is active in this session"
            );
            return Err(OfferRefusal::Busy);
        }
        let mut transfer_id = [0_u8; 16];
        transfer_id.copy_from_slice(&offer.transfer_id);
        let mut fingerprint = [0_u8; 20];
        fingerprint.copy_from_slice(&offer.openpgp_fingerprint);
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&offer.sha256_digest);
        let total_size = offer.total_size as usize;
        let chunk_count = total_size.div_ceil(SYNTHETIC_KEY_CHUNK_BYTES) as u32;
        self.clear_material();
        self.active = Some(ActiveInbound {
            transfer_id,
            fingerprint,
            total_size,
            digest,
            encrypted_challenge: offer.encrypted_challenge.clone(),
            assembly: Zeroizing::new(Vec::new()),
            next_chunk: 0,
            chunk_count,
            deadline: now + INBOUND_KEY_TIMEOUT,
        });
        self.progress = Some(InboundTransferProgress {
            phase: InboundTransferPhase::AwaitingDecision,
            transfer_id,
            fingerprint,
            total_size: offer.total_size,
            received: 0,
            failure: None,
        });
        tracing::info!(
            total_size,
            chunk_count,
            challenge_bytes = offer.encrypted_challenge.len(),
            "Inbound key transfer: offer accepted for review, awaiting local decision"
        );
        Ok(())
    }

    /// Applies the local user's (or policy's) decision for the exact operation.
    pub(crate) fn decide(
        &mut self,
        transfer_id: [u8; 16],
        accepted: bool,
        now: Instant,
    ) -> Option<SyntheticTransferOutbound> {
        if let Some(expired) = self.expire(now) {
            return Some(expired);
        }
        if self.phase() != Some(InboundTransferPhase::AwaitingDecision)
            || self.active.as_ref().map(|active| active.transfer_id) != Some(transfer_id)
        {
            tracing::warn!(
                phase = ?self.phase(),
                "Inbound key transfer: decision ignored, no matching operation awaits a decision"
            );
            return None;
        }
        let decision = proto::SyntheticKeyDecision {
            transfer_id: transfer_id.to_vec(),
            accepted,
        };
        if accepted {
            if let Some(active) = self.active.as_mut() {
                let mut assembly = Vec::new();
                if assembly.try_reserve_exact(active.total_size).is_err() {
                    tracing::warn!("Inbound key transfer: could not reserve the receive buffer");
                    return Some(self.fail(InboundTransferFailure::Local, ABORT_FAILED));
                }
                active.assembly = Zeroizing::new(assembly);
            }
            self.set_phase(InboundTransferPhase::Receiving);
            tracing::info!("Inbound key transfer: accepted locally, receiving");
        } else {
            self.finish(InboundTransferPhase::Rejected, None);
            tracing::info!("Inbound key transfer: rejected locally");
        }
        Some(SyntheticTransferOutbound::Decision(decision))
    }

    /// Appends one strictly sequential chunk. Returns an abort to send on failure.
    pub(crate) fn chunk(
        &mut self,
        chunk: &proto::SyntheticKeyChunk,
        now: Instant,
    ) -> Option<SyntheticTransferOutbound> {
        if !self.matches_active(&chunk.transfer_id) {
            return self.stray(&chunk.transfer_id, "chunk");
        }
        if let Some(expired) = self.expire(now) {
            return Some(expired);
        }
        if self.phase() != Some(InboundTransferPhase::Receiving) {
            tracing::warn!(phase = ?self.phase(), "Inbound key transfer: chunk outside the receiving phase");
            return Some(self.fail(InboundTransferFailure::Protocol, ABORT_FAILED));
        }
        let active = self.active.as_mut()?;
        let written = active.assembly.len();
        let remaining = active.total_size - written;
        let expected = remaining.min(SYNTHETIC_KEY_CHUNK_BYTES);
        let reason = if chunk.chunk_index != active.next_chunk {
            Some("chunk out of order")
        } else if chunk.chunk_count != active.chunk_count {
            Some("chunk_count mismatch")
        } else if chunk.total_size != active.total_size as u64 {
            Some("total_size mismatch")
        } else if chunk.sha256_digest.as_slice() != active.digest.as_slice() {
            Some("digest mismatch with the offer")
        } else if chunk.data.is_empty() || chunk.data.len() != expected {
            Some("chunk length unexpected")
        } else {
            None
        };
        if let Some(reason) = reason {
            tracing::warn!(
                reason,
                chunk_index = chunk.chunk_index,
                expected_index = active.next_chunk,
                chunk_count = chunk.chunk_count,
                expected_count = active.chunk_count,
                data_bytes = chunk.data.len(),
                expected_bytes = expected,
                "Inbound key transfer: chunk rejected"
            );
            return Some(self.fail(InboundTransferFailure::Protocol, ABORT_FAILED));
        }
        active.assembly.extend_from_slice(&chunk.data);
        active.next_chunk += 1;
        let received = active.assembly.len() as u64;
        if let Some(progress) = self.progress.as_mut() {
            progress.received = received;
        }
        None
    }

    /// Verifies size and digest, then hands the bytes to the application slot.
    pub(crate) fn complete(
        &mut self,
        message: &proto::SyntheticKeyComplete,
        now: Instant,
    ) -> Option<SyntheticTransferOutbound> {
        if !self.matches_active(&message.transfer_id) {
            return self.stray(&message.transfer_id, "complete");
        }
        if let Some(expired) = self.expire(now) {
            return Some(expired);
        }
        if self.phase() != Some(InboundTransferPhase::Receiving) {
            tracing::warn!(phase = ?self.phase(), "Inbound key transfer: complete outside the receiving phase");
            return Some(self.fail(InboundTransferFailure::Protocol, ABORT_FAILED));
        }
        let active = self.active.as_mut()?;
        if active.assembly.len() != active.total_size || active.next_chunk != active.chunk_count {
            tracing::warn!(
                received = active.assembly.len(),
                total_size = active.total_size,
                chunks = active.next_chunk,
                chunk_count = active.chunk_count,
                "Inbound key transfer: complete before every byte arrived"
            );
            return Some(self.fail(InboundTransferFailure::Protocol, ABORT_FAILED));
        }
        let actual: [u8; 32] = Sha256::digest(active.assembly.as_slice()).into();
        if actual != active.digest {
            tracing::warn!(
                total_size = active.total_size,
                "Inbound key transfer: SHA-256 of the assembled block does not match the offer"
            );
            return Some(self.fail(InboundTransferFailure::Digest, ABORT_FAILED));
        }
        let material = InboundKeyMaterial {
            transfer_id: active.transfer_id,
            fingerprint: active.fingerprint,
            key: std::mem::take(&mut active.assembly),
            encrypted_challenge: active.encrypted_challenge.clone(),
        };
        let stored = match self.material.lock() {
            Ok(mut slot) => {
                *slot = Some(material);
                true
            }
            Err(_) => false,
        };
        if !stored {
            tracing::warn!("Inbound key transfer: handoff slot unavailable");
            return Some(self.fail(InboundTransferFailure::Local, ABORT_FAILED));
        }
        self.set_phase(InboundTransferPhase::Importing);
        tracing::info!(
            total_size = self.progress.map_or(0, |progress| progress.total_size),
            "Inbound key transfer: block complete and digest verified, handing to import"
        );
        None
    }

    /// Ends the import stage with the application's result: a receipt on
    /// success, `Abort{code 4}` on failure.
    pub(crate) fn finish_import(
        &mut self,
        transfer_id: [u8; 16],
        challenge_sha256: Option<[u8; 32]>,
        now: Instant,
    ) -> Option<SyntheticTransferOutbound> {
        if self.phase() != Some(InboundTransferPhase::Importing)
            || self.active.as_ref().map(|active| active.transfer_id) != Some(transfer_id)
        {
            tracing::warn!(
                phase = ?self.phase(),
                "Inbound key transfer: import result ignored, operation is no longer importing"
            );
            return None;
        }
        if let Some(expired) = self.expire(now) {
            return Some(expired);
        }
        let Some(challenge_sha256) = challenge_sha256 else {
            tracing::warn!("Inbound key transfer: local import or challenge failed, aborting");
            return Some(self.fail(InboundTransferFailure::Local, ABORT_FAILED));
        };
        let fingerprint = self
            .active
            .as_ref()
            .map(|active| active.fingerprint)
            .unwrap_or_default();
        self.finish(InboundTransferPhase::Complete, None);
        tracing::info!("Inbound key transfer: import verified, sending receipt");
        Some(SyntheticTransferOutbound::Receipt(
            proto::SyntheticKeyReceipt {
                transfer_id: transfer_id.to_vec(),
                openpgp_fingerprint: fingerprint.to_vec(),
                challenge_sha256: challenge_sha256.to_vec(),
            },
        ))
    }

    /// Applies a peer abort for the operation this receiver owns.
    pub(crate) fn remote_abort(&mut self, abort: &proto::SyntheticKeyAbort) {
        if !self.matches_active(&abort.transfer_id) {
            tracing::info!(
                code = abort.code,
                "Inbound key transfer: abort for a finished operation ignored"
            );
            return;
        }
        let phase = match abort.code {
            1 => InboundTransferPhase::Cancelled,
            2 => InboundTransferPhase::Expired,
            3 => InboundTransferPhase::Rejected,
            _ => InboundTransferPhase::Failed,
        };
        tracing::warn!(code = abort.code, phase = ?self.phase(), "Inbound key transfer: phone aborted");
        self.finish(phase, Some(InboundTransferFailure::Remote(abort.code)));
    }

    /// Expires the whole operation, including the human decision and import.
    pub(crate) fn expire(&mut self, now: Instant) -> Option<SyntheticTransferOutbound> {
        let active = self.active.as_ref()?;
        if !self.is_active() || now < active.deadline {
            return None;
        }
        tracing::warn!(phase = ?self.phase(), "Inbound key transfer: expired");
        let transfer_id = active.transfer_id;
        self.finish(InboundTransferPhase::Expired, None);
        Some(SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
            transfer_id: transfer_id.to_vec(),
            code: ABORT_EXPIRED,
        }))
    }

    /// Wipes pending state on transport loss or session replacement.
    pub(crate) fn disconnected(&mut self) {
        if self.is_active() {
            tracing::warn!(phase = ?self.phase(), "Inbound key transfer: session ended before a receipt");
            self.finish(
                InboundTransferPhase::Failed,
                Some(InboundTransferFailure::Disconnected),
            );
        }
        self.clear_material();
    }

    fn phase(&self) -> Option<InboundTransferPhase> {
        self.progress.map(|progress| progress.phase)
    }

    fn matches_active(&self, transfer_id: &[u8]) -> bool {
        self.is_active()
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.transfer_id.as_slice() == transfer_id)
    }

    /// Late frames for the last finished operation are consumed silently; a
    /// frame for an unknown id is a transfer error answered with an abort.
    fn stray(&self, transfer_id: &[u8], kind: &'static str) -> Option<SyntheticTransferOutbound> {
        if self.knows(transfer_id) {
            tracing::info!(kind, phase = ?self.phase(), "Inbound key transfer: late frame for a finished operation ignored");
            return None;
        }
        tracing::warn!(
            kind,
            phase = ?self.phase(),
            "Inbound key transfer: frame for an unknown operation, answering abort"
        );
        (transfer_id.len() == 16).then(|| {
            SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
                transfer_id: transfer_id.to_vec(),
                code: ABORT_FAILED,
            })
        })
    }

    fn fail(&mut self, failure: InboundTransferFailure, code: i32) -> SyntheticTransferOutbound {
        let transfer_id = self
            .active
            .as_ref()
            .map(|active| active.transfer_id)
            .unwrap_or_default();
        self.finish(InboundTransferPhase::Failed, Some(failure));
        SyntheticTransferOutbound::Abort(proto::SyntheticKeyAbort {
            transfer_id: transfer_id.to_vec(),
            code,
        })
    }

    fn set_phase(&mut self, phase: InboundTransferPhase) {
        if let Some(progress) = self.progress.as_mut() {
            progress.phase = phase;
        }
    }

    fn finish(&mut self, phase: InboundTransferPhase, failure: Option<InboundTransferFailure>) {
        if let Some(progress) = self.progress.as_mut() {
            progress.phase = phase;
            progress.failure = failure;
        }
        // Dropping the assembly zeroizes the received bytes.
        self.active = None;
        self.clear_material();
    }

    fn clear_material(&self) {
        if let Ok(mut slot) = self.material.lock() {
            // InboundKeyMaterial wipes its key on drop.
            *slot = None;
        }
    }
}

impl Drop for SyntheticTransferReceiver {
    fn drop(&mut self) {
        self.active = None;
        self.clear_material();
    }
}
