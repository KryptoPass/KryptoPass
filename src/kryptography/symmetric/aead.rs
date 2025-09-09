use core::fmt;
use core::str::FromStr;
use thiserror::Error;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AeadAlg {
    AesGcm,
    ChaCha20Poly1305,
}

impl AeadAlg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AesGcm => "AES-GCM",
            Self::ChaCha20Poly1305 => "ChaCha20-Poly1305",
        }
    }

    pub fn is_supported(self) -> bool {
        crate::kryptography::support::Support::active().aead.contains(&self)
    }
}

impl fmt::Display for AeadAlg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error)]
#[error("AEAD desconocido: {0}")]
pub struct ParseAeadAlgError(pub String);
impl FromStr for AeadAlg {
    type Err = ParseAeadAlgError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "aes-gcm" | "aes_gcm" | "aesgcm" => Ok(Self::AesGcm),
            "chacha20-poly1305" | "chacha20_poly1305" | "chacha20" | "cc20p1305" => Ok(Self::ChaCha20Poly1305),
            _ => Err(ParseAeadAlgError(s)),
        }
    }
}
