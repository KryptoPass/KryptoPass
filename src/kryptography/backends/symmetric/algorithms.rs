use std::fmt;

use crate::kryptography::backends::symmetric::errors::AlgorithmError;

pub trait BlockCipherAlgorithm: fmt::Debug {
    fn name(&self) -> &'static str;
    fn key_size_bits(&self) -> usize;
    fn block_size_bits(&self) -> Option<usize>;
}

fn verify_key_size(key: &[u8], allowed: &'static [usize]) -> Result<(), AlgorithmError> {
    if key.is_empty() {
        return Err(AlgorithmError::EmptyKey);
    }
    let bits = key.len() * 8;
    if allowed.contains(&bits) {
        Ok(())
    } else {
        Err(AlgorithmError::InvalidKeySize {
            allowed,
            got_bits: bits,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AES {
    key: Vec<u8>,
    key_sizes: &'static [usize],
}
pub const AES: AES = AES {
    key: vec![],
    key_sizes: &[128, 192, 256, 512],
};

impl BlockCipherAlgorithm for AES {
    fn name(&self) -> &'static str {
        "AES"
    }
    fn key_size_bits(&self) -> usize {
        self.key.len() * 8
    }
    fn block_size_bits(&self) -> Option<usize> {
        Some(128)
    }
}
