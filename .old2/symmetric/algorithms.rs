use std::fmt;

use crate::kryptography::backends::symmetric::errors::AlgorithmError;

#[derive(Debug, Clone, Copy)]
pub struct AlgorithmDescriptor {
    pub name: &'static str,
    pub block_size_bits: Option<usize>,
    pub key_sizes_bits: &'static [usize],
}

pub trait BlockCipherAlgorithm: fmt::Debug {
    fn desc(&self) -> AlgorithmDescriptor;

    fn name(&self) -> &'static str {
        self.desc().name
    }

    fn block_size_bits(&self) -> Option<usize> {
        self.desc().block_size_bits
    }

    fn key_sizes(&self) -> &'static [usize] {
        self.desc().key_sizes_bits
    }

    fn verify_key_size(&self, key: &[u8]) -> Result<(), AlgorithmError> {
        let bits = key.len() * 8;
        if bits == 0 {
            return Err(AlgorithmError::EmptyKey);
        }
        if self.key_sizes().contains(&bits) {
            Ok(())
        } else {
            Err(AlgorithmError::InvalidKeySize {
                allowed: self.key_sizes(),
                got_bits: bits,
            })
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AES;

impl BlockCipherAlgorithm for AES {
    fn desc(&self) -> AlgorithmDescriptor {
        AlgorithmDescriptor {
            name: "AES",
            block_size_bits: Some(128),
            key_sizes_bits: &[128, 192, 256],
        }
    }
}
