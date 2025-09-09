use crate::kryptography::active_backend;
use crate::kryptography::{BACKEND_NAME, aead::AeadAlg, block::BlockModeAlg, stream::StreamAlg};

pub struct Support {
    pub backend: &'static str,
    pub aead: &'static [AeadAlg],
    pub stream: &'static [StreamAlg],
    pub block: &'static [BlockModeAlg],
}

impl Support {
    pub fn active() -> Self {
        Self {
            backend: BACKEND_NAME,
            aead: active_backend::AEAD_SUPPORTED,
            stream: active_backend::STREAM_SUPPORTED,
            block: active_backend::BLOCK_SUPPORTED,
        }
    }
}
