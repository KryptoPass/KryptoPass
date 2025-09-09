use crate::kryptography::backends::symmetric::{
    algorithms::BlockCipherAlgorithm, errors::ModeError,
};

pub type Bytes<'a> = &'a [u8];

/// Contrato base de un modo.
pub trait Mode {
    fn name(&self) -> &'static str;
    fn validate_for_algorithm(&self, alg: &dyn BlockCipherAlgorithm) -> Result<(), ModeError>;
}

pub trait ModeWithInitializationVector: Mode {
    fn initialization_vector(&self) -> Bytes<'_>;
}

pub trait ModeWithAuthenticationTag: Mode {
    fn tag(&self) -> Option<Bytes<'_>>;
}

pub trait ModeWithNonce: Mode {
    fn nonce(&self) -> Bytes<'_>;
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
    fn validate_for_algorithm(&self, alg: &dyn BlockCipherAlgorithm) -> Result<(), ModeError> {
        let bs = alg
            .block_size_bits()
            .ok_or(ModeError::BlockCipherRequired(self.name()))?;

        if self.iv.len() * 8 != bs {
            return Err(ModeError::InvalidIv {
                mode: self.name(),
                got: self.iv.len(),
                expected_bits: bs,
            });
        }

        Ok(())
    }
}
impl<'a> ModeWithInitializationVector for CBC<'a> {
    fn initialization_vector(&self) -> Bytes<'_> {
        self.iv
    }
}

#[derive(Debug, Clone)]
pub struct GCM<'a> {
    nonce: Bytes<'a>,
    tag: Option<Bytes<'a>>,
}

impl<'a> Mode for GCM<'a> {
    fn name(&self) -> &'static str {
        "GCM"
    }

    fn validate_for_algorithm(&self, alg: &dyn BlockCipherAlgorithm) -> Result<(), ModeError> {
        let _bs = alg
            .block_size_bits()
            .ok_or(ModeError::BlockCipherRequired(self.name()))?;

        let nlen = self.nonce.len();
        if nlen != 12 {
            // estrictos (96 bits)
            return Err(ModeError::InvalidNonce {
                mode: self.name(),
                got: nlen,
                expected_bits: 96,
            });
        }

        if let Some(tag) = self.tag {
            if tag.len() < 12 || tag.len() > 16 {
                return Err(ModeError::InvalidTag {
                    mode: self.name(),
                    got: tag.len(),
                });
            }
        }

        Ok(())
    }
}

impl<'a> ModeWithNonce for GCM<'a> {
    fn nonce(&self) -> Bytes<'_> {
        self.nonce
    }
}

impl<'a> ModeWithAuthenticationTag for GCM<'a> {
    fn tag(&self) -> Option<Bytes<'_>> {
        self.tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kryptography::backends::symmetric::algorithms::AES;

    #[test]
    fn cbc_ok_with_aes_iv_16_bytes() {
        let aes = AES::default();
        let iv = [0u8; 16];
        let cbc = CBC::new(&iv);
        assert!(cbc.validate_for_algorithm(&aes).is_ok());
    }

    #[test]
    fn cbc_err_with_aes_iv_wrong_len() {
        let aes = AES::default();
        let iv = [0u8; 8]; // 64 bits
        let cbc = CBC::new(&iv);
        assert!(cbc.validate_for_algorithm(&aes).is_err());
    }

    #[test]
    fn gcm_ok_nonce_12_tag_16() {
        let aes = AES::default();
        let nonce = [0u8; 12];
        let tag = [0u8; 16];
        let gcm = GCM {
            nonce: &nonce,
            tag: Some(&tag),
        };
        assert!(gcm.validate_for_algorithm(&aes).is_ok());
    }

    #[test]
    fn gcm_err_nonce_not_12() {
        let aes = AES::default();
        let nonce = [0u8; 8];
        let gcm = GCM {
            nonce: &nonce,
            tag: None,
        };
        assert!(gcm.validate_for_algorithm(&aes).is_err());
    }
}
