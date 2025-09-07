use std::fmt;

use crate::kryptography::backends::symmetric::errors::AlgorithmError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherKind {
    Aes,
    Other(&'static str),
}

pub trait CipherAlgorithm: fmt::Debug {
    fn name(&self) -> &'static str;
    fn key_size_bits(&self) -> usize;
    fn block_size_bits(&self) -> Option<usize>; // None => no es de bloque (p.ej. ChaCha20)
    fn kind(&self) -> CipherKind;
}

// Helper genérico para validar tamaños de clave (en bits)
fn verify_key_size(key: &[u8], allowed: &'static [usize]) -> Result<(), AlgorithmError> {
    if key.is_empty() {
        return Err(AlgorithmError::EmptyKey);
    }
    let bits = key.len() * 8;
    if allowed.contains(&bits) {
        Ok(())
    } else {
        Err(AlgorithmError::InvalidKeySize {
            allowed,
            got_bits: bits,
        })
    }
}

// ================================ AES general ====================================

#[derive(Debug, Clone)]
pub struct AES {
    key: Vec<u8>,
}
impl AES {
    /// Acepta 128, 192, 256 y 512 bits (512 se usa para AES-256-XTS)
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[128, 192, 256, 512])?;
        Ok(Self { key: key.to_vec() })
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }
}
impl CipherAlgorithm for AES {
    fn name(&self) -> &'static str {
        "AES"
    }
    fn key_size_bits(&self) -> usize {
        self.key.len() * 8
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
    fn kind(&self) -> CipherKind {
        CipherKind::Aes
    }
}

// ================================ AES128 wrapper =================================

#[derive(Debug, Clone)]
pub struct AES128 {
    key: Vec<u8>,
}
impl AES128 {
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[128])?;
        Ok(Self { key: key.to_vec() })
    }
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}
impl CipherAlgorithm for AES128 {
    fn name(&self) -> &'static str {
        "AES"
    }
    fn key_size_bits(&self) -> usize {
        128
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
    fn kind(&self) -> CipherKind {
        CipherKind::Aes
    }
}

// ================================ AES256 wrapper =================================

#[derive(Debug, Clone)]
pub struct AES256 {
    key: Vec<u8>,
}
impl AES256 {
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[256])?;
        Ok(Self { key: key.to_vec() })
    }
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}
impl CipherAlgorithm for AES256 {
    fn name(&self) -> &'static str {
        "AES"
    }
    fn key_size_bits(&self) -> usize {
        256
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
    fn kind(&self) -> CipherKind {
        CipherKind::Aes
    }
}

// ================================= Camellia ======================================

#[derive(Debug, Clone)]
pub struct Camellia {
    key: Vec<u8>,
}
impl Camellia {
    /// Acepta 128, 192, 256 bits.
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[128, 192, 256])?;
        Ok(Self { key: key.to_vec() })
    }
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}
impl CipherAlgorithm for Camellia {
    fn name(&self) -> &'static str {
        "camellia"
    }
    fn key_size_bits(&self) -> usize {
        self.key.len() * 8
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
    fn kind(&self) -> CipherKind {
        CipherKind::Other("camellia")
    }
}

// ================================= ChaCha20 ======================================
//
// Nota: el snippet de Python exige nonce de 128 bits (16 bytes). Esto no es el
// perfil IETF (96-bit) ni XChaCha20 (192-bit). Reproducimos la misma restricción.

#[derive(Debug, Clone)]
pub struct ChaCha20 {
    key: Vec<u8>,
    nonce: [u8; 16],
}
impl ChaCha20 {
    /// Clave de 256 bits y nonce de 128 bits (16 bytes).
    pub fn new(key: impl AsRef<[u8]>, nonce: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[256])?;
        let nonce = nonce.as_ref();
        if nonce.len() != 16 {
            return Err(AlgorithmError::NonceMustBe16);
        }
        let mut n = [0u8; 16];
        n.copy_from_slice(nonce);
        Ok(Self {
            key: key.to_vec(),
            nonce: n,
        })
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }
    pub fn nonce(&self) -> &[u8; 16] {
        &self.nonce
    }
}
impl CipherAlgorithm for ChaCha20 {
    fn name(&self) -> &'static str {
        "ChaCha20"
    }
    fn key_size_bits(&self) -> usize {
        256
    }
    fn block_size_bits(&self) -> Option<usize> {
        None
    } // stream cipher
    fn kind(&self) -> CipherKind {
        CipherKind::Other("ChaCha20")
    }
}

// =================================== SM4 =========================================

#[derive(Debug, Clone)]
pub struct SM4 {
    key: Vec<u8>,
}
impl SM4 {
    /// Clave de 128 bits, bloque de 128 bits.
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, AlgorithmError> {
        let key = key.as_ref();
        verify_key_size(key, &[128])?;
        Ok(Self { key: key.to_vec() })
    }
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}
impl CipherAlgorithm for SM4 {
    fn name(&self) -> &'static str {
        "SM4"
    }
    fn key_size_bits(&self) -> usize {
        self.key.len() * 8
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
    fn kind(&self) -> CipherKind {
        CipherKind::Other("SM4")
    }
}

// ================================== Tests ========================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_ok_sizes() {
        for bits in [128usize, 192, 256, 512] {
            let key = vec![0u8; bits / 8];
            assert!(AES::new(&key).is_ok());
        }
        assert!(AES::new(&[0u8; 24]).is_ok()); // 192
    }

    #[test]
    fn aes128_exact() {
        assert!(AES128::new(&[0u8; 16]).is_ok());
        assert!(AES128::new(&[0u8; 24]).is_err());
    }

    #[test]
    fn chacha20_nonce_len() {
        let key = [0u8; 32];
        assert!(ChaCha20::new(key, [0u8; 16]).is_ok());
        assert!(matches!(
            ChaCha20::new(key, [0u8; 12]).unwrap_err(),
            AlgorithmError::NonceMustBe16
        ));
    }

    #[test]
    fn sm4_key() {
        assert!(SM4::new([0u8; 16]).is_ok());
        assert!(SM4::new([0u8; 32]).is_err());
    }

    #[test]
    fn camellia_keys() {
        assert!(Camellia::new([0u8; 16]).is_ok());
        assert!(Camellia::new([0u8; 24]).is_ok());
        assert!(Camellia::new([0u8; 32]).is_ok());
        assert!(Camellia::new([0u8; 20]).is_err());
    }
}
