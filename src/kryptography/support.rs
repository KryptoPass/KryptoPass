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

#[cfg(test)]
mod sanity {
    use crate::kryptography::{CryptoFactory, support::Support};

    #[test]
    fn aead_factory_matches_support() {
        for &alg in Support::active().aead {
            CryptoFactory::try_aead(alg).expect("supported AEAD must build");
        }
    }

    #[test]
    fn stream_factory_matches_support() {
        for &alg in Support::active().stream {
            CryptoFactory::try_stream(alg).expect("supported Stream must build");
        }
    }

    #[test]
    fn block_factory_matches_support() {
        for &alg in Support::active().block {
            CryptoFactory::try_block_mode(alg).expect("supported Block must build");
        }
    }
}
