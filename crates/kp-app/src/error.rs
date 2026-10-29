//! Failures of application operations.

use kp_template::LibraryError;
use kryptopass_core::ActionError;
use kryptopass_core::vault::VaultError;
use kryptopass_generator::GeneratorError;
use thiserror::Error;

/// Why an application operation failed. Like the layers below, it never
/// carries a stored value; [`AppError::code`] is what the UI translates.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("the vault store could not be opened")]
    Store(#[source] anyhow::Error),
    #[error("the templates could not be loaded")]
    Templates(#[from] LibraryError),
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("keyring operation failed")]
    Keyring(#[from] kryptopass_keyring::Error),
    #[error(transparent)]
    Generator(#[from] GeneratorError),
    #[error("the template has no such field")]
    UnknownField,
    #[error("the field has no generator")]
    NoGenerator,
    #[error("the value does not fit the field")]
    WrongShape,
    #[error("the item cannot be filled into a browser")]
    NotFillable,
    #[error("the one-time code configuration is invalid")]
    InvalidTotp,
    #[error(transparent)]
    Action(ActionError),
}

impl AppError {
    /// Stable, language-neutral code for the UI to translate.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Store(_) => "store-unavailable",
            Self::Templates(_) => "templates-unavailable",
            Self::Vault(error) => error.code(),
            Self::Keyring(kryptopass_keyring::Error::Canceled) => "canceled",
            Self::Keyring(_) => "keyring-failed",
            Self::Generator(_) => "generator-failed",
            Self::UnknownField => "unknown-field",
            Self::NoGenerator => "no-generator",
            Self::WrongShape => "wrong-shape",
            Self::NotFillable => "not-fillable",
            Self::InvalidTotp => "invalid-totp",
            Self::Action(ActionError::Unsupported) => "action-unsupported",
            Self::Action(_) => "action-failed",
        }
    }
}
