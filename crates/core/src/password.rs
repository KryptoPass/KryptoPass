//! Cryptographically secure password generation.

use anyhow::{Result, bail, ensure};
use zeroize::{Zeroize, Zeroizing};

/// Character classes and length for a generated password.
pub struct PasswordOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub excluded: String,
}

/// Generates a password with at least one character from every selected class.
///
/// Each draw uses rejection sampling, so mapping random words onto the class
/// alphabets does not introduce modulo bias. The result is zeroized on drop.
pub fn generate_password(options: &PasswordOptions) -> Result<Zeroizing<String>> {
    ensure!(
        (4..=1024).contains(&options.length),
        "password length must be between 4 and 1024"
    );
    ensure!(
        options.excluded.len() <= 256,
        "excluded characters list is too long"
    );

    let mut classes = Vec::with_capacity(4);
    if options.lowercase {
        classes.push(filtered_class(b"abcdefghijklmnopqrstuvwxyz", options));
    }
    if options.uppercase {
        classes.push(filtered_class(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ", options));
    }
    if options.digits {
        classes.push(filtered_class(b"0123456789", options));
    }
    if options.symbols {
        classes.push(filtered_class(b"!@#$%^&*()-_=+[]{};:,.?", options));
    }

    ensure!(!classes.is_empty(), "select at least one character class");
    ensure!(
        options.length >= classes.len(),
        "password length is too short for the selected character classes"
    );
    if classes.iter().any(Vec::is_empty) {
        bail!("a selected character class has no allowed characters");
    }

    let alphabet: Vec<u8> = classes.iter().flatten().copied().collect();
    let mut random = RandomWords::new()?;
    let mut output = Zeroizing::new(Vec::with_capacity(options.length));

    for class in &classes {
        output.push(class[random.index(class.len())?]);
    }
    while output.len() < options.length {
        output.push(alphabet[random.index(alphabet.len())?]);
    }

    for index in (1..output.len()).rev() {
        let swap_index = random.index(index + 1)?;
        output.swap(index, swap_index);
    }

    let output = std::mem::take(&mut *output);
    let password = String::from_utf8(output).map_err(|error| {
        error.into_bytes().zeroize();
        anyhow::anyhow!("password generation failed")
    })?;
    Ok(Zeroizing::new(password))
}

fn filtered_class(source: &[u8], options: &PasswordOptions) -> Vec<u8> {
    source
        .iter()
        .copied()
        .filter(|byte| !options.excluded.contains(char::from(*byte)))
        .collect()
}

struct RandomWords {
    bytes: [u8; 256],
    offset: usize,
}

impl RandomWords {
    fn new() -> Result<Self> {
        let mut result = Self {
            bytes: [0; 256],
            offset: 256,
        };
        result.refill()?;
        Ok(result)
    }

    fn refill(&mut self) -> Result<()> {
        getrandom::fill(&mut self.bytes)
            .map_err(|_| anyhow::anyhow!("secure randomness unavailable"))?;
        self.offset = 0;
        Ok(())
    }

    fn next_u32(&mut self) -> Result<u32> {
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

    fn index(&mut self, upper_bound: usize) -> Result<usize> {
        let bound = u64::try_from(upper_bound)
            .map_err(|_| anyhow::anyhow!("invalid password character set"))?;
        ensure!(bound > 0, "invalid password character set");
        let range = u64::from(u32::MAX) + 1;
        let acceptance_limit = (range / bound) * bound;
        for _ in 0..128 {
            let value = u64::from(self.next_u32()?);
            if value < acceptance_limit {
                return usize::try_from(value % bound)
                    .map_err(|_| anyhow::anyhow!("password generation failed"));
            }
        }
        bail!("password generation failed")
    }
}

impl Drop for RandomWords {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.bytes.zeroize();
    }
}
