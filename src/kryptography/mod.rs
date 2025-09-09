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

// --- Enforce: exactamente 1 backend activo ---
#[cfg(all(feature = "backend-rustcrypto", feature = "backend-openssl"))]
compile_error!("Habilita exactamente UN backend: 'backend-rustcrypto' O 'backend-openssl'.");
#[cfg(not(any(feature = "backend-rustcrypto", feature = "backend-openssl")))]
compile_error!("Debes habilitar un backend: 'backend-rustcrypto' o 'backend-openssl'.");

#[cfg(feature = "backend-rustcrypto")]
pub const BACKEND_NAME: &str = "RustCrypto";
#[cfg(feature = "backend-openssl")]
pub const BACKEND_NAME: &str = "OpenSSL";

pub struct CryptoFactory;
impl CryptoFactory {
    // --- AEAD ---
    pub fn try_aead(alg: AeadAlg) -> Result<Box<dyn Aead>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                AeadAlg::AesGcm => Ok(Box::new(backends::rustcrypto::AesGcmEngine)),
                AeadAlg::ChaCha20Poly1305 => Ok(Box::new(backends::rustcrypto::ChaCha20Poly1305Engine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                AeadAlg::AesGcm => Ok(Box::new(backends::openssl::AesGcmEngine)),
                AeadAlg::ChaCha20Poly1305 => Ok(Box::new(backends::openssl::ChaCha20Poly1305Engine)),
            }
        }
    }
    pub fn try_aead_from_str(name: &str) -> Result<Box<dyn Aead>, FactoryError> {
        let alg: AeadAlg = name.parse().map_err(|e: ParseAeadAlgError| FactoryError::Parse(e.0))?;
        Self::try_aead(alg)
    }

    // --- Stream ---
    pub fn try_stream(alg: StreamAlg) -> Result<Box<dyn Stream>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                StreamAlg::AesCtr => Ok(Box::new(backends::rustcrypto::AesCtrEngine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                StreamAlg::AesCtr => Ok(Box::new(backends::openssl::AesCtrEngine)),
            }
        }
    }
    pub fn try_stream_from_str(name: &str) -> Result<Box<dyn Stream>, FactoryError> {
        let alg: StreamAlg = name
            .parse()
            .map_err(|e: ParseStreamAlgError| FactoryError::Parse(e.0))?;
        Self::try_stream(alg)
    }

    // --- BlockMode ---
    pub fn try_block_mode(alg: BlockModeAlg) -> Result<Box<dyn BlockMode>, FactoryError> {
        if !alg.is_supported() {
            return Err(FactoryError::Unsupported {
                algo: alg.as_str(),
                backend: BACKEND_NAME,
            });
        }
        #[cfg(feature = "backend-rustcrypto")]
        {
            match alg {
                BlockModeAlg::AesCbc => Ok(Box::new(backends::rustcrypto::AesCbcEngine)),
            }
        }
        #[cfg(feature = "backend-openssl")]
        {
            match alg {
                BlockModeAlg::AesCbc => Ok(Box::new(backends::openssl::AesCbcEngine)),
            }
        }
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
