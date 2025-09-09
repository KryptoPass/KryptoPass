use anyhow::Result;

use crate::kryptography::traits::{Aead, BlockMode, Stream};

// 4) Implementaciones mock (solo para compilar el ejemplo)
pub struct AesGcmEngine;
impl Aead for AesGcmEngine {
    fn encrypt(&mut self, _k: &[u8], _n: &[u8], pt: &[u8], _aad: Option<&[u8]>) -> Result<Vec<u8>> {
        Ok(pt.to_vec())
    }
    fn decrypt(&mut self, _k: &[u8], _n: &[u8], ct: &[u8], _aad: Option<&[u8]>) -> Result<Vec<u8>> {
        Ok(ct.to_vec())
    }
}

pub struct ChaCha20Poly1305Engine;
impl Aead for ChaCha20Poly1305Engine {
    fn encrypt(&mut self, _k: &[u8], _n: &[u8], pt: &[u8], _aad: Option<&[u8]>) -> Result<Vec<u8>> {
        Ok(pt.to_vec())
    }
    fn decrypt(&mut self, _k: &[u8], _n: &[u8], ct: &[u8], _aad: Option<&[u8]>) -> Result<Vec<u8>> {
        Ok(ct.to_vec())
    }
}

pub struct AesCtrEngine;
impl Stream for AesCtrEngine {
    fn apply_keystream(&mut self, _k: &[u8], _n: &[u8], data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }
}

pub struct AesCbcEngine;
impl BlockMode for AesCbcEngine {
    fn encrypt(&mut self, _k: &[u8], _iv: &[u8], pt: &[u8]) -> Result<Vec<u8>> {
        Ok(pt.to_vec())
    }
    fn decrypt(&mut self, _k: &[u8], _iv: &[u8], ct: &[u8]) -> Result<Vec<u8>> {
        Ok(ct.to_vec())
    }
}
