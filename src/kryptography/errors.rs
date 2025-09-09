use thiserror::Error;

#[derive(Debug, Error)]
#[error("Longitud inválida: se esperaban {expected} bytes, se obtuvieron {got}")]
pub struct LengthError {
    pub expected: usize,
    pub got: usize,
}

#[derive(Debug, Error)]
pub enum AeadError {
    #[error("Entrada inválida: {0}")]
    Invalid(&'static str),
    #[error("Fallo de backend AEAD: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("Entrada inválida: {0}")]
    Invalid(&'static str),
    #[error("Fallo de backend Stream: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum BlockModeError {
    #[error("Entrada inválida: {0}")]
    Invalid(&'static str),
    #[error("Fallo de backend BlockMode: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum FactoryError {
    #[error("Algoritmo no soportado por backend {backend}: {algo}")]
    Unsupported {
        algo: &'static str,
        backend: &'static str,
    },
    #[error("Algoritmo no implementado aún en backend {backend}: {algo}")]
    NotImplemented {
        algo: &'static str,
        backend: &'static str,
    },
    #[error("Parse de algoritmo inválido: {0}")]
    Parse(String),
}
