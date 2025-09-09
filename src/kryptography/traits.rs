use anyhow::Result;

// 1) Capacidades (interfaces de alto nivel)
pub trait Aead {
    fn encrypt(
        &mut self,
        key: &[u8],
        nonce: &[u8],
        pt: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
    fn decrypt(
        &mut self,
        key: &[u8],
        nonce: &[u8],
        ct: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
}

pub trait Stream {
    fn apply_keystream(&mut self, key: &[u8], nonce: &[u8], data: &[u8]) -> Result<Vec<u8>>;
}

pub trait BlockMode {
    fn encrypt(&mut self, key: &[u8], iv: &[u8], pt: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&mut self, key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>>;
}
