use core::fmt;
use core::str::FromStr;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockModeAlg {
    AesCbc,
}
impl BlockModeAlg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AesCbc => "AES-CBC",
        }
    }
    pub fn is_supported(self) -> bool {
        #[cfg(feature = "backend-rustcrypto")]
        {
            matches!(self, Self::AesCbc)
        }
        #[cfg(feature = "backend-openssl")]
        {
            false /* implementar cuando corresponda */
        }
    }
}
impl fmt::Display for BlockModeAlg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Debug, Error)]
#[error("BlockMode desconocido: {0}")]
pub struct ParseBlockAlgError(pub String);
impl FromStr for BlockModeAlg {
    type Err = ParseBlockAlgError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "aes-cbc" | "aes_cbc" | "aescbc" | "cbc" => Ok(Self::AesCbc),
            _ => Err(ParseBlockAlgError(s)),
        }
    }
}
