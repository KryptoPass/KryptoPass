//! Unbiased random indices from the operating system's generator.

use kryptopass_secure_memory::SensitiveBytes;

use crate::GeneratorError;

/// Random words read in blocks, kept in zeroizing memory because they decide
/// which characters a secret gets.
pub(crate) struct RandomWords {
    bytes: SensitiveBytes,
    offset: usize,
}

const BLOCK: usize = 256;

impl RandomWords {
    pub(crate) fn new() -> Result<Self, GeneratorError> {
        let mut bytes = SensitiveBytes::with_capacity(BLOCK);
        bytes.resize(BLOCK, 0);
        let mut words = Self {
            bytes,
            offset: BLOCK,
        };
        words.refill()?;
        Ok(words)
    }

    fn refill(&mut self) -> Result<(), GeneratorError> {
        getrandom::fill(self.bytes.as_mut_slice()).map_err(|_| GeneratorError::Randomness)?;
        self.offset = 0;
        Ok(())
    }

    fn next_u32(&mut self) -> Result<u32, GeneratorError> {
        if self.offset + 4 > self.bytes.len() {
            self.refill()?;
        }
        let word = u32::from_le_bytes([
            self.bytes[self.offset],
            self.bytes[self.offset + 1],
            self.bytes[self.offset + 2],
            self.bytes[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(word)
    }

    /// A uniform index below `bound`, by rejection sampling.
    pub(crate) fn index(&mut self, bound: usize) -> Result<usize, GeneratorError> {
        let bound = u64::try_from(bound).map_err(|_| GeneratorError::EmptyClass)?;
        if bound == 0 {
            return Err(GeneratorError::EmptyClass);
        }
        let range = u64::from(u32::MAX) + 1;
        let acceptance = (range / bound) * bound;
        // A draw is rejected with probability below one half, so 128 failures
        // in a row mean the source is broken, not unlucky.
        for _ in 0..128 {
            let value = u64::from(self.next_u32()?);
            if value < acceptance {
                return usize::try_from(value % bound).map_err(|_| GeneratorError::Randomness);
            }
        }
        Err(GeneratorError::Randomness)
    }
}
