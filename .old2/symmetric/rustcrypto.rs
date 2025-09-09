use crate::kryptography::backends::symmetric::{
    BackendError, SymmetricBackend, algorithms::BlockCipherAlgorithm, modes::Mode,
};

pub struct RustCryptoBackend;

impl SymmetricBackend for RustCryptoBackend {
    fn encrypt(
        &self,
        alg: &dyn BlockCipherAlgorithm,
        mode: &dyn Mode,
        key: &[u8],
        aad: Option<&[u8]>,
        pt: &[u8],
    ) -> Result<Vec<u8>, BackendError> {
        match (alg.name(), mode.name()) {
            // ---------- AES-CBC (PKCS7) ----------
            ("AES", "CBC") => {
                unimplemented!()
            }

            // ---------- AES-GCM (AEAD) ----------
            ("AES", "GCM") => {
                unimplemented!()
            }

            _ => Err(BackendError::Unsupported("alg/mode")),
        }
    }

    fn decrypt(
        &self,
        alg: &dyn BlockCipherAlgorithm,
        mode: &dyn Mode,
        key: &[u8],
        aad: Option<&[u8]>,
        ct: &[u8],
    ) -> Result<Vec<u8>, BackendError> {
        match (alg.name(), mode.name()) {
            // ---------- AES-CBC (PKCS7) ----------
            ("AES", "CBC") => {
                unimplemented!()
            }

            // ---------- AES-GCM (AEAD) ----------
            ("AES", "GCM") => {
                unimplemented!()
            }

            _ => Err(BackendError::Unsupported("alg/mode")),
        }
    }
}
