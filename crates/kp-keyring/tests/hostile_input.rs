//! Inputs an attacker or a mistake could supply: each must be refused before
//! it reaches GnuPG or the filesystem, without changing the keyring.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::{fs, os::unix::fs::symlink};

use kryptopass_keyring::{Error, FileError, Fingerprint, InvalidInput, Keyserver, UserId};
use support::GnupgHome;

#[test]
fn values_are_validated_before_any_operation() {
    let v4 = "a".repeat(40);
    assert_eq!(Fingerprint::parse(&v4).unwrap().as_str(), "A".repeat(40));
    assert!(Fingerprint::parse(&"a".repeat(64)).is_ok());
    for bad in [
        "",
        "ABCD",
        &"G".repeat(40),
        &format!("{v4} "),
        &"a".repeat(41),
    ] {
        assert_eq!(
            Fingerprint::parse(bad),
            Err(InvalidInput::Fingerprint),
            "{bad:?}"
        );
    }

    assert!(UserId::parse("Ada <ada@example.org>").is_ok());
    for bad in [
        "",
        "   ",
        "Ada\n<ada@example.org>",
        "Ada\0",
        &"x".repeat(256),
    ] {
        assert_eq!(UserId::parse(bad), Err(InvalidInput::UserId), "{bad:?}");
    }

    assert!(Keyserver::parse("hkps://keys.openpgp.org").is_ok());
    for bad in [
        "hkps://user:secret@keys.openpgp.org",
        "file:///etc/passwd",
        "hkps://keys.openpgp.org --exec x",
        "keys.openpgp.org",
    ] {
        assert_eq!(
            Keyserver::parse(bad),
            Err(InvalidInput::Keyserver),
            "{bad:?}"
        );
    }
}

#[test]
fn import_refuses_garbage_symlinks_and_oversized_files() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let outbox = home.outbox();

    assert!(matches!(
        keyring.import(b"definitely not OpenPGP"),
        Err(Error::NothingToImport)
    ));

    // A symlink could redirect the read to a file the user never chose.
    let target = outbox.join("real.asc");
    fs::write(&target, "irrelevant").unwrap();
    let link = outbox.join("link.asc");
    symlink(&target, &link).unwrap();
    assert!(matches!(
        keyring.import_file(&link),
        Err(Error::File(FileError::Open { .. }))
    ));

    // Sparse, so the test is cheap; the size check must fire before reading.
    let huge = outbox.join("huge.asc");
    fs::File::create(&huge)
        .unwrap()
        .set_len(33 * 1024 * 1024)
        .unwrap();
    assert!(matches!(
        keyring.import_file(&huge),
        Err(Error::File(FileError::TooLarge(_)))
    ));

    assert!(keyring.list_keys().unwrap().is_empty());
}

#[test]
fn exports_refuse_destinations_that_could_damage_data() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let fingerprint = home.create("Ada <ada@example.org>");
    let outbox = home.outbox();

    // Writing into the GnuPG home could plant files gpg would later trust.
    let inside = home.path().join("planted.asc");
    assert!(matches!(
        keyring.backup_secret_to(&fingerprint, &inside),
        Err(Error::File(FileError::InsideGnupgHome(_)))
    ));
    assert!(!inside.exists());

    // A key dump must never land on a vault database.
    let database = outbox.join("vault.sqlite");
    assert!(matches!(
        keyring.export_public_to(&fingerprint, &database),
        Err(Error::File(FileError::DatabaseExtension(_)))
    ));

    // An existing file, even a dangling symlink, is never replaced.
    let dangling = outbox.join("dangling.asc");
    symlink(outbox.join("missing"), &dangling).unwrap();
    assert!(matches!(
        keyring.backup_secret_to(&fingerprint, &dangling),
        Err(Error::File(FileError::DestinationExists(_)))
    ));
    assert!(
        !outbox.join("missing").exists(),
        "nothing written through the link"
    );

    // No temporary leftovers after the refusals.
    let leftovers: Vec<_> = fs::read_dir(&outbox)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().contains("kryptopass-export"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}
