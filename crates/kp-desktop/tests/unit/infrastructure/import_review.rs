use anyhow::Result;
use kryptopass_core::SecretContent;
use kryptopass_import::{EntryStatistics, EntryType, ImportError, ImportLimits};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};
use uuid::Uuid;

use super::review_export;

const PROTON_IMPORTER: &str = "org.kryptopass.importer.proton";
const SYNTHETIC_EXPORT: &[u8] = br#"{
    "version": "1",
    "vaults": {
        "synthetic-vault": {
            "name": "Synthetic vault",
            "items": [{
                "itemId": "synthetic-login",
                "state": 1,
                "data": {
                    "metadata": { "name": "Synthetic login" },
                    "type": "login",
                    "content": {
                        "username": "synthetic-user",
                        "password": "synthetic-password"
                    }
                }
            }]
        }
    }
}"#;

struct TemporaryExport {
    directory: PathBuf,
    path: PathBuf,
}

impl TemporaryExport {
    fn new(filename: &str, content: &[u8]) -> Result<Self> {
        let directory = std::env::temp_dir().join(Uuid::new_v4().to_string());
        fs::create_dir(&directory)?;
        let path = directory.join(filename);
        let mut file = File::create_new(&path).inspect_err(|_| {
            let _ = fs::remove_dir(&directory);
        })?;
        let export = Self { directory, path };
        file.write_all(content)?;
        Ok(export)
    }
}

impl Drop for TemporaryExport {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.directory);
    }
}

#[test]
fn proton_content_is_accepted_at_unrelated_filenames_and_locations() -> Result<()> {
    for filename in ["unrelated.random", "Proton Pass.json", "data.pgp"] {
        let export = TemporaryExport::new(filename, SYNTHETIC_EXPORT)?;

        let prepared = review_export(&export.path, "org.kryptopass.importer.proton")?;

        assert_eq!(
            prepared.source,
            EntryStatistics {
                collections: 1,
                folders: 1,
                source_entries: 1,
                imported_entries: 1,
                entries_by_type: [(EntryType::Login, 1)].into(),
                ..EntryStatistics::default()
            }
        );
        assert_eq!(prepared.batch.folders.len(), 1);
        assert_eq!(prepared.batch.items.len(), 1);
        assert_eq!(prepared.batch.folders[0].name, "Synthetic vault");
        assert_eq!(prepared.batch.items[0].title, "Synthetic login");
        let SecretContent::Login(login) = &prepared.batch.items[0].content else {
            panic!("the reviewed entry must be converted to a login");
        };
        assert_eq!(login.username.as_str(), "synthetic-user");
        assert_eq!(login.password.as_str(), "synthetic-password");
        assert!(prepared.warnings.is_empty());
    }
    Ok(())
}

#[test]
fn proton_content_does_not_override_the_selected_importer_id() -> Result<()> {
    let export = TemporaryExport::new("unrelated.random", SYNTHETIC_EXPORT)?;

    let result = review_export(&export.path, "org.kryptopass.importer.unregistered");

    let Err(error) = result else {
        panic!("valid Proton content must not override an unregistered importer ID");
    };
    assert_eq!(
        error.downcast_ref::<ImportError>(),
        Some(&ImportError::Unsupported)
    );
    Ok(())
}

#[test]
fn proton_filename_and_importer_id_do_not_make_malformed_content_acceptable() -> Result<()> {
    for content in [
        &b"not an export"[..],
        &br#"{"version":"1","vaults":{}} trailing"#[..],
        &br#"{"version":"1","vaults":{"synthetic-vault":{"name":"Missing items"}}}"#[..],
    ] {
        let export = TemporaryExport::new("Proton Pass.json", content)?;

        assert!(review_export(&export.path, PROTON_IMPORTER).is_err());
    }
    Ok(())
}

#[test]
fn oversized_input_is_rejected_by_the_file_io_limit() -> Result<()> {
    let export = TemporaryExport::new("unrelated.random", SYNTHETIC_EXPORT)?;
    // Extending the file sparsely exercises the limit without allocating a large fixture.
    OpenOptions::new()
        .write(true)
        .open(&export.path)?
        .set_len(ImportLimits::default().input_bytes + 1)?;

    let result = review_export(&export.path, PROTON_IMPORTER);

    let Err(error) = result else {
        panic!("an export larger than the input limit must be rejected");
    };
    assert_eq!(
        error.downcast_ref::<ImportError>(),
        Some(&ImportError::Limit)
    );
    Ok(())
}
