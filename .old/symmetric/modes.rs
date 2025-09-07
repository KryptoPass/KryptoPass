use crate::kryptography::backends::symmetric::{
    algorithms::{CipherAlgorithm, CipherKind},
    errors::ModeError,
};

/// Tipo para datos binarios inmutables
pub type Bytes<'a> = &'a [u8];

/// Contrato base de un modo.
pub trait Mode {
    fn name(&self) -> &'static str;

    /// Valida el par (modo, algoritmo).
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError>;
}

/// Sub‐traits especializados (paridad con las ABC de Python)
pub trait ModeWithInitializationVector: Mode {
    fn initialization_vector(&self) -> Bytes<'_>;
}

pub trait ModeWithTweak: Mode {
    fn tweak(&self) -> Bytes<'_>;
}

pub trait ModeWithNonce: Mode {
    fn nonce(&self) -> Bytes<'_>;
}

pub trait ModeWithAuthenticationTag: Mode {
    fn tag(&self) -> Option<Bytes<'_>>;
}

fn check_aes_key_length(
    mode_name: &'static str,
    alg: &dyn CipherAlgorithm,
) -> Result<(), ModeError> {
    if matches!(alg.kind(), CipherKind::Aes) && alg.key_size_bits() > 256 {
        return Err(ModeError::AesKeySize(mode_name));
    }
    Ok(())
}

fn check_iv_length(
    mode_name: &'static str,
    iv: &[u8],
    alg: &dyn CipherAlgorithm,
) -> Result<(), ModeError> {
    let bs = alg
        .block_size_bits()
        .ok_or(ModeError::BlockCipherRequired(mode_name))?;
    if iv.len() * 8 != bs {
        return Err(ModeError::InvalidIv {
            mode: mode_name,
            got: iv.len(),
            expected_bits: bs,
        });
    }
    Ok(())
}

fn check_nonce_length(
    mode_name: &'static str,
    nonce: &[u8],
    alg: &dyn CipherAlgorithm,
) -> Result<(), ModeError> {
    let bs = alg
        .block_size_bits()
        .ok_or(ModeError::BlockCipherRequired(mode_name))?;
    if nonce.len() * 8 != bs {
        return Err(ModeError::InvalidNonce {
            mode: mode_name,
            got: nonce.len(),
            expected_bits: bs,
        });
    }
    Ok(())
}

fn check_iv_and_key_length(
    mode_name: &'static str,
    iv: &[u8],
    alg: &dyn CipherAlgorithm,
) -> Result<(), ModeError> {
    if alg.block_size_bits().is_none() {
        return Err(ModeError::BlockCipherRequired(mode_name));
    }
    check_aes_key_length(mode_name, alg)?;
    check_iv_length(mode_name, iv, alg)
}

#[derive(Debug, Clone)]
pub struct CBC<'a> {
    iv: Bytes<'a>,
}
impl<'a> CBC<'a> {
    pub fn new(iv: Bytes<'a>) -> Self {
        Self { iv }
    }
}
impl<'a> Mode for CBC<'a> {
    fn name(&self) -> &'static str {
        "CBC"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_iv_and_key_length(self.name(), self.iv, alg)
    }
}
impl<'a> ModeWithInitializationVector for CBC<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}

#[derive(Debug, Clone)]
pub struct XTS<'a> {
    tweak: Bytes<'a>,
}
impl<'a> XTS<'a> {
    pub fn new(tweak: Bytes<'a>) -> Result<Self, ModeError> {
        if tweak.len() != 16 {
            return Err(ModeError::InvalidTweak);
        }
        Ok(Self { tweak })
    }
}
impl<'a> Mode for XTS<'a> {
    fn name(&self) -> &'static str {
        "XTS"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        // En Python se prohíben las clases AES128/AES256 “wrapper”; aquí solo validamos tamaños.
        match alg.kind() {
            CipherKind::Aes => match alg.key_size_bits() {
                256 | 512 => Ok(()),
                _ => Err(ModeError::InvalidXtsKeySize),
            },
            _ => Err(ModeError::BlockCipherRequired(self.name())), // XTS se define para cifradores de bloque
        }
    }
}
impl<'a> ModeWithTweak for XTS<'a> {
    fn tweak(&self) -> Bytes<'_> {
        self.tweak
    }
}

#[derive(Debug, Clone, Default)]
pub struct ECB;
impl Mode for ECB {
    fn name(&self) -> &'static str {
        "ECB"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_aes_key_length(self.name(), alg)
    }
}

#[derive(Debug, Clone)]
pub struct OFB<'a> {
    iv: Bytes<'a>,
}
impl<'a> OFB<'a> {
    pub fn new(iv: Bytes<'a>) -> Self {
        Self { iv }
    }
}
impl<'a> Mode for OFB<'a> {
    fn name(&self) -> &'static str {
        "OFB"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_iv_and_key_length(self.name(), self.iv, alg)
    }
}
impl<'a> ModeWithInitializationVector for OFB<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}

#[derive(Debug, Clone)]
pub struct CFB<'a> {
    iv: Bytes<'a>,
}
impl<'a> CFB<'a> {
    pub fn new(iv: Bytes<'a>) -> Self {
        Self { iv }
    }
}
impl<'a> Mode for CFB<'a> {
    fn name(&self) -> &'static str {
        "CFB"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_iv_and_key_length(self.name(), self.iv, alg)
    }
}
impl<'a> ModeWithInitializationVector for CFB<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}

#[derive(Debug, Clone)]
pub struct CFB8<'a> {
    iv: Bytes<'a>,
}
impl<'a> CFB8<'a> {
    pub fn new(iv: Bytes<'a>) -> Self {
        Self { iv }
    }
}
impl<'a> Mode for CFB8<'a> {
    fn name(&self) -> &'static str {
        "CFB8"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_iv_and_key_length(self.name(), self.iv, alg)
    }
}
impl<'a> ModeWithInitializationVector for CFB8<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}

#[derive(Debug, Clone)]
pub struct CTR<'a> {
    nonce: Bytes<'a>,
}
impl<'a> CTR<'a> {
    pub fn new(nonce: Bytes<'a>) -> Self {
        Self { nonce }
    }
}
impl<'a> Mode for CTR<'a> {
    fn name(&self) -> &'static str {
        "CTR"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_aes_key_length(self.name(), alg)?;
        check_nonce_length(self.name(), self.nonce, alg)
    }
}
impl<'a> ModeWithNonce for CTR<'a> {
    fn nonce(&self) -> Bytes<'_> {
        self.nonce
    }
}

#[derive(Debug, Clone)]
pub struct GCM<'a> {
    iv: Bytes<'a>,
    tag: Option<Bytes<'a>>,
    min_tag_length: usize,
}
impl<'a> GCM<'a> {
    pub fn new(
        iv: Bytes<'a>,
        tag: Option<Bytes<'a>>,
        min_tag_length: usize,
    ) -> Result<Self, ModeError> {
        if iv.len() < 8 || iv.len() > 128 {
            return Err(ModeError::GcmIvOutOfRange);
        }
        if let Some(t) = tag {
            if min_tag_length < 4 {
                return Err(ModeError::MinTagLengthTooSmall);
            }
            if t.len() < min_tag_length {
                return Err(ModeError::TagTooShort {
                    min: min_tag_length,
                });
            }
        }
        Ok(Self {
            iv,
            tag,
            min_tag_length,
        })
    }

    pub const MAX_ENCRYPTED_BYTES: u128 = ((1u128 << 39) - 256) / 8;
    pub const MAX_AAD_BYTES: u128 = (1u128 << 64) / 8;
}
impl<'a> Mode for GCM<'a> {
    fn name(&self) -> &'static str {
        "GCM"
    }
    fn validate_for_algorithm(&self, alg: &dyn CipherAlgorithm) -> Result<(), ModeError> {
        check_aes_key_length(self.name(), alg)?;
        let bs_bits = alg
            .block_size_bits()
            .ok_or(ModeError::BlockCipherRequired(self.name()))?;
        if let Some(tag) = self.tag {
            let block_size_bytes = bs_bits / 8;
            if tag.len() > block_size_bytes {
                return Err(ModeError::TagTooLong {
                    mode: self.name(),
                    max_tag_bytes: block_size_bytes,
                });
            }
        }
        Ok(())
    }
}
impl<'a> ModeWithInitializationVector for GCM<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}
impl<'a> ModeWithAuthenticationTag for GCM<'a> {
    fn tag(&self) -> Option<Bytes<'_>> {
        self.tag
    }
}

#[cfg(test)]
mod tests {
    use crate::kryptography::backends::symmetric::algorithms::{AES, AES256};

    use super::*;

    #[test]
    fn cbc_ok() {
        let mode = CBC::new(&[0u8; 16]);
        let aes = AES::new(&[0u8; 16]).unwrap();
        assert!(mode.validate_for_algorithm(&aes).is_ok());
    }

    #[test]
    fn ctr_nonce_bad_len() {
        let mode = CTR::new(&[0u8; 15]);
        let aes = AES::new(&[0u8; 16]).unwrap();
        let err = mode.validate_for_algorithm(&aes).unwrap_err();
        matches!(err, ModeError::InvalidNonce { .. });
    }

    #[test]
    fn xts_keysize() {
        let mode = XTS::new(&[0u8; 16]).unwrap();
        let aes_bad = AES::new(&[0u8; 16]).unwrap(); // 128 no es válido para XTS (debe ser 256/512)
        assert_eq!(
            mode.validate_for_algorithm(&aes_bad).unwrap_err(),
            ModeError::InvalidXtsKeySize
        );
        let aes_ok = AES::new(&[0u8; 32]).unwrap();
        assert!(mode.validate_for_algorithm(&aes_ok).is_ok());
    }

    #[test]
    fn gcm_tag_limits() {
        let mode = GCM::new(&[0u8; 12], Some(&[0u8; 17]), 16).unwrap();
        let aes = AES256::new(&[0u8; 32]).unwrap();
        let err = mode.validate_for_algorithm(&aes).unwrap_err();
        matches!(err, ModeError::TagTooLong { .. });
    }
}
