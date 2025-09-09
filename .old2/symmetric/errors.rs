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

    #[error("Etiqueta inválida (tamaño {got} bytes) para {mode}")]
    InvalidTag { mode: &'static str, got: usize },

    #[error("nonce inválido (tamaño {got} bytes) para {mode}; se esperaba {expected_bits} bits")]
    InvalidNonce {
        mode: &'static str,
        got: usize,
        expected_bits: usize,
    },

    #[error("{mode}: {param} inválido (tamaño {got_bits} bits); permitidos: {allowed:?}")]
    InvalidParameterLength {
        mode: &'static str,
        param: &'static str,
        got_bits: usize,
        allowed: &'static [usize],
    },
}
