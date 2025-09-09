use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::kryptography::errors::LengthError;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct Sensitive<const N: usize>([u8; N]);
impl<const N: usize> Sensitive<N> {
    pub fn new(bytes: [u8; N]) -> Self {
        Self(bytes)
    }
    pub fn try_from_slice(s: &[u8]) -> Result<Self, LengthError> {
        if s.len() != N {
            return Err(LengthError {
                expected: N,
                got: s.len(),
            });
        }
        let mut arr = [0u8; N];
        arr.copy_from_slice(s);
        Ok(Self(arr))
    }
    pub fn as_array(&self) -> &[u8; N] {
        &self.0
    }
}
impl<const N: usize> AsRef<[u8]> for Sensitive<N> {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl<const N: usize> fmt::Debug for Sensitive<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sensitive<{}>([REDACTED])", N)
    }
}

pub type Key32 = Sensitive<32>; // AES-256, ChaCha20-Poly1305
pub type Nonce12 = Sensitive<12>; // AEAD (GCM/ChaCha20-Poly1305)
pub type Iv16 = Sensitive<16>; // AES-CTR/AES-CBC
