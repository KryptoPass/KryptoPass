use crate::kryptography::{BACKEND_NAME, aead::AeadAlg, block::BlockModeAlg, stream::StreamAlg};

pub struct Support {
    pub backend: &'static str,
    pub aead: &'static [AeadAlg],
    pub stream: &'static [StreamAlg],
    pub block: &'static [BlockModeAlg],
}

#[cfg(feature = "backend-rustcrypto")]
const AEAD_SUPPORTED: &[AeadAlg] = &[AeadAlg::AesGcm, AeadAlg::ChaCha20Poly1305];
#[cfg(feature = "backend-rustcrypto")]
const STREAM_SUPPORTED: &[StreamAlg] = &[StreamAlg::AesCtr];
#[cfg(feature = "backend-rustcrypto")]
const BLOCK_SUPPORTED: &[BlockModeAlg] = &[BlockModeAlg::AesCbc];

#[cfg(feature = "backend-openssl")]
const AEAD_SUPPORTED: &[AeadAlg] = &[AeadAlg::AesGcm /*, AeadAlg::ChaCha20Poly1305 si implementas */];
#[cfg(feature = "backend-openssl")]
const STREAM_SUPPORTED: &[StreamAlg] = &[];
#[cfg(feature = "backend-openssl")]
const BLOCK_SUPPORTED: &[BlockModeAlg] = &[];

impl Support {
    pub fn active() -> Self {
        Self {
            backend: BACKEND_NAME,
            aead: AEAD_SUPPORTED,
            stream: STREAM_SUPPORTED,
            block: BLOCK_SUPPORTED,
        }
    }
}
