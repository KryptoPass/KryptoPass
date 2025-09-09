pub mod aead;
pub mod backends;
pub mod block;
pub mod enums;
pub mod errors;
pub mod stream;
pub mod support;
pub mod traits;
pub mod utils;

use crate::kryptography::{
    aead::{AeadAlg, ParseAeadAlgError},
    block::{BlockModeAlg, ParseBlockAlgError},
    errors::{AeadError, BlockModeError, FactoryError, StreamError},
    stream::{ParseStreamAlgError, StreamAlg},
    traits::{Aead, BlockMode, Stream},
    utils::{Iv16, Key32, Nonce12},
};

// --- Enforce: exactamente 1 backend activo ---
#[cfg(all(feature = "backend-rustcrypto", feature = "backend-openssl"))]
compile_error!("Habilita exactamente UN backend: 'backend-rustcrypto' O 'backend-openssl'.");
#[cfg(not(any(feature = "backend-rustcrypto", feature = "backend-openssl")))]
compile_error!("Debes habilitar un backend: 'backend-rustcrypto' o 'backend-openssl'.");

#[cfg(feature = "backend-rustcrypto")]
pub const BACKEND_NAME: &str = "RustCrypto";
#[cfg(feature = "backend-openssl")]
pub const BACKEND_NAME: &str = "OpenSSL";

pub struct CryptoFactory;
impl CryptoFactory {
    // --- AEAD ---
    pub fn try_aead(alg: AeadAlg) -> Result<Box<dyn Aead>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                AeadAlg::AesGcm => Ok(Box::new(backends::rustcrypto::AesGcmEngine)),
                AeadAlg::ChaCha20Poly1305 => Ok(Box::new(backends::rustcrypto::ChaCha20Poly1305Engine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                AeadAlg::AesGcm => Ok(Box::new(backends::openssl::AesGcmEngine)),
                AeadAlg::ChaCha20Poly1305 => Err(FactoryError::NotImplemented {
                    algo: alg.as_str(),
                    backend: BACKEND_NAME,
                }),
            }
        }
    }
    pub fn try_aead_from_str(name: &str) -> Result<Box<dyn Aead>, FactoryError> {
        let alg: AeadAlg = name.parse().map_err(|e: ParseAeadAlgError| FactoryError::Parse(e.0))?;
        Self::try_aead(alg)
    }

    // --- Stream ---
    pub fn try_stream(alg: StreamAlg) -> Result<Box<dyn Stream>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                StreamAlg::AesCtr => Ok(Box::new(backends::rustcrypto::AesCtrEngine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                _ => Err(FactoryError::NotImplemented {
                    algo: alg.as_str(),
                    backend: BACKEND_NAME,
                }),
            }
        }
    }
    pub fn try_stream_from_str(name: &str) -> Result<Box<dyn Stream>, FactoryError> {
        let alg: StreamAlg = name
            .parse()
            .map_err(|e: ParseStreamAlgError| FactoryError::Parse(e.0))?;
        Self::try_stream(alg)
    }

    // --- BlockMode ---
    pub fn try_block_mode(alg: BlockModeAlg) -> Result<Box<dyn BlockMode>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                BlockModeAlg::AesCbc => Ok(Box::new(backends::rustcrypto::AesCbcEngine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                _ => Err(FactoryError::NotImplemented {
                    algo: alg.as_str(),
                    backend: BACKEND_NAME,
                }),
            }
        }
    }
    pub fn try_block_mode_from_str(name: &str) -> Result<Box<dyn BlockMode>, FactoryError> {
        let alg: BlockModeAlg = name.parse().map_err(|e: ParseBlockAlgError| FactoryError::Parse(e.0))?;
        Self::try_block_mode(alg)
    }
}

/// Cifra AEAD usando tipos fuertes (Key32/Nonce12). Internamente llama al trait con slices.
pub fn aead_encrypt_checked(
    engine: &dyn Aead,
    key: &Key32,
    nonce: &Nonce12,
    pt: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    engine.encrypt(key.as_ref(), nonce.as_ref(), pt, aad)
}
/// Descifra AEAD usando tipos fuertes.
pub fn aead_decrypt_checked(
    engine: &dyn Aead,
    key: &Key32,
    nonce: &Nonce12,
    ct: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, AeadError> {
    engine.decrypt(key.as_ref(), nonce.as_ref(), ct, aad)
}
/// Aplica keystream con tipos fuertes (AES-CTR: Key32/Iv16)
pub fn stream_apply_checked(engine: &dyn Stream, key: &Key32, iv: &Iv16, data: &[u8]) -> Result<Vec<u8>, StreamError> {
    engine.apply_keystream(key.as_ref(), iv.as_ref(), data)
}
/// Cifra/Descifra CBC con tipos fuertes (AES-CBC: Key32/Iv16)
pub fn cbc_encrypt_checked(
    engine: &dyn BlockMode,
    key: &Key32,
    iv: &Iv16,
    pt: &[u8],
) -> Result<Vec<u8>, BlockModeError> {
    engine.encrypt(key.as_ref(), iv.as_ref(), pt)
}
pub fn cbc_decrypt_checked(
    engine: &dyn BlockMode,
    key: &Key32,
    iv: &Iv16,
    ct: &[u8],
) -> Result<Vec<u8>, BlockModeError> {
    engine.decrypt(key.as_ref(), iv.as_ref(), ct)
}
