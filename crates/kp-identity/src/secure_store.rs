use kryptopass_secure_memory::SensitiveBytes;
use thiserror::Error;

/// Result of a create-only secure-store write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreIfAbsent {
    /// This call stored the value.
    Stored,
    /// A value was already present and was left untouched.
    AlreadyExists,
}

/// Synchronous secure storage for one profile's versioned identity record.
///
/// Implementations must make [`Self::store_if_absent`] atomic with respect to
/// other writers for the same account. Implementations must never expose the
/// supplied secret through errors or debugging output.
pub trait SecureKeyStore: Send + Sync {
    /// Loads the existing secret into a zeroizing buffer, or returns `None`.
    fn load(&self) -> Result<Option<SensitiveBytes>, SecureStoreError>;

    /// Stores the value only if no value already exists for this account.
    fn store_if_absent(&self, secret: &[u8]) -> Result<StoreIfAbsent, SecureStoreError>;
}

/// Errors from operating-system secure storage.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SecureStoreError {
    /// The platform does not provide the configured backend.
    #[error("the configured secure-store backend is unsupported on this platform")]
    UnsupportedPlatform,
    /// The Secret Service or its credential store is unavailable.
    #[error("the operating-system secure store is unavailable")]
    Unavailable,
    /// The caller cannot read or write this credential.
    #[error("access to the operating-system secure store was denied")]
    AccessDenied,
    /// The profile identifier or private profile directory is invalid.
    #[error("invalid secure-store profile configuration")]
    InvalidProfile,
    /// A non-classified operating-system backend operation failed.
    #[error("the operating-system secure-store operation failed")]
    BackendFailure,
}

/// Operating-system key storage, bound to one application profile.
///
/// On Linux this uses only the Secret Service backend with the `crypto-rust`
/// implementation. Operations are synchronous and blocking; call them from a
/// worker thread rather than an async runtime worker.
#[cfg(target_os = "linux")]
pub struct SystemSecretKeyStore {
    account: String,
    profile_dir: std::path::PathBuf,
}

/// Operating-system key storage on unsupported platforms fails closed.
#[cfg(not(target_os = "linux"))]
pub struct SystemSecretKeyStore;

#[cfg(target_os = "linux")]
impl SystemSecretKeyStore {
    /// Opens the keyring account and profile-scoped interprocess creation lock.
    pub fn new(
        profile_dir: impl AsRef<std::path::Path>,
        profile_id: impl Into<String>,
    ) -> Result<Self, SecureStoreError> {
        let profile_id = profile_id.into();
        if profile_id.is_empty()
            || profile_id.len() > 128
            || !profile_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        {
            return Err(SecureStoreError::InvalidProfile);
        }
        validate_private_profile_dir(profile_dir.as_ref())?;
        let profile_dir = std::fs::canonicalize(profile_dir.as_ref())
            .map_err(|_| SecureStoreError::InvalidProfile)?;
        validate_private_profile_dir(&profile_dir)?;
        let account = profile_account(&profile_dir, &profile_id);
        Ok(Self {
            account,
            profile_dir,
        })
    }

    fn entry(&self) -> Result<keyring::Entry, SecureStoreError> {
        let credential = keyring::secret_service::SsCredential::new_with_target(
            Some("default"),
            "org.kryptopass.device-identity.v1",
            &self.account,
        )
        .map_err(map_keyring_error)?;
        Ok(keyring::Entry::new_with_credential(Box::new(credential)))
    }

    fn profile_lock(&self) -> Result<std::fs::File, SecureStoreError> {
        use std::os::unix::fs::MetadataExt;

        validate_private_profile_dir(&self.profile_dir)?;
        let path = self.profile_dir.join(".device-identity.lock");
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDWR
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::CLOEXEC
                | rustix::fs::OFlags::NOFOLLOW,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .map_err(|_| SecureStoreError::InvalidProfile)?;
        let file = std::fs::File::from(fd);
        let metadata = file
            .metadata()
            .map_err(|_| SecureStoreError::InvalidProfile)?;
        if !metadata.is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.nlink() != 1
            || metadata.mode() & 0o777 != 0o600
        {
            return Err(SecureStoreError::InvalidProfile);
        }
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(|_| SecureStoreError::BackendFailure)?;
        Ok(file)
    }
}

#[cfg(target_os = "linux")]
impl SecureKeyStore for SystemSecretKeyStore {
    fn load(&self) -> Result<Option<SensitiveBytes>, SecureStoreError> {
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(secret) if secret.len() <= 4096 => Ok(Some(SensitiveBytes::from(secret))),
            Ok(mut secret) => {
                zeroize::Zeroize::zeroize(&mut secret);
                Err(SecureStoreError::BackendFailure)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn store_if_absent(&self, secret: &[u8]) -> Result<StoreIfAbsent, SecureStoreError> {
        let _lock = self.profile_lock()?;
        let entry = self.entry()?;
        match entry.get_secret() {
            Ok(mut existing) => {
                zeroize::Zeroize::zeroize(&mut existing);
                return Ok(StoreIfAbsent::AlreadyExists);
            }
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(map_keyring_error(error)),
        }
        entry.set_secret(secret).map_err(map_keyring_error)?;
        Ok(StoreIfAbsent::Stored)
    }
}

#[cfg(not(target_os = "linux"))]
impl SystemSecretKeyStore {
    /// Returns the unsupported-platform error without creating a fallback store.
    pub fn new(
        _profile_dir: impl AsRef<std::path::Path>,
        _profile_id: impl Into<String>,
    ) -> Result<Self, SecureStoreError> {
        Err(SecureStoreError::UnsupportedPlatform)
    }
}

#[cfg(not(target_os = "linux"))]
impl SecureKeyStore for SystemSecretKeyStore {
    fn load(&self) -> Result<Option<SensitiveBytes>, SecureStoreError> {
        Err(SecureStoreError::UnsupportedPlatform)
    }

    fn store_if_absent(&self, _secret: &[u8]) -> Result<StoreIfAbsent, SecureStoreError> {
        Err(SecureStoreError::UnsupportedPlatform)
    }
}

#[cfg(target_os = "linux")]
fn validate_private_profile_dir(path: &std::path::Path) -> Result<(), SecureStoreError> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::symlink_metadata(path).map_err(|_| SecureStoreError::InvalidProfile)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(SecureStoreError::InvalidProfile);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn map_keyring_error(error: keyring::Error) -> SecureStoreError {
    match error {
        keyring::Error::NoStorageAccess(_) => SecureStoreError::AccessDenied,
        keyring::Error::PlatformFailure(_) => SecureStoreError::Unavailable,
        _ => SecureStoreError::BackendFailure,
    }
}

#[cfg(target_os = "linux")]
fn profile_account(profile_dir: &std::path::Path, profile_id: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::os::unix::ffi::OsStrExt;

    let mut digest = Sha256::new();
    digest.update(b"org.kryptopass.secret-service-account.v1\0");
    digest.update(profile_dir.as_os_str().as_bytes());
    digest.update([0]);
    digest.update(profile_id.as_bytes());
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
