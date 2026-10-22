#![allow(clippy::expect_used)] // Filesystem fixtures should name the failing setup step.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use kryptopass_identity::{DeviceId, DeviceIdentity, PublicIdentity};
use kryptopass_sync::{Id, Ledger, Operation, SyncError, TrustSource};
use rusqlite::{Connection, params};

const VAULT: Id = [0x38; 16];

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let suffix = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("kp-sync-files-{}-{suffix}", std::process::id()));
        std::fs::create_dir(&path).expect("create isolated test directory");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .expect("make test directory private");
        }
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Trust(PublicIdentity);

impl TrustSource for Trust {
    fn lookup(&self, device_id: DeviceId) -> Result<Option<PublicIdentity>, SyncError> {
        Ok((self.0.device_id == device_id).then_some(self.0))
    }
}

fn identity() -> DeviceIdentity {
    DeviceIdentity::from_seed_material(&[0x27; 32], &[0x39; 32])
}

#[cfg(unix)]
#[test]
fn created_ledger_is_private_and_existing_paths_are_never_replaced() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let directory = TempDirectory::new();
    let path = directory.file("vault.sqlite");
    let existing = directory.file("existing.bin");
    let link = directory.file("linked.sqlite");
    std::fs::write(&existing, b"keep this file").expect("existing fixture");

    let ledger = Ledger::create(&path, VAULT).expect("create private ledger");
    drop(ledger);
    assert_eq!(
        std::fs::metadata(&path)
            .expect("ledger file exists")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    assert!(matches!(
        Ledger::create(&existing, VAULT),
        Err(SyncError::Storage | SyncError::InvalidLedger)
    ));
    assert_eq!(
        std::fs::read(&existing).expect("existing file survives"),
        b"keep this file"
    );

    symlink(&existing, &link).expect("create symlink fixture");
    assert!(matches!(
        Ledger::create(&link, VAULT),
        Err(SyncError::Storage | SyncError::InvalidLedger)
    ));
    assert!(matches!(
        Ledger::open_existing(&link, VAULT, &Trust(identity().public_identity())),
        Err(SyncError::InvalidLedger)
    ));
    assert_eq!(
        std::fs::read(&existing).expect("symlink target survives"),
        b"keep this file"
    );
}

#[cfg(unix)]
#[test]
fn ledger_creation_rejects_a_shared_parent_directory() {
    use std::os::unix::fs::PermissionsExt;

    let path = std::env::temp_dir().join(format!(
        "kp-sync-public-parent-{}.sqlite",
        std::process::id()
    ));
    assert_eq!(
        std::fs::metadata(std::env::temp_dir())
            .expect("temporary directory exists")
            .permissions()
            .mode()
            & 0o077,
        0o077,
        "test requires the usual shared temporary directory"
    );
    assert!(matches!(
        Ledger::create(&path, VAULT),
        Err(SyncError::InvalidLedger)
    ));
    assert!(
        !Path::new(&path).exists(),
        "failed creation removes the file"
    );
}

#[test]
fn reopening_rejects_a_database_whose_signed_ciphertext_was_modified() {
    let directory = TempDirectory::new();
    let path = directory.file("tampered.sqlite");
    let identity = identity();
    let trust = Trust(identity.public_identity());
    let revision_id;
    {
        let mut ledger = Ledger::create(&path, VAULT).expect("create private ledger");
        let revision = ledger
            .append_local(
                &identity,
                [0x49; 16],
                &[],
                Operation::Write,
                b"original".to_vec(),
                &trust,
            )
            .expect("store signed ciphertext");
        revision_id = revision.revision_id;
    }

    let connection = Connection::open(&path).expect("open fixture for tampering");
    connection
        .execute(
            "UPDATE revisions SET ciphertext = ?1 WHERE revision_id = ?2",
            params![b"modified", revision_id.as_slice()],
        )
        .expect("tamper with stored ciphertext");
    drop(connection);

    assert!(matches!(
        Ledger::open_existing(&path, VAULT, &trust),
        Err(SyncError::InvalidLedger)
    ));
}
