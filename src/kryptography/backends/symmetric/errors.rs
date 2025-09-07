#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AlgorithmError {
    #[error("tamaño de clave inválido: {got_bits} bits; permitidos: {allowed:?}")]
    InvalidKeySize {
        allowed: &'static [usize],
        got_bits: usize,
    },

    #[error("nonce debe ser de 128 bits (16 bytes)")]
    NonceMustBe16,

    #[error("clave ausente o vacía")]
    EmptyKey,
}

/// Errores de validación de modos.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModeError {
    #[error("solo se permiten claves AES de 128/192/256 bits para {0}")]
    AesKeySize(&'static str),

    #[error("{0} requiere un cifrador de bloque")]
    BlockCipherRequired(&'static str),

    #[error("IV inválido (tamaño {got} bytes) para {mode}; se esperaba {expected_bits} bits")]
    InvalidIv {
        mode: &'static str,
        got: usize,
        expected_bits: usize,
    },

    #[error("nonce inválido (tamaño {got} bytes) para {mode}; se esperaba {expected_bits} bits")]
    InvalidNonce {
        mode: &'static str,
        got: usize,
        expected_bits: usize,
    },

    #[error("tweak debe ser de 128 bits (16 bytes)")]
    InvalidTweak,

    #[error("XTS requiere clave de 256 bits (AES-128-XTS) o 512 bits (AES-256-XTS)")]
    InvalidXtsKeySize,

    #[error("para {mode}, la etiqueta no puede superar {max_tag_bytes} bytes")]
    TagTooLong {
        mode: &'static str,
        max_tag_bytes: usize,
    },

    #[error("min_tag_length debe ser >= 4")]
    MinTagLengthTooSmall,

    #[error("autenticación: la etiqueta debe tener al menos {min} bytes")]
    TagTooShort { min: usize },

    #[error("GCM: el IV debe tener entre 8 y 128 bytes")]
    GcmIvOutOfRange,
}
