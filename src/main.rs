use anyhow::Result;
use kryptopass::kryptography::{
    CryptoFactory,
    aead::AeadAlg,
    aead_decrypt_checked, aead_encrypt_checked,
    block::BlockModeAlg,
    cbc_decrypt_checked, cbc_encrypt_checked,
    stream::StreamAlg,
    stream_apply_checked,
    utils::{Iv16, Key32, Nonce12},
};

fn demo_aead() {
    let key = Key32::try_from_slice(&[7u8; 32]).unwrap();
    let nonce = Nonce12::try_from_slice(&[1u8; 12]).unwrap();

    let engine = CryptoFactory::try_aead(AeadAlg::AesGcm).expect("aead");
    let ct = aead_encrypt_checked(&*engine, &key, &nonce, b"hola", Some(b"hdr")).expect("enc");
    let pt = aead_decrypt_checked(&*engine, &key, &nonce, &ct, Some(b"hdr")).expect("dec");
    assert_eq!(pt, b"hola");
}

fn demo_stream_and_block_if_supported() {
    let key = Key32::try_from_slice(&[9u8; 32]).unwrap();
    let iv = Iv16::try_from_slice(&[2u8; 16]).unwrap();

    if StreamAlg::AesCtr.is_supported() {
        let s = CryptoFactory::try_stream(StreamAlg::AesCtr).unwrap();
        let data = b"data";
        let out = stream_apply_checked(&*s, &key, &iv, data).unwrap();
        let round = stream_apply_checked(&*s, &key, &iv, &out).unwrap();
        assert_eq!(round, data);
    }

    if BlockModeAlg::AesCbc.is_supported() {
        let b = CryptoFactory::try_block_mode(BlockModeAlg::AesCbc).unwrap();
        let ct = cbc_encrypt_checked(&*b, &key, &iv, b"hola mundo").unwrap();
        let pt = cbc_decrypt_checked(&*b, &key, &iv, &ct).unwrap();
        assert_eq!(pt, b"hola mundo");
    }
}

fn main() -> Result<()> {
    demo_aead();
    demo_stream_and_block_if_supported();

    Ok(())
}
