use crate::kryptography::{
    backends::symmetric::{AesCbcEngine, AesCtrEngine, AesGcmEngine, ChaCha20Poly1305Engine},
    enums::{AeadAlg, BlockModeAlg, StreamAlg},
    traits::{Aead, BlockMode, Stream},
};

pub mod backends;
pub mod enums;
pub mod errors;
pub mod traits;

// 3) Factoría: devuelve *handles* tipados por capacidad
pub struct CryptoFactory;

impl CryptoFactory {
    pub fn aead(alg: AeadAlg) -> Box<dyn Aead> {
        match alg {
            AeadAlg::AesGcm => Box::new(AesGcmEngine),
            AeadAlg::ChaCha20Poly1305 => Box::new(ChaCha20Poly1305Engine),
        }
    }

    pub fn stream(alg: StreamAlg) -> Box<dyn Stream> {
        match alg {
            StreamAlg::AesCtr => Box::new(AesCtrEngine),
        }
    }

    pub fn block_mode(alg: BlockModeAlg) -> Box<dyn BlockMode> {
        match alg {
            BlockModeAlg::AesCbc => Box::new(AesCbcEngine),
        }
    }
}
