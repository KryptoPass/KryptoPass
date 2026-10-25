//! A catalogue of keyring operations and who should own each one.

/// Every mutating operation [`crate::Keyring`] offers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operation {
    CreateKey,
    Import,
    ExportPublic,
    BackupSecret,
    Fetch,
    ChangePassphrase,
    SetExpiry,
    AddUserId,
    SetPrimaryUserId,
    RevokeUserId,
    AddSubkey,
    RevokeSubkey,
    WriteRevocationCertificate,
    RevokeKey,
    Certify,
    Delete,
    MarkAsOwn,
}

/// Why an operation exists in KryptoPass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// The vault cannot be set up, shared or recovered without it.
    Vault,
    /// General key management, which a dedicated manager such as Kleopatra
    /// can take over when one is installed.
    KeyManagement,
}

impl Operation {
    pub const ALL: [Self; 17] = [
        Self::CreateKey,
        Self::Import,
        Self::ExportPublic,
        Self::BackupSecret,
        Self::Fetch,
        Self::ChangePassphrase,
        Self::SetExpiry,
        Self::AddUserId,
        Self::SetPrimaryUserId,
        Self::RevokeUserId,
        Self::AddSubkey,
        Self::RevokeSubkey,
        Self::WriteRevocationCertificate,
        Self::RevokeKey,
        Self::Certify,
        Self::Delete,
        Self::MarkAsOwn,
    ];

    pub const fn scope(self) -> Scope {
        match self {
            Self::CreateKey
            | Self::Import
            | Self::ExportPublic
            | Self::BackupSecret
            | Self::Fetch
            | Self::MarkAsOwn => Scope::Vault,
            Self::ChangePassphrase
            | Self::SetExpiry
            | Self::AddUserId
            | Self::SetPrimaryUserId
            | Self::RevokeUserId
            | Self::AddSubkey
            | Self::RevokeSubkey
            | Self::WriteRevocationCertificate
            | Self::RevokeKey
            | Self::Certify
            | Self::Delete => Scope::KeyManagement,
        }
    }

    /// Whether the operation can destroy access to existing data, so callers
    /// should confirm it, all the more for the key that opens the vault.
    pub const fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::RevokeUserId | Self::RevokeSubkey | Self::RevokeKey | Self::Delete
        )
    }
}
