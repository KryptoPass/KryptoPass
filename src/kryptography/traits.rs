use crate::kryptography::errors::{AeadError, BlockModeError, StreamError};

pub trait Aead: Send + Sync {
    fn algorithm_name(&self) -> &'static str;
    fn encrypt(&self, key: &[u8], nonce: &[u8], pt: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError>;
    fn decrypt(&self, key: &[u8], nonce: &[u8], ct: &[u8], aad: Option<&[u8]>) -> Result<Vec<u8>, AeadError>;
}

pub trait Stream: Send + Sync {
    fn algorithm_name(&self) -> &'static str;
    fn apply_keystream(&self, key: &[u8], nonce: &[u8], data: &[u8]) -> Result<Vec<u8>, StreamError>;
}

pub trait BlockMode: Send + Sync {
    fn algorithm_name(&self) -> &'static str;
    fn encrypt(&self, key: &[u8], iv: &[u8], pt: &[u8]) -> Result<Vec<u8>, BlockModeError>;
    fn decrypt(&self, key: &[u8], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, BlockModeError>;
}
