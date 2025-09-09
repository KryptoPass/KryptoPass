use anyhow::Result;
use kryptopass::kryptography::{
    CryptoFactory,
    enums::{AeadAlg, StreamAlg},
};

// 5) Uso: el tipo devuelto restringe las operaciones disponibles
fn main() -> Result<()> {
    let key = [0u8; 32];
    let nonce = [0u8; 12];
    let aad = b"hdr";

    // AEAD: solo existen métodos AEAD
    let mut aead = CryptoFactory::aead(AeadAlg::AesGcm);
    let ct = aead.encrypt(&key, &nonce, b"hola", Some(aad))?;
    let pt = aead.decrypt(&key, &nonce, &ct, Some(aad))?;
    assert_eq!(pt, b"hola");

    // Cambio “on the fly” a otro AEAD (sigue siendo un handle AEAD)
    let mut aead = CryptoFactory::aead(AeadAlg::ChaCha20Poly1305);
    let _ = aead.encrypt(&key, &nonce, b"hola", Some(aad))?;

    // Si quiero un stream, pido un handle Stream (no tendrá métodos AEAD)
    let mut stream = CryptoFactory::stream(StreamAlg::AesCtr);
    let _ = stream.apply_keystream(&key, &nonce, b"data")?;

    Ok(())
}
