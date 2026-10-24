//! End-to-end flows against a real GnuPG: each test performs the steps a
//! user would and checks the keyring state after each one.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::fs;

use kryptopass_keyring::{
    CertificationKind, DeleteScope, Error, Expiry, ExpiryTarget, FileError, SubkeyUsage,
};
use support::{GnupgHome, Pinentry, mode, user_id};

/// Set up a vault key, share it, back it up and recover it elsewhere.
#[test]
fn vault_key_survives_backup_and_restore_on_another_machine() {
    let laptop = GnupgHome::new();
    let keyring = laptop.keyring();

    let fingerprint = laptop.create("Ada <ada@example.org>");
    let key = laptop.key(&fingerprint);
    assert!(key.has_secret);
    assert!(key.can_encrypt, "the vault needs an encryption subkey");
    assert_eq!(key.user_ids[0].email.as_deref(), Some("ada@example.org"));
    keyring.probe_agent().expect("the new secret key can sign");

    let armored = keyring.public_key_armored(&fingerprint).unwrap();
    assert!(armored.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----"));

    let outbox = laptop.outbox();
    let backup = outbox.join("ada-secret.asc");
    let published = keyring.backup_secret_to(&fingerprint, &backup).unwrap();
    assert_eq!(published.path, backup);
    assert_eq!(mode(&backup), 0o600, "a secret backup must be owner-only");

    // A second backup to the same path must not replace the first one.
    let first = fs::read(&backup).unwrap();
    let again = keyring.backup_secret_to(&fingerprint, &backup);
    assert!(matches!(
        again,
        Err(Error::File(FileError::DestinationExists(_)))
    ));
    assert_eq!(fs::read(&backup).unwrap(), first);

    // A friend who only receives the public key can encrypt to it, nothing more.
    let friend = GnupgHome::new();
    let report = friend.keyring().import(armored.as_bytes()).unwrap();
    assert_eq!(report.first_key.as_ref(), Some(&fingerprint));
    assert!(!friend.key(&fingerprint).has_secret);
    let repeated = friend.keyring().import(armored.as_bytes()).unwrap();
    assert_eq!((repeated.imported, repeated.unchanged), (0, 1));

    // A new machine restoring the backup gets the full key back.
    let new_machine = GnupgHome::new();
    new_machine.keyring().import_file(&backup).unwrap();
    assert!(new_machine.key(&fingerprint).has_secret);
    new_machine.keyring().probe_agent().unwrap();
}

/// Edit identities and subkeys, checking each change landed in the keyring.
#[test]
fn identities_and_subkeys_change_as_requested() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let fingerprint = home.create("Ada <ada@example.org>");
    let work = user_id("Ada Work <ada@work.example>");

    keyring.add_user_id(&fingerprint, &work).unwrap();
    assert_eq!(home.key(&fingerprint).user_ids.len(), 2);
    assert!(matches!(
        keyring.add_user_id(&fingerprint, &work),
        Err(Error::UserIdAlreadyPresent(_))
    ));

    keyring.set_primary_user_id(&fingerprint, &work).unwrap();
    assert_eq!(home.key(&fingerprint).user_ids[0].id, work.as_str());

    let subkeys_before = home.key(&fingerprint).subkeys.len();
    keyring
        .add_subkey(&fingerprint, SubkeyUsage::Sign, Expiry::Never)
        .unwrap();
    let key = home.key(&fingerprint);
    assert_eq!(key.subkeys.len(), subkeys_before + 1);
    let signing = key.subkeys.last().unwrap().clone();
    assert!(signing.can_sign && signing.expires_at.is_none());
    let signing_fpr = signing.fingerprint.unwrap();

    keyring
        .set_expiry(
            &fingerprint,
            &ExpiryTarget::Subkeys(vec![signing_fpr.clone()]),
            Expiry::from_days(30),
        )
        .unwrap();
    let key = home.key(&fingerprint);
    let signing = key.subkeys.last().unwrap();
    assert!(signing.expires_at.is_some(), "subkey got an expiry");
    assert!(
        key.subkeys[0].expires_at.is_none(),
        "primary left untouched"
    );

    keyring.revoke_subkey(&fingerprint, &signing_fpr).unwrap();
    assert!(home.key(&fingerprint).subkeys.last().unwrap().revoked);
    assert!(matches!(
        keyring.revoke_subkey(&fingerprint, &signing_fpr),
        Err(Error::AlreadyRevoked(_))
    ));

    keyring.revoke_user_id(&fingerprint, &work).unwrap();
    let key = home.key(&fingerprint);
    assert!(
        key.user_ids
            .iter()
            .any(|uid| uid.id == work.as_str() && uid.revoked)
    );
    assert!(matches!(
        keyring.set_primary_user_id(&fingerprint, &work),
        Err(Error::UserIdRevoked)
    ));
}

/// A saved revocation certificate really revokes the key where it is imported.
#[test]
fn revocation_certificate_revokes_the_key_elsewhere() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let fingerprint = home.create("Ada <ada@example.org>");
    let public = keyring.public_key_armored(&fingerprint).unwrap();

    let certificate = home.outbox().join("ada.rev");
    keyring
        .write_revocation_certificate(&fingerprint, &certificate)
        .unwrap();
    assert_eq!(mode(&certificate), 0o600);
    assert!(!home.key(&fingerprint).revoked, "saving is not revoking");

    let contact = GnupgHome::new();
    contact.keyring().import(public.as_bytes()).unwrap();
    contact.keyring().import_file(&certificate).unwrap();
    assert!(contact.key(&fingerprint).revoked);

    keyring.revoke_key(&fingerprint).unwrap();
    assert!(home.key(&fingerprint).revoked);
    assert!(matches!(
        keyring.add_user_id(&fingerprint, &user_id("Late <late@example.org>")),
        Err(Error::KeyUnusable(_))
    ));
}

/// Each delete scope removes exactly what it says and refuses the rest.
#[test]
fn delete_scopes_remove_only_what_they_name() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let fingerprint = home.create("Ada <ada@example.org>");

    assert!(matches!(
        keyring.delete(&fingerprint, DeleteScope::PublicOnly),
        Err(Error::SecretMaterialPresent(_))
    ));
    assert!(home.key(&fingerprint).has_secret, "refusal changed nothing");

    keyring
        .delete(&fingerprint, DeleteScope::SecretOnly)
        .unwrap();
    assert!(!home.key(&fingerprint).has_secret);
    assert!(matches!(
        keyring.delete(&fingerprint, DeleteScope::SecretOnly),
        Err(Error::NoSecretMaterial(_))
    ));

    keyring
        .delete(&fingerprint, DeleteScope::PublicOnly)
        .unwrap();
    assert!(keyring.key(&fingerprint).unwrap().is_none());
    assert!(matches!(
        keyring.delete(&fingerprint, DeleteScope::Everything),
        Err(Error::KeyNotFound(_))
    ));
}

/// A local certification stays on this machine; an exportable one travels
/// with the contact's key.
#[test]
fn local_certifications_never_leave_the_keyring() {
    let home = GnupgHome::new();
    let keyring = home.keyring();
    let me = home.create("Ada <ada@example.org>");
    let grace = home.create("Grace <grace@example.org>");
    let linus = home.create("Linus <linus@example.org>");

    assert!(matches!(
        keyring.certify(
            &me,
            &me,
            &[user_id("Ada <ada@example.org>")],
            CertificationKind::Local
        ),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        keyring.certify(
            &grace,
            &me,
            &[user_id("Nobody <no@example.org>")],
            CertificationKind::Local
        ),
        Err(Error::UserIdNotFound(_))
    ));

    let grace_uid = user_id("Grace <grace@example.org>");
    let linus_uid = user_id("Linus <linus@example.org>");
    assert_eq!(
        keyring
            .certify(&grace, &me, &[grace_uid], CertificationKind::Local)
            .unwrap(),
        1
    );
    assert_eq!(
        keyring
            .certify(&linus, &me, &[linus_uid], CertificationKind::Exportable)
            .unwrap(),
        1
    );
    let signed_by_me = |home: &GnupgHome, key| {
        home.key(key).user_ids[0]
            .certifications
            .iter()
            .any(|cert| cert.signer_key_id == me.key_id())
    };
    assert!(signed_by_me(&home, &grace) && signed_by_me(&home, &linus));

    let elsewhere = GnupgHome::new();
    for contact in [&grace, &linus] {
        let armored = keyring.public_key_armored(contact).unwrap();
        elsewhere.keyring().import(armored.as_bytes()).unwrap();
    }
    assert!(
        !signed_by_me(&elsewhere, &grace),
        "local certification leaked"
    );
    assert!(
        signed_by_me(&elsewhere, &linus),
        "exportable certification lost"
    );
}

/// Pressing Cancel in pinentry is its own outcome and leaves no half key.
#[test]
fn cancelled_pinentry_is_reported_and_creates_nothing() {
    let home = GnupgHome::with(Pinentry::Cancels);
    let keyring = home.keyring();

    let result = keyring.create_key(&user_id("Ada <ada@example.org>"), Expiry::Never);
    assert!(matches!(result, Err(Error::Canceled)), "{result:?}");
    assert!(keyring.list_keys().unwrap().is_empty());
}
