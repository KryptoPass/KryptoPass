use crate::kryptography::backends::symmetric::{algorithms::BlockCipherAlgorithm, modes::Mode};

pub mod algorithms;
pub mod errors;
pub mod modes;
pub mod rustcrypto;

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("algoritmo o modo no soportado por este backend: {0}")]
    Unsupported(&'static str),
    #[error("fallo criptográfico: {0}")]
    Crypto(&'static str),
}

pub trait SymmetricBackend {
    fn encrypt(
        &self,
        alg: &dyn BlockCipherAlgorithm,
        mode: &dyn Mode,
        key: &[u8],
        aad: Option<&[u8]>, // para AEAD (GCM); ignorar en CBC
        plaintext: &[u8],
    ) -> Result<Vec<u8>, BackendError>;

    fn decrypt(
        &self,
        alg: &dyn BlockCipherAlgorithm,
        mode: &dyn Mode,
        key: &[u8],
        aad: Option<&[u8]>, // para AEAD (GCM); ignorar en CBC
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, BackendError>;
}

pub struct CipherContext<'a> {
    pub alg: &'a dyn BlockCipherAlgorithm,
    pub mode: &'a dyn Mode,
    pub key: &'a [u8],
}

impl<'a> CipherContext<'a> {
    pub fn new(
        alg: &'a dyn BlockCipherAlgorithm,
        mode: &'a dyn Mode,
        key: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        alg.verify_key_size(key)?;
        mode.validate_for_algorithm(alg)?;
        Ok(Self { alg, mode, key })
    }

    pub fn encrypt<B: SymmetricBackend>(
        &self,
        be: &B,
        aad: Option<&[u8]>,
        pt: &[u8],
    ) -> Result<Vec<u8>, BackendError> {
        be.encrypt(self.alg, self.mode, self.key, aad, pt)
    }

    pub fn decrypt<B: SymmetricBackend>(
        &self,
        be: &B,
        aad: Option<&[u8]>,
        ct: &[u8],
    ) -> Result<Vec<u8>, BackendError> {
        be.decrypt(self.alg, self.mode, self.key, aad, ct)
    }
}
