//! Secret generators: passwords and PINs.
//!
//! What a generator offers (length range, character classes and their
//! defaults) comes from the generators a template library declares; this
//! crate turns those settings into a value. It knows nothing about vaults or
//! items, so the editor, the browser extension and any other client can use
//! it alone.

#![forbid(unsafe_code)]

mod random;
mod settings;

pub use settings::{GeneratorError, Settings};

use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};

use random::RandomWords;

/// Generates a value with at least one character of every enabled class.
///
/// Each draw uses rejection sampling, so mapping random words onto the
/// alphabets adds no modulo bias; a final shuffle hides where the guaranteed
/// characters went. The result is zeroized on drop.
pub fn generate(settings: &Settings) -> Result<SensitiveText, GeneratorError> {
    let classes = settings.alphabets()?;
    let length = settings.length();
    if length < classes.len() {
        return Err(GeneratorError::TooShort);
    }
    let alphabet: Vec<u8> = classes.iter().flatten().copied().collect();
    let mut random = RandomWords::new()?;
    let mut output = SensitiveBytes::with_capacity(length);
    output.resize(length, 0);

    let mut written = 0;
    for class in &classes {
        output.as_mut_slice()[written] = class[random.index(class.len())?];
        written += 1;
    }
    while written < length {
        output.as_mut_slice()[written] = alphabet[random.index(alphabet.len())?];
        written += 1;
    }
    for index in (1..output.len()).rev() {
        let swap = random.index(index + 1)?;
        output.as_mut_slice().swap(index, swap);
    }
    // Every alphabet is ASCII, so this cannot fail.
    SensitiveText::from_utf8(output).map_err(|_| GeneratorError::Randomness)
}
