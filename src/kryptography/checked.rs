use crate::kryptography::{
    errors::{AeadError, BlockModeError, StreamError},
    traits::{Aead, BlockMode, Stream},
    types::{Iv16, Key32, Nonce12},
};

// src/checked.rs
pub trait AeadChecked {
    fn encrypt_checked(
        &self,
        key: &Key32,
        nonce: &Nonce12,
        pt: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadError>;
    fn decrypt_checked(
        &self,
        key: &Key32,
        nonce: &Nonce12,
        ct: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadError>;
}

impl<T: Aead + ?Sized> AeadChecked for T {
    fn encrypt_checked(
        &self,
        key: &Key32,
        nonce: &Nonce12,
        pt: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadError> {
        self.encrypt(key.as_ref(), nonce.as_ref(), pt, aad)
    }

    fn decrypt_checked(
        &self,
        key: &Key32,
        nonce: &Nonce12,
        ct: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, AeadError> {
        self.decrypt(key.as_ref(), nonce.as_ref(), ct, aad)
    }
}

pub trait StreamChecked {
    fn apply_keystream_checked(&self, key: &Key32, iv: &Iv16, data: &[u8]) -> Result<Vec<u8>, StreamError>;
}
impl<T: Stream + ?Sized> StreamChecked for T {
    fn apply_keystream_checked(&self, key: &Key32, iv: &Iv16, data: &[u8]) -> Result<Vec<u8>, StreamError> {
        self.apply_keystream(key.as_ref(), iv.as_ref(), data)
    }
}

pub trait BlockModeChecked {
    fn encrypt_checked(&self, key: &Key32, iv: &Iv16, pt: &[u8]) -> Result<Vec<u8>, BlockModeError>;
    fn decrypt_checked(&self, key: &Key32, iv: &Iv16, ct: &[u8]) -> Result<Vec<u8>, BlockModeError>;
}
impl<T: BlockMode + ?Sized> BlockModeChecked for T {
    fn encrypt_checked(&self, key: &Key32, iv: &Iv16, pt: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        self.encrypt(key.as_ref(), iv.as_ref(), pt)
    }
    fn decrypt_checked(&self, key: &Key32, iv: &Iv16, ct: &[u8]) -> Result<Vec<u8>, BlockModeError> {
        self.decrypt(key.as_ref(), iv.as_ref(), ct)
    }
}
