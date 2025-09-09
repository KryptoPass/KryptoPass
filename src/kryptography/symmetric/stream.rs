use core::fmt;
use core::str::FromStr;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamAlg {
    AesCtr,
}
impl StreamAlg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AesCtr => "AES-CTR",
        }
    }

    pub fn is_supported(self) -> bool {
        crate::kryptography::support::Support::active().stream.contains(&self)
    }
}
impl fmt::Display for StreamAlg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Debug, Error)]
#[error("Stream desconocido: {0}")]
pub struct ParseStreamAlgError(pub String);
impl FromStr for StreamAlg {
    type Err = ParseStreamAlgError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_lowercase();
        match s.as_str() {
            "aes-ctr" | "aes_ctr" | "aesctr" | "ctr" => Ok(Self::AesCtr),
            _ => Err(ParseStreamAlgError(s)),
        }
    }
}
