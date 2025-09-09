use anyhow::Result;
use kryptopass::kryptography::{prelude::*, support::Support};

fn demo_aead() -> Result<()> {
    let key = Key32::try_from_slice(&[7u8; 32])?;
    let nonce = Nonce12::try_from_slice(&[1u8; 12])?;

    let engine = CryptoFactory::try_aead(AeadAlg::AesGcm)?;
    let ct = engine.encrypt_checked(&key, &nonce, b"hola", Some(b"hdr"))?;
    let pt = engine.decrypt_checked(&key, &nonce, &ct, Some(b"hdr"))?;
    assert_eq!(pt, b"hola");
    Ok(())
}

fn demo_stream_and_block_if_supported() -> Result<()> {
    let key = Key32::try_from_slice(&[9u8; 32])?;
    let iv = Iv16::try_from_slice(&[2u8; 16])?;

    if StreamAlg::AesCtr.is_supported() {
        let s = CryptoFactory::try_stream(StreamAlg::AesCtr)?;
        let data = b"data";
        let out = s.apply_keystream_checked(&key, &iv, data)?;
        let round = s.apply_keystream_checked(&key, &iv, &out)?;
        assert_eq!(round, data);
    }

    if BlockModeAlg::AesCbc.is_supported() {
        let b = CryptoFactory::try_block_mode(BlockModeAlg::AesCbc)?;
        let ct = b.encrypt_checked(&key, &iv, b"hola mundo")?;
        let pt = b.decrypt_checked(&key, &iv, &ct)?;
        assert_eq!(pt, b"hola mundo");
    }

    Ok(())
}

fn print_support() {
    let s = Support::active();
    print!("AEAD:");
    s.aead.iter().for_each(|s| print!("{s}, "));
    println!();
    print!("Block:");
    s.block.iter().for_each(|s| print!("{s}, "));
    println!();
    print!("Stream:");
    s.stream.iter().for_each(|s| print!("{s}, "));
    println!();
}

fn main() -> Result<()> {
    demo_aead()?;
    demo_stream_and_block_if_supported()?;
    print_support();

    Ok(())
}
