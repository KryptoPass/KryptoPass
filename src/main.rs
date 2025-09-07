use anyhow::Result;
use kryptopass::kryptography::symmetric::{Algorithm, Cipher, CipherSpec, Mode, Padding};

// Tipos fuertes para claves/IV
pub struct Key<const N: usize>(pub [u8; N]);
pub struct Iv<const N: usize>(pub [u8; N]);

fn main() -> Result<()> {
    let spec = CipherSpec {
        algorithm: Algorithm::AES256,
        key: Key::<32>([0x11; 32]),
        mode: Mode::CBC {
            iv: Iv::<16>([0u8; 16]),
            padding: Padding::Pkcs7,
        },
    };

    let cipher = Cipher::create(spec)?;
    let ct = cipher.encrypt(b"hola mundo!")?;
    let pt = cipher.decrypt(&ct)?;
    assert_eq!(pt.as_slice(), b"hola mundo!");
    Ok(())
}

// Modes:
// Initialization Vector
// Tweak
// Nonce
// Authentication Tag
