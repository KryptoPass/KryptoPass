//! Typed failures. Variants say what went wrong, not how to phrase it; the
//! `Display` text is English and meant for logs.

use std::{io, path::PathBuf, process::ExitStatus};

use crate::types::Fingerprint;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid input: {0}")]
    InvalidInput(#[from] InvalidInput),
    #[error("GnuPG {found} is not supported; {minimum} or later is required")]
    UnsupportedEngine {
        found: String,
        minimum: &'static str,
    },
    #[error("the operation was canceled in pinentry")]
    Canceled,
    #[error("no local key has primary fingerprint {0}")]
    KeyNotFound(Fingerprint),
    #[error("subkey {subkey} does not belong to {key}")]
    SubkeyNotFound {
        key: Fingerprint,
        subkey: Fingerprint,
    },
    #[error("the user ID does not belong to {0}")]
    UserIdNotFound(Fingerprint),
    #[error("{0} already carries that user ID")]
    UserIdAlreadyPresent(Fingerprint),
    #[error("the user ID is revoked")]
    UserIdRevoked,
    #[error("the user ID is invalid")]
    UserIdInvalid,
    #[error("{0} is already revoked")]
    AlreadyRevoked(Fingerprint),
    #[error("{0} is revoked, disabled or invalid")]
    KeyUnusable(Fingerprint),
    #[error("{0} is expired")]
    KeyExpired(Fingerprint),
    #[error("{0} has no secret primary key able to certify")]
    NotCertifier(Fingerprint),
    #[error("{0} has no local secret material")]
    NoSecretMaterial(Fingerprint),
    #[error("the keyring has no secret key")]
    NoSecretKey,
    /// Under [`Unlock::Cached`](crate::Unlock::Cached), the passphrase is not
    /// cached and asking for it is not allowed.
    #[error("the secret key is locked")]
    Locked,
    #[error("the passphrase is wrong")]
    BadPassphrase,
    #[error("{0} has no usable encryption subkey")]
    CannotEncrypt(Fingerprint),
    /// The person's own key is in the keyring with its secret part but not
    /// marked as theirs, so GnuPG will not encrypt to it.
    #[error("{0} is the user's key but is not trusted as such")]
    OwnKeyNotTrusted(Fingerprint),
    #[error("the data exceeds the size limit")]
    TooLarge,
    #[error("the ciphertext has no integrity protection")]
    UnprotectedCiphertext,
    #[error("the ciphertext uses an unsupported algorithm")]
    UnsupportedAlgorithm,
    #[error("{0} still has secret material; GnuPG cannot delete only its public part")]
    SecretMaterialPresent(Fingerprint),
    #[error("the input holds no OpenPGP keys or revocations GnuPG can import")]
    NothingToImport,
    #[error("the import changed nothing: {not_imported} not imported, {failed} failed")]
    ImportChangedNothing { not_imported: u32, failed: u32 },
    /// The primary key exists in the keyring even though this is an error.
    #[error("primary key {fingerprint} was created, but its encryption subkey failed")]
    EncryptionSubkeyFailed {
        fingerprint: Fingerprint,
        #[source]
        source: Backend,
    },
    #[error("malformed revocation certificate: {0}")]
    MalformedRevocation(&'static str),
    #[error("GnuPG did not confirm the result: {0}")]
    Unconfirmed(Unconfirmed),
    #[error("GnuPG failed to {step}")]
    Gpg {
        step: Step,
        #[source]
        source: Backend,
    },
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Command(#[from] CommandError),
}

/// A value rejected before GnuPG is involved.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidInput {
    #[error("a fingerprint needs 40 or 64 hexadecimal digits")]
    Fingerprint,
    #[error("a user ID needs 1 to 255 bytes without control characters")]
    UserId,
    #[error("the expiry date is beyond what GnuPG can store")]
    ExpiryTooFar,
    #[error("a keyserver needs an hkp, hkps, http or https URL with a host and no credentials")]
    Keyserver,
    #[error("the path is empty, too long or not valid UTF-8")]
    Path,
    #[error("at most 64 subkeys can change expiry at once")]
    TooManySubkeys,
    #[error("the subkey list repeats a fingerprint")]
    DuplicateSubkey,
    #[error("certify between 1 and 64 user IDs")]
    UserIdCount,
    #[error("the user ID list repeats an entry")]
    DuplicateUserId,
    #[error("a key cannot certify itself")]
    SelfCertification,
    #[error("encrypt to between 1 and 64 recipients")]
    RecipientCount,
    #[error("the ciphertext is empty")]
    EmptyCiphertext,
}

/// GnuPG reported success, but reading the keyring back disagreed.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum Unconfirmed {
    #[error("created key {0} is missing from the keyring")]
    CreatedKeyMissing(Fingerprint),
    #[error("{0} is not revoked")]
    KeyNotRevoked(Fingerprint),
    #[error("subkey {0} is not revoked")]
    SubkeyNotRevoked(Fingerprint),
    #[error("{0} is still in the keyring")]
    KeyStillPresent(Fingerprint),
    #[error("secret material of {0} is still present")]
    SecretStillPresent(Fingerprint),
    #[error("the public part of {0} was deleted too")]
    PublicPartDeleted(Fingerprint),
    #[error("the source did not return {0}")]
    NotFetched(Fingerprint),
    #[error("GnuPG did not record the ownertrust of {0}")]
    OwnerTrustNotApplied(Fingerprint),
    #[error("GnuPG returned no fingerprint for a new key")]
    MissingFingerprint,
    #[error("GnuPG produced no data")]
    EmptyOutput,
    #[error("the exported public key is not UTF-8")]
    NotUtf8,
}

/// The GPGME call that failed, so callers can say which step broke.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Step {
    OpenContext,
    ListKeys,
    CreateKey,
    Import,
    Export,
    ChangePassphrase,
    SetExpiry,
    AddUserId,
    SetPrimaryUserId,
    RevokeUserId,
    AddSubkey,
    RevokeSubkey,
    SelectSigner,
    Certify,
    Sign,
    Delete,
    Encrypt,
    Decrypt,
}

impl std::fmt::Display for Step {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::OpenContext => "open a context",
            Self::ListKeys => "list keys",
            Self::CreateKey => "create the key",
            Self::Import => "import",
            Self::Export => "export",
            Self::ChangePassphrase => "change the passphrase",
            Self::SetExpiry => "set the expiry",
            Self::AddUserId => "add the user ID",
            Self::SetPrimaryUserId => "set the primary user ID",
            Self::RevokeUserId => "revoke the user ID",
            Self::AddSubkey => "add the subkey",
            Self::RevokeSubkey => "revoke the subkey",
            Self::SelectSigner => "select the signing key",
            Self::Certify => "certify",
            Self::Sign => "sign",
            Self::Delete => "delete",
            Self::Encrypt => "encrypt",
            Self::Decrypt => "decrypt",
        })
    }
}

/// An opaque GPGME failure; it keeps GPGME out of the public signatures.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Backend(pub(crate) gpgme::Error);

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileError {
    #[error("cannot open {}", .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot read {}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a regular local file", .0.display())]
    NotRegular(PathBuf),
    #[error("{} exceeds the size limit", .0.display())]
    TooLarge(PathBuf),
    #[error("{} does not name a file", .0.display())]
    NotAFile(PathBuf),
    #[error("cannot use {} as destination folder", .path.display())]
    Destination {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} already exists and will not be overwritten", .0.display())]
    DestinationExists(PathBuf),
    #[error("{} has a database extension", .0.display())]
    DatabaseExtension(PathBuf),
    #[error("{} is inside the GnuPG home", .0.display())]
    InsideGnupgHome(PathBuf),
    #[error("cannot write {}", .path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot create a private temporary file")]
    Temporary(#[source] io::Error),
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CommandError {
    #[error("cannot start gpg")]
    Spawn(#[source] io::Error),
    #[error("cannot talk to gpg")]
    Io(#[source] io::Error),
    #[error("gpg exceeded the time limit")]
    Timeout,
    #[error("gpg exited with {0}")]
    Exit(ExitStatus),
    #[error("unexpected gpg dialogue: {0}")]
    Protocol(&'static str),
}

/// Maps a GPGME error, turning a dismissed pinentry into [`Error::Canceled`].
pub(crate) fn gpg(step: Step) -> impl FnOnce(gpgme::Error) -> Error {
    move |source| {
        if source.code() == gpgme::Error::CANCELED.code() {
            Error::Canceled
        } else {
            Error::Gpg {
                step,
                source: Backend(source),
            }
        }
    }
}
