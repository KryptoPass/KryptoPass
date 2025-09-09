//! backends/symmetric/openssl_engines.rs
#![cfg(feature = "backend-openssl")]

use openssl::symm::{Cipher, Crypter, Mode};

use crate::kryptography::errors::{AeadError, BlockModeError, StreamError};

const TAG_LEN: usize = 16; // GCM/ChaCha20-Poly1305 usan tag de 128 bits

// =====================
// AEAD: AES-256-GCM
// =====================
pub struct AesGcmEngine;

impl super::super::Aead for AesGcmEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-GCM"
    }

    fn encrypt(
        &self,
        key: &[u8],
        nonce: &[u8], // IV
        pt: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("AES-GCM requiere key=32 y nonce=12"));
        }

        let cipher = Cipher::aes_256_gcm();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(nonce))
            .map_err(|e| AeadError::Backend(format!("Crypter::new: {e}")))?;

        // AAD (opcional)
        if let Some(a) = aad {
            crypter
                .aad_update(a)
                .map_err(|e| AeadError::Backend(format!("aad_update: {e}")))?;
        }

        // ciphertext
        let mut out = vec![0u8; pt.len() + cipher.block_size()];
        let mut count = crypter
            .update(pt, &mut out)
            .map_err(|e| AeadError::Backend(format!("update: {e}")))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| AeadError::Backend(format!("finalize: {e}")))?;
        out.truncate(count);

        // GCM tag
        let mut tag = [0u8; TAG_LEN];
        crypter
            .get_tag(&mut tag)
            .map_err(|e| AeadError::Backend(format!("get_tag: {e}")))?;

        // devolvemos ct || tag (mismo contrato que Aes256Gcm de RustCrypto)
        out.extend_from_slice(&tag);
        Ok(out)
    }

    fn decrypt(&self, key: &[u8], nonce: &[u8], ct_and_tag: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("AES-GCM requiere key=32 y nonce=12"));
        }
        if ct_and_tag.len() < TAG_LEN {
            return Err(AeadError::Invalid("ciphertext demasiado corto (falta tag)"));
        }

        let (ct, tag) = ct_and_tag.split_at(ct_and_tag.len() - TAG_LEN);

        let cipher = Cipher::aes_256_gcm();
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, key, Some(nonce))
            .map_err(|e| AeadError::Backend(format!("Crypter::new: {e}")))?;

        // AAD
        if let Some(a) = aad {
            crypter
                .aad_update(a)
                .map_err(|e| AeadError::Backend(format!("aad_update: {e}")))?;
        }

        // setear tag ANTES del finalize
        crypter
            .set_tag(tag)
            .map_err(|e| AeadError::Backend(format!("set_tag: {e}")))?;

        let mut out = vec![0u8; ct.len() + cipher.block_size()];
        let mut count = crypter
            .update(ct, &mut out)
            .map_err(|e| AeadError::Backend(format!("update: {e}")))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| AeadError::Backend(format!("finalize (tag inválido?): {e}")))?;
        out.truncate(count);
        Ok(out)
    }
}

// ==========================================
// AEAD: ChaCha20-Poly1305 (IETF: nonce 12)
// ==========================================
pub struct ChaCha20Poly1305Engine;

impl super::super::Aead for ChaCha20Poly1305Engine {
    fn algorithm_name(&self) -> &'static str {
        "ChaCha20-Poly1305"
    }

    fn encrypt(&self, key: &[u8], nonce: &[u8], pt: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("ChaCha20-Poly1305 requiere key=32 y nonce=12"));
        }

        let cipher = Cipher::chacha20_poly1305();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(nonce))
            .map_err(|e| AeadError::Backend(format!("Crypter::new: {e}")))?;

        if let Some(a) = aad {
            crypter
                .aad_update(a)
                .map_err(|e| AeadError::Backend(format!("aad_update: {e}")))?;
        }

        let mut out = vec![0u8; pt.len() + cipher.block_size()];
        let mut count = crypter
            .update(pt, &mut out)
            .map_err(|e| AeadError::Backend(format!("update: {e}")))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| AeadError::Backend(format!("finalize: {e}")))?;
        out.truncate(count);

        let mut tag = [0u8; TAG_LEN];
        crypter
            .get_tag(&mut tag)
            .map_err(|e| AeadError::Backend(format!("get_tag: {e}")))?;
        out.extend_from_slice(&tag);
        Ok(out)
    }

    fn decrypt(&self, key: &[u8], nonce: &[u8], ct_and_tag: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("ChaCha20-Poly1305 requiere key=32 y nonce=12"));
        }
        if ct_and_tag.len() < TAG_LEN {
            return Err(AeadError::Invalid("ciphertext demasiado corto (falta tag)"));
        }
        let (ct, tag) = ct_and_tag.split_at(ct_and_tag.len() - TAG_LEN);

        let cipher = Cipher::chacha20_poly1305();
        let mut crypter = Crypter::new(cipher, Mode::Decrypt, key, Some(nonce))
            .map_err(|e| AeadError::Backend(format!("Crypter::new: {e}")))?;

        if let Some(a) = aad {
            crypter
                .aad_update(a)
                .map_err(|e| AeadError::Backend(format!("aad_update: {e}")))?;
        }

        crypter
            .set_tag(tag)
            .map_err(|e| AeadError::Backend(format!("set_tag: {e}")))?;

        let mut out = vec![0u8; ct.len() + cipher.block_size()];
        let mut count = crypter
            .update(ct, &mut out)
            .map_err(|e| AeadError::Backend(format!("update: {e}")))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| AeadError::Backend(format!("finalize (tag inválido?): {e}")))?;
        out.truncate(count);
        Ok(out)
    }
}

// =====================
// Stream: AES-256-CTR
// =====================
pub struct AesCtrEngine;

impl super::super::Stream for AesCtrEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-CTR"
    }

    fn apply_keystream(&self, key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, StreamError> {
        if key.len() != 32 || iv.len() != 16 {
            return Err(StreamError::Invalid("AES-CTR requiere key=32 e iv=16"));
        }
        let cipher = Cipher::aes_256_ctr();
        let mut crypter = Crypter::new(cipher, Mode::Encrypt, key, Some(iv))
            .map_err(|e| StreamError::Backend(format!("Crypter::new: {e}")))?;

        let mut out = vec![0u8; data.len() + cipher.block_size()];
        let mut count = crypter
            .update(data, &mut out)
            .map_err(|e| StreamError::Backend(format!("update: {e}")))?;
        count += crypter
            .finalize(&mut out[count..])
            .map_err(|e| StreamError::Backend(format!("finalize: {e}")))?;
        out.truncate(count);
        Ok(out)
    }
}

// =======================================
// BlockMode: AES-256-CBC + PKCS#7
// =======================================
pub struct AesCbcEngine;

impl super::super::BlockMode for AesCbcEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-CBC"
    }

    fn encrypt(&self, key: &[u8], iv: &[u8], pt: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        if key.len() != 32 || iv.len() != 16 {
            return Err(BlockModeError::Invalid("AES-CBC requiere key=32 e iv=16"));
        }
        openssl::symm::encrypt(Cipher::aes_256_cbc(), key, Some(iv), pt)
            .map_err(|e| BlockModeError::Backend(format!("encrypt: {e}")))
    }

    fn decrypt(&self, key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        if key.len() != 32 || iv.len() != 16 {
            return Err(BlockModeError::Invalid("AES-CBC requiere key=32 e iv=16"));
        }
        openssl::symm::decrypt(Cipher::aes_256_cbc(), key, Some(iv), ct)
            .map_err(|e| BlockModeError::Backend(format!("decrypt: {e}")))
    }
}
