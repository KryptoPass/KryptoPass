pub mod backends;
pub mod checked;
pub mod errors;
pub mod support;
pub mod symmetric;
pub mod traits;
pub mod types;

pub use aead::AeadAlg;
pub use block::BlockModeAlg;
pub use checked::{AeadChecked, BlockModeChecked, StreamChecked};
pub use stream::StreamAlg;
pub use symmetric::{aead, block, stream};
pub use traits::{Aead, BlockMode, Stream};
pub use types::{Iv16, Key32, Nonce12};

use crate::kryptography::{
    aead::ParseAeadAlgError, block::ParseBlockAlgError, errors::FactoryError, stream::ParseStreamAlgError,
};

// Enforce: exactly one backend active
#[cfg(all(feature = "backend-rustcrypto", feature = "backend-openssl"))]
compile_error!("Enable exactly one backend: 'backend-rustcrypto' OR 'backend-openssl'.");

#[cfg(not(any(feature = "backend-rustcrypto", feature = "backend-openssl")))]
compile_error!("You must enable a backend: 'backend-rustcrypto' or 'backend-openssl'.");

// Make the active backend visible to sibling modules (support, factory, etc.)
#[cfg(feature = "backend-rustcrypto")]
pub(crate) mod active_backend {
    pub use crate::kryptography::backends::rustcrypto::*;
}

#[cfg(feature = "backend-openssl")]
pub(crate) mod active_backend {
    pub use crate::kryptography::backends::openssl::*;
}

pub use active_backend::BACKEND_NAME;

pub struct CryptoFactory;

impl CryptoFactory {
    // AEAD
    pub fn try_aead(alg: AeadAlg) -> Result<Box<dyn Aead>, FactoryError> {
        if !active_backend::AEAD_SUPPORTED.contains(&alg) {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        active_backend::make_aead(alg).ok_or_else(|| FactoryError::Unsupported {
            algo: alg.as_str(),
            backend: BACKEND_NAME,
        })
    }

    pub fn try_aead_from_str(name: &str) -> Result<Box<dyn Aead>, FactoryError> {
        let alg: AeadAlg = name.parse().map_err(|e: ParseAeadAlgError| FactoryError::Parse(e.0))?;
        Self::try_aead(alg)
    }

    // Stream
    pub fn try_stream(alg: StreamAlg) -> Result<Box<dyn Stream>, FactoryError> {
        if !active_backend::STREAM_SUPPORTED.contains(&alg) {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        active_backend::make_stream(alg).ok_or_else(|| FactoryError::Unsupported {
            algo: alg.as_str(),
            backend: BACKEND_NAME,
        })
    }

    pub fn try_stream_from_str(name: &str) -> Result<Box<dyn Stream>, FactoryError> {
        let alg: StreamAlg = name
            .parse()
            .map_err(|e: ParseStreamAlgError| FactoryError::Parse(e.0))?;
        Self::try_stream(alg)
    }

    // BlockMode
    pub fn try_block_mode(alg: BlockModeAlg) -> Result<Box<dyn BlockMode>, FactoryError> {
        if !active_backend::BLOCK_SUPPORTED.contains(&alg) {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        active_backend::make_block(alg).ok_or_else(|| FactoryError::Unsupported {
            algo: alg.as_str(),
            backend: BACKEND_NAME,
        })
    }

    pub fn try_block_mode_from_str(name: &str) -> Result<Box<dyn BlockMode>, FactoryError> {
        let alg: BlockModeAlg = name.parse().map_err(|e: ParseBlockAlgError| FactoryError::Parse(e.0))?;
        Self::try_block_mode(alg)
    }
}

pub mod prelude {
    pub use crate::kryptography::checked::{AeadChecked, BlockModeChecked, StreamChecked};
    pub use crate::kryptography::traits::{Aead, BlockMode, Stream};
    pub use crate::kryptography::types::{Iv16, Key32, Nonce12};
    pub use crate::kryptography::{AeadAlg, BlockModeAlg, CryptoFactory, StreamAlg};
}
