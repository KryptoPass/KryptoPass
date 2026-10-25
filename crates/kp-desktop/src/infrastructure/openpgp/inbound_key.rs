//! Import of the vault identity received from a paired phone (key transfer v3).
//!
//! The received block is validated offline first: exactly one v4 secret
//! primary key with the offered fingerprint, every secret part protected by a
//! passphrase. Only then is it merged into the user's GnuPG keyring with a
//! normal import (never deletes or replaces material), and the offer's
//! challenge is decrypted to prove the imported key works. Errors carry static
//! descriptions only; no key bytes, user IDs or plaintext leave this module.

use super::{BoundedWriter, open_context};
use gpgme::Data;
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// Bytes of the random challenge the phone encrypts to its own key.
const CHALLENGE_BYTES: usize = 32;

/// Packet tags accepted in a transferable secret key.
const TAG_SIGNATURE: u8 = 2;
const TAG_SECRET_KEY: u8 = 5;
const TAG_SECRET_SUBKEY: u8 = 7;
const TAG_TRUST: u8 = 12;
const TAG_USER_ID: u8 = 13;
const TAG_USER_ATTRIBUTE: u8 = 17;

/// What GnuPG already holds for the offered fingerprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExistingKey {
    /// Not in the keyring: the import creates it.
    Absent,
    /// Only the public key: the import adds the secret parts.
    PublicOnly,
    /// Public and secret parts: the import merges new material.
    WithSecret,
}

/// Bounded failure of one stage; safe to log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InboundKeyError {
    /// `validate`, `import` or `verify`.
    pub(crate) stage: &'static str,
    /// Static, secret-free description of the failure class.
    pub(crate) reason: &'static str,
}

impl InboundKeyError {
    const fn validate(reason: &'static str) -> Self {
        Self {
            stage: "validate",
            reason,
        }
    }
    const fn import(reason: &'static str) -> Self {
        Self {
            stage: "import",
            reason,
        }
    }
    const fn verify(reason: &'static str) -> Self {
        Self {
            stage: "verify",
            reason,
        }
    }
}

/// Looks up the offered fingerprint without prompting.
pub(crate) fn existing_key(fingerprint_hex: &str) -> anyhow::Result<ExistingKey> {
    let mut context = open_context()?;
    if let Ok(key) = context.get_secret_key(fingerprint_hex) {
        if key
            .fingerprint()
            .is_ok_and(|stored| stored.eq_ignore_ascii_case(fingerprint_hex))
        {
            return Ok(ExistingKey::WithSecret);
        }
    }
    if let Ok(key) = context.get_key(fingerprint_hex) {
        if key
            .fingerprint()
            .is_ok_and(|stored| stored.eq_ignore_ascii_case(fingerprint_hex))
        {
            return Ok(ExistingKey::PublicOnly);
        }
    }
    Ok(ExistingKey::Absent)
}

/// Checks the received packets without touching GnuPG.
pub(crate) fn validate_protected_secret_key(
    bytes: &[u8],
    fingerprint: &[u8; 20],
) -> Result<(), InboundKeyError> {
    let mut offset = 0;
    let mut primary_seen = false;
    let mut subkeys = 0_usize;
    let mut user_ids = 0_usize;
    while offset < bytes.len() {
        let start = offset;
        let (tag, body, next) = next_packet(bytes, offset).map_err(InboundKeyError::validate)?;
        offset = next;
        match tag {
            TAG_SECRET_KEY => {
                if primary_seen {
                    return Err(InboundKeyError::validate(
                        "more than one secret primary key in the block",
                    ));
                }
                if start != 0 {
                    return Err(InboundKeyError::validate(
                        "the secret primary key does not lead the block",
                    ));
                }
                primary_seen = true;
                let public_len = protected_secret_key(body).map_err(InboundKeyError::validate)?;
                let mut hasher = Sha1::new();
                hasher.update([0x99]);
                hasher.update(
                    u16::try_from(public_len)
                        .map_err(|_| InboundKeyError::validate("public key part too large"))?
                        .to_be_bytes(),
                );
                hasher.update(&body[..public_len]);
                let actual: [u8; 20] = hasher.finalize().into();
                if &actual != fingerprint {
                    return Err(InboundKeyError::validate(
                        "primary fingerprint differs from the offer",
                    ));
                }
            }
            TAG_SECRET_SUBKEY => {
                if !primary_seen {
                    return Err(InboundKeyError::validate("subkey before the primary key"));
                }
                protected_secret_key(body).map_err(InboundKeyError::validate)?;
                subkeys += 1;
            }
            TAG_USER_ID | TAG_USER_ATTRIBUTE => {
                if !primary_seen {
                    return Err(InboundKeyError::validate("user ID before the primary key"));
                }
                user_ids += 1;
            }
            TAG_SIGNATURE | TAG_TRUST => {
                if !primary_seen {
                    return Err(InboundKeyError::validate(
                        "signature before the primary key",
                    ));
                }
            }
            _ => {
                tracing::warn!(tag, "Inbound key transfer: unexpected OpenPGP packet tag");
                return Err(InboundKeyError::validate(
                    "unexpected packet: not exactly one transferable secret key",
                ));
            }
        }
    }
    if !primary_seen {
        return Err(InboundKeyError::validate(
            "no secret primary key in the block",
        ));
    }
    tracing::info!(
        subkeys,
        user_ids,
        size = bytes.len(),
        "Inbound key transfer: block is one protected v4 secret key with the offered fingerprint"
    );
    Ok(())
}

/// Merges the validated block into GnuPG. A normal import creates the key or
/// adds new user IDs, subkeys and signatures; it never deletes material.
pub(crate) fn import_protected_secret_key(
    bytes: &[u8],
    fingerprint_hex: &str,
) -> Result<(), InboundKeyError> {
    let mut context =
        open_context().map_err(|_| InboundKeyError::import("could not open GnuPG"))?;
    let result = context
        .import(bytes)
        .map_err(|_| InboundKeyError::import("GnuPG rejected the import"))?;
    tracing::info!(
        considered = result.considered(),
        imported = result.imported(),
        unchanged = result.unchanged(),
        new_user_ids = result.new_user_ids(),
        new_subkeys = result.new_subkeys(),
        new_signatures = result.new_signatures(),
        secret_considered = result.secret_considered(),
        secret_imported = result.secret_imported(),
        secret_unchanged = result.secret_unchanged(),
        not_imported = result.not_imported(),
        "Inbound key transfer: GnuPG import result"
    );
    for import in result.imports() {
        let matches = import
            .fingerprint()
            .is_ok_and(|stored| stored.eq_ignore_ascii_case(fingerprint_hex));
        if !matches {
            return Err(InboundKeyError::import(
                "GnuPG reported a key other than the offered one",
            ));
        }
        if import.result().is_err() {
            return Err(InboundKeyError::import("GnuPG could not import a key part"));
        }
    }
    if result.secret_considered() == 0
        || result.secret_imported() + result.secret_unchanged() == 0
        || result.not_imported() != 0
    {
        return Err(InboundKeyError::import(
            "GnuPG did not store the secret key",
        ));
    }
    let key = context
        .get_secret_key(fingerprint_hex)
        .map_err(|_| InboundKeyError::import("secret key missing after import"))?;
    if !key
        .fingerprint()
        .is_ok_and(|stored| stored.eq_ignore_ascii_case(fingerprint_hex))
    {
        return Err(InboundKeyError::import("imported key fingerprint mismatch"));
    }
    Ok(())
}

/// Decrypts the offer's challenge with GnuPG (pinentry may ask for the
/// passphrase) and returns SHA-256 of its 32-byte plaintext, only if a
/// subkey of the offered fingerprint decrypted it.
pub(crate) fn decrypt_challenge(
    encrypted_challenge: &[u8],
    fingerprint_hex: &str,
) -> Result<[u8; 32], InboundKeyError> {
    let mut context =
        open_context().map_err(|_| InboundKeyError::verify("could not open GnuPG"))?;
    let key = context
        .get_secret_key(fingerprint_hex)
        .map_err(|_| InboundKeyError::verify("secret key missing before decryption"))?;
    let subkey_ids: Vec<String> = key
        .subkeys()
        .filter_map(|subkey| subkey.id().ok().map(str::to_ascii_uppercase))
        .collect();
    let mut plaintext = BoundedWriter::new(CHALLENGE_BYTES);
    let result = {
        let mut output = Data::from_writer(&mut plaintext)
            .map_err(|_| InboundKeyError::verify("could not prepare decryption output"))?;
        context
            .decrypt(encrypted_challenge, &mut output)
            .map_err(|_| InboundKeyError::verify("GnuPG could not decrypt the challenge"))?
    };
    if result.is_legacy_cipher_no_mdc() {
        return Err(InboundKeyError::verify(
            "challenge uses legacy encryption without integrity",
        ));
    }
    let mut used_offered = false;
    for recipient in result.recipients() {
        let id = recipient.key_id().unwrap_or_default().to_ascii_uppercase();
        let belongs = subkey_ids.contains(&id);
        if recipient.status().is_ok() {
            if belongs {
                used_offered = true;
            } else {
                return Err(InboundKeyError::verify(
                    "another local key could decrypt the challenge",
                ));
            }
        }
    }
    if !used_offered {
        return Err(InboundKeyError::verify(
            "the challenge was not decrypted by the offered key",
        ));
    }
    if plaintext.bytes.len() != CHALLENGE_BYTES {
        return Err(InboundKeyError::verify(
            "challenge plaintext has the wrong size",
        ));
    }
    // BoundedWriter wipes the plaintext when dropped.
    Ok(Sha256::digest(&plaintext.bytes).into())
}

/// Returns `(tag, body, next_offset)` for one definite-length packet.
fn next_packet(bytes: &[u8], offset: usize) -> Result<(u8, &[u8], usize), &'static str> {
    let header = *bytes.get(offset).ok_or("truncated packet header")?;
    if header & 0x80 == 0 {
        return Err("invalid packet header");
    }
    let mut position = offset + 1;
    let byte = |position: usize| {
        bytes
            .get(position)
            .copied()
            .ok_or("truncated packet length")
    };
    let (tag, length) = if header & 0x40 != 0 {
        let tag = header & 0x3f;
        let first = byte(position)?;
        position += 1;
        let length = match first {
            0..=191 => usize::from(first),
            192..=223 => {
                let second = byte(position)?;
                position += 1;
                ((usize::from(first) - 192) << 8) + usize::from(second) + 192
            }
            255 => {
                let mut value = 0_usize;
                for _ in 0..4 {
                    value = (value << 8) | usize::from(byte(position)?);
                    position += 1;
                }
                value
            }
            _ => return Err("partial body length is not allowed in a key"),
        };
        (tag, length)
    } else {
        let tag = (header >> 2) & 0x0f;
        let size = match header & 0x03 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => return Err("indeterminate packet length is not allowed"),
        };
        let mut value = 0_usize;
        for _ in 0..size {
            value = (value << 8) | usize::from(byte(position)?);
            position += 1;
        }
        (tag, value)
    };
    let end = position
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or("packet extends past the block")?;
    Ok((tag, &bytes[position..end], end))
}

/// Parses a v4 secret (sub)key body, requires passphrase protection, and
/// returns the length of its public part (for the fingerprint).
fn protected_secret_key(body: &[u8]) -> Result<usize, &'static str> {
    if body.first() != Some(&4) {
        return Err("only v4 secret keys are accepted");
    }
    let algorithm = *body.get(5).ok_or("truncated key packet")?;
    let mut position = 6;
    let mpi = |position: &mut usize| -> Result<(), &'static str> {
        let high = *body.get(*position).ok_or("truncated key material")?;
        let low = *body.get(*position + 1).ok_or("truncated key material")?;
        let bits = (usize::from(high) << 8) | usize::from(low);
        *position += 2 + bits.div_ceil(8);
        if *position > body.len() {
            return Err("truncated key material");
        }
        Ok(())
    };
    let length_prefixed = |position: &mut usize| -> Result<(), &'static str> {
        let length = *body.get(*position).ok_or("truncated key material")?;
        *position += 1 + usize::from(length);
        if *position > body.len() {
            return Err("truncated key material");
        }
        Ok(())
    };
    let fixed = |position: &mut usize, length: usize| -> Result<(), &'static str> {
        *position += length;
        if *position > body.len() {
            return Err("truncated key material");
        }
        Ok(())
    };
    match algorithm {
        // RSA
        1..=3 => {
            mpi(&mut position)?;
            mpi(&mut position)?;
        }
        // Elgamal
        16 | 20 => {
            for _ in 0..3 {
                mpi(&mut position)?;
            }
        }
        // DSA
        17 => {
            for _ in 0..4 {
                mpi(&mut position)?;
            }
        }
        // ECDH: curve OID, point, KDF parameters
        18 => {
            length_prefixed(&mut position)?;
            mpi(&mut position)?;
            length_prefixed(&mut position)?;
        }
        // ECDSA, legacy EdDSA: curve OID, point
        19 | 22 => {
            length_prefixed(&mut position)?;
            mpi(&mut position)?;
        }
        25 => fixed(&mut position, 32)?,
        26 => fixed(&mut position, 56)?,
        27 => fixed(&mut position, 32)?,
        28 => fixed(&mut position, 57)?,
        _ => return Err("unsupported public-key algorithm"),
    }
    let public_len = position;
    let usage = *body.get(position).ok_or("secret key part missing")?;
    match usage {
        0 => return Err("a secret part is not protected by a passphrase"),
        254 | 255 => {
            // Symmetric algorithm, then the S2K specifier type; 101 is GnuPG's
            // dummy/stub extension, which carries no secret material.
            let s2k_type = *body
                .get(position + 2)
                .ok_or("truncated protection header")?;
            if s2k_type == 101 {
                return Err("secret part is a GnuPG stub without key material");
            }
        }
        _ => {}
    }
    Ok(public_len)
}
