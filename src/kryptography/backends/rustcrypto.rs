use aes::Aes256;
use aes::cipher::BlockDecryptMut;
use aes::cipher::BlockEncryptMut;
use aes::cipher::block_padding::Pkcs7;
use aes::cipher::generic_array::GenericArray;
use aes_gcm::Aes256Gcm;
use cbc::{Decryptor, Encryptor};
use chacha20poly1305::ChaCha20Poly1305;
use chacha20poly1305::Nonce as ChaChaNonce;
use ctr::Ctr128BE;
use ctr::cipher::{KeyIvInit, StreamCipher};

use aes_gcm::aead::{Aead, KeyInit, Payload};

use crate::kryptography::errors::{AeadError, BlockModeError, StreamError};

/// --- AEAD: AES-GCM (AES-256-GCM, nonce de 96 bits) ---
pub struct AesGcmEngine;

impl super::super::Aead for AesGcmEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-GCM"
    }

    fn encrypt(&self, key: &[u8], nonce: &[u8], pt: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("AES-GCM requiere key=32 y nonce=12"));
        }
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
        cipher
            .encrypt(
                GenericArray::from_slice(nonce),
                Payload {
                    msg: pt,
                    aad: aad.unwrap_or(&[]),
                },
            )
            .map_err(|e| AeadError::Backend(format!("encrypt: {e}")))
    }

    fn decrypt(&self, key: &[u8], nonce: &[u8], ct: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("AES-GCM requiere key=32 y nonce=12"));
        }
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key));
        cipher
            .decrypt(
                GenericArray::from_slice(nonce),
                Payload {
                    msg: ct,
                    aad: aad.unwrap_or(&[]),
                },
            )
            .map_err(|e| AeadError::Backend(format!("decrypt: {e}")))
    }
}

/// --- AEAD: ChaCha20-Poly1305 (key 256 bits, nonce 96 bits) ---
pub struct ChaCha20Poly1305Engine;

impl super::super::Aead for ChaCha20Poly1305Engine {
    fn algorithm_name(&self) -> &'static str {
        "ChaCha20-Poly1305"
    }

    fn encrypt(&self, key: &[u8], nonce: &[u8], pt: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("ChaCha20-Poly1305 requiere key=32 y nonce=12"));
        }
        let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|e| AeadError::Backend(format!("key: {e}")))?;
        cipher
            .encrypt(
                ChaChaNonce::from_slice(nonce),
                Payload {
                    msg: pt,
                    aad: aad.unwrap_or(&[]),
                },
            )
            .map_err(|e| AeadError::Backend(format!("encrypt: {e}")))
    }

    fn decrypt(&self, key: &[u8], nonce: &[u8], ct: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError> {
        if key.len() != 32 || nonce.len() != 12 {
            return Err(AeadError::Invalid("ChaCha20-Poly1305 requiere key=32 y nonce=12"));
        }
        let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|e| AeadError::Backend(format!("key: {e}")))?;
        cipher
            .decrypt(
                ChaChaNonce::from_slice(nonce),
                Payload {
                    msg: ct,
                    aad: aad.unwrap_or(&[]),
                },
            )
            .map_err(|e| AeadError::Backend(format!("decrypt: {e}")))
    }
}

/// --- Stream: AES-CTR (AES-256-CTR, IV de 128 bits) ---
pub struct AesCtrEngine;

impl super::super::Stream for AesCtrEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-CTR"
    }

    fn apply_keystream(&self, key: &[u8], nonce: &[u8], data: &[u8]) -> Result<Vec<u8>, StreamError> {
        if key.len() != 32 || nonce.len() != 16 {
            return Err(StreamError::Invalid("AES-CTR requiere key=32 e iv=16"));
        }
        type Aes256Ctr = Ctr128BE<Aes256>;
        let mut cipher =
            Aes256Ctr::new_from_slices(key, nonce).map_err(|e| StreamError::Backend(format!("init: {e}")))?;
        let mut out = data.to_vec();
        cipher.apply_keystream(&mut out);
        Ok(out)
    }
}

/// --- BlockMode: AES-CBC (AES-256-CBC con PKCS#7, IV 128 bits) ---
pub struct AesCbcEngine;

impl super::super::BlockMode for AesCbcEngine {
    fn algorithm_name(&self) -> &'static str {
        "AES-CBC"
    }

    fn encrypt(&self, key: &[u8], iv: &[u8], pt: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        if key.len() != 32 || iv.len() != 16 {
            return Err(BlockModeError::Invalid("AES-CBC requiere key=32 e iv=16"));
        }
        let enc =
            Encryptor::<Aes256>::new_from_slices(key, iv).map_err(|e| BlockModeError::Backend(format!("init: {e}")))?;

        // La API nueva usa encrypt_padded_mut::<Pkcs7>(&mut buf, msg_len)
        // Preparamos un buffer con espacio para el padding
        let bs = 16;
        let msg_len = pt.len();
        let pad = bs - (msg_len % bs);
        let mut buf = Vec::with_capacity(msg_len + pad);
        buf.extend_from_slice(pt);
        buf.resize(msg_len + pad, 0);

        let out = enc
            .encrypt_padded_mut::<Pkcs7>(&mut buf, msg_len)
            .map_err(|e| BlockModeError::Backend(format!("encrypt: {e}")))?;

        Ok(out.to_vec())
    }

    fn decrypt(&self, key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        if key.len() != 32 || iv.len() != 16 {
            return Err(BlockModeError::Invalid("AES-CBC requiere key=32 e iv=16"));
        }
        let dec =
            Decryptor::<Aes256>::new_from_slices(key, iv).map_err(|e| BlockModeError::Backend(format!("init: {e}")))?;

        // La API nueva usa decrypt_padded_mut::<Pkcs7>(&mut buf)
        let mut buf = ct.to_vec();
        let out = dec
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|e| BlockModeError::Backend(format!("decrypt: {e}")))?;

        Ok(out.to_vec())
    }
}
