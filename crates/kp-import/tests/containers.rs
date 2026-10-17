#![allow(clippy::unwrap_used)]

use kryptopass_import::{
    Detection, EntryContent, ImportError, ImportLimits, ImportedEntries, Importer,
    importers::proton::{OpenPgpDecryptor, ProtonPassImporter},
};
use std::{
    io::{Cursor, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const JSON: &[u8] = br#"
{
    "version": "1",
    "vaults": {
        "synthetic-vault": {
            "name": "Synthetic vault",
            "items": [{
                "itemId": "synthetic-note",
                "state": 1,
                "data": {
                    "metadata": {"name": "Synthetic note", "note": "Synthetic contents"},
                    "type": "note",
                    "content": {}
                }
            }]
        }
    }
}
"#;
// Framing markers exercise routing only; neither fixture is real ciphertext.
const ARMORED_PGP: &[u8] =
    b"-----BEGIN PGP MESSAGE-----\nsynthetic ciphertext\n-----END PGP MESSAGE-----";
const BINARY_PGP: &[u8] = b"\xc3\x00synthetic ciphertext";

const ZIP_PAYLOADS: [(&str, &[u8], Detection); 4] = [
    ("Proton Pass/data.json", JSON, Detection::Match),
    ("data.json", JSON, Detection::Possible),
    ("Proton Pass/data.pgp", ARMORED_PGP, Detection::Match),
    ("data.pgp", ARMORED_PGP, Detection::Possible),
];

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for &(name, contents) in entries {
        if name.ends_with('/') {
            writer.add_directory(name, options).unwrap();
        } else {
            writer.start_file(name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
    }
    writer.finish().unwrap().into_inner()
}

fn encrypted_containers() -> Vec<(&'static str, Vec<u8>, Detection)> {
    let mut inputs = vec![
        ("armored OpenPGP", ARMORED_PGP.to_vec(), Detection::Possible),
        ("binary OpenPGP", BINARY_PGP.to_vec(), Detection::Possible),
    ];
    for (name, payload, detection) in ZIP_PAYLOADS {
        if name.ends_with(".pgp") {
            inputs.push((name, archive(&[(name, payload)]), detection));
        }
    }
    inputs
}

fn all_containers() -> Vec<(&'static str, Vec<u8>, Detection)> {
    let mut inputs = encrypted_containers();
    inputs.push(("direct JSON", JSON.to_vec(), Detection::Match));
    for (name, payload, detection) in ZIP_PAYLOADS {
        if name.ends_with(".json") {
            inputs.push((name, archive(&[(name, payload)]), detection));
        }
    }
    inputs
}

fn assert_synthetic_import(imported: ImportedEntries) {
    assert_eq!(imported.statistics().collections, 1);
    assert_eq!(imported.statistics().source_entries, 1);
    assert_eq!(imported.statistics().imported_entries, 1);
    assert_eq!(imported.folders().len(), 1);
    assert_eq!(imported.folders()[0].name.as_str(), "Synthetic vault");
    assert_eq!(imported.entries().len(), 1);
    let entry = &imported.entries()[0];
    assert_eq!(entry.title.as_str(), "Synthetic note");
    assert_eq!(entry.folder, Some(imported.folders()[0].id));
    let EntryContent::Note(note) = &entry.content else {
        panic!("the synthetic container should import its note");
    };
    assert_eq!(note.value.as_str(), "Synthetic contents");
}

struct FakeDecryptor {
    calls: Arc<AtomicUsize>,
}

impl OpenPgpDecryptor for FakeDecryptor {
    fn decrypt(&self, ciphertext: &[u8], plaintext: &mut dyn Write) -> Result<(), ImportError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(ciphertext == ARMORED_PGP || ciphertext == BINARY_PGP);
        for chunk in JSON.chunks(17) {
            plaintext
                .write_all(chunk)
                .map_err(|_| ImportError::Decryption)?;
        }
        Ok(())
    }
}

fn importer_with_fake_decryptor() -> (ProtonPassImporter, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let importer = ProtonPassImporter::with_decryptor(FakeDecryptor {
        calls: Arc::clone(&calls),
    });
    (importer, calls)
}

struct IgnoringOverrunDecryptor {
    overrun_first: bool,
}

impl OpenPgpDecryptor for IgnoringOverrunDecryptor {
    fn decrypt(&self, _ciphertext: &[u8], plaintext: &mut dyn Write) -> Result<(), ImportError> {
        if self.overrun_first {
            let mut oversized = JSON.to_vec();
            oversized.push(b' ');
            assert!(plaintext.write_all(&oversized).is_err());
        }
        plaintext
            .write_all(JSON)
            .map_err(|_| ImportError::Decryption)?;
        if !self.overrun_first {
            assert!(plaintext.write_all(b" ").is_err());
        }
        // Both paths leave valid JSON in the sink and deliberately suppress an overrun error.
        Ok(())
    }
}

#[test]
fn direct_json_is_detected_and_imported() {
    let importer = ProtonPassImporter::new();
    let limits = ImportLimits::default();

    assert_eq!(importer.detect(JSON, &limits), Detection::Match);
    assert_synthetic_import(importer.import(JSON, &limits).unwrap());
}

#[test]
fn direct_json_detection_requires_a_version_and_vault_object() {
    let inputs: &[&[u8]] = &[
        b"not JSON",
        b"{}",
        br#"{"version":"1"}"#,
        br#"{"vaults":{}}"#,
        br#"{"version":"","vaults":{}}"#,
        br#"{"version":"1","vaults":[]}"#,
    ];
    let importer = ProtonPassImporter::new();
    for input in inputs {
        assert_eq!(
            importer.detect(input, &ImportLimits::default()),
            Detection::Unsupported
        );
    }
}

#[test]
fn exact_zip_payload_names_are_detected_and_imported_among_unrelated_entries() {
    let limits = ImportLimits::default();
    for (name, payload, detection) in ZIP_PAYLOADS {
        let input = archive(&[
            ("Proton Pass/", b""),
            ("README.txt", b"synthetic archive"),
            (name, payload),
            ("unrelated.json", b"not a payload"),
        ]);
        let (importer, calls) = importer_with_fake_decryptor();

        assert_eq!(importer.detect(&input, &limits), detection, "{name}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
        assert_eq!(
            calls.load(Ordering::SeqCst),
            usize::from(name.ends_with(".pgp")),
            "{name}"
        );
    }
}

#[test]
fn every_pair_of_recognized_zip_payloads_is_rejected_in_either_order() {
    let limits = ImportLimits::default();
    let (importer, calls) = importer_with_fake_decryptor();
    for (index, first) in ZIP_PAYLOADS.iter().enumerate() {
        for second in &ZIP_PAYLOADS[index + 1..] {
            for (first, second) in [(first, second), (second, first)] {
                let input = archive(&[(first.0, first.1), (second.0, second.1)]);
                assert_eq!(
                    importer.detect(&input, &limits),
                    Detection::Unsupported,
                    "{} and {}",
                    first.0,
                    second.0
                );
                assert_eq!(
                    importer.import(&input, &limits).err(),
                    Some(ImportError::InvalidExport),
                    "{} and {}",
                    first.0,
                    second.0
                );
            }
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unknown_zip_names_are_neither_detected_nor_imported_even_with_valid_payloads() {
    let names = [
        "randominternalnames",
        "random.json",
        "random.pgp",
        "Proton Pass/random.json",
        "Proton Pass/random.pgp",
        "nested/data.json",
        "nested/data.pgp",
        "./data.json",
        "../data.json",
        "/data.json",
        "Proton Pass/../data.json",
        "Proton Pass//data.json",
        "Proton Pass\\data.json",
        "proton pass/data.json",
        "Proton Pass/Data.json",
        "data.JSON",
        "data.PGP",
        "Proton Pass/data.json.bak",
        "data.pgp.bak",
    ];
    let limits = ImportLimits::default();
    let (importer, calls) = importer_with_fake_decryptor();
    for name in names {
        for payload in [JSON, ARMORED_PGP] {
            let input = archive(&[(name, payload)]);
            assert_eq!(
                importer.detect(&input, &limits),
                Detection::Unsupported,
                "{name}"
            );
            assert_eq!(
                importer.import(&input, &limits).err(),
                Some(ImportError::InvalidExport),
                "{name}"
            );
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn encrypted_containers_require_a_decryptor_and_detection_never_invokes_it() {
    let limits = ImportLimits::default();
    for (name, input, detection) in encrypted_containers() {
        let unavailable = ProtonPassImporter::new();
        assert_eq!(unavailable.detect(&input, &limits), detection, "{name}");
        assert_eq!(
            unavailable.import(&input, &limits).err(),
            Some(ImportError::DecryptionRequired),
            "{name}"
        );

        let (importer, calls) = importer_with_fake_decryptor();
        assert_eq!(importer.detect(&input, &limits), detection, "{name}");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn input_byte_limit_rejects_all_containers_before_decryption_and_accepts_exact_size() {
    for (name, input, detection) in all_containers() {
        let (importer, calls) = importer_with_fake_decryptor();
        let size = u64::try_from(input.len()).unwrap();
        for maximum in [0, size - 1] {
            let limits = ImportLimits {
                input_bytes: maximum,
                ..ImportLimits::default()
            };
            assert_eq!(
                importer.detect(&input, &limits),
                Detection::Unsupported,
                "{name}, maximum {maximum}"
            );
            assert_eq!(
                importer.import(&input, &limits).err(),
                Some(ImportError::Limit),
                "{name}, maximum {maximum}"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let limits = ImportLimits {
            input_bytes: size,
            ..ImportLimits::default()
        };
        assert_eq!(importer.detect(&input, &limits), detection, "{name}");
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
    }
}

#[test]
fn decoded_byte_limit_bounds_direct_json_and_expanded_zip_payloads() {
    let mut expanded = JSON.to_vec();
    expanded.extend_from_slice(&[b' '; 4096]);
    let size = u64::try_from(expanded.len()).unwrap();
    let mut inputs = vec![("direct JSON", expanded.clone())];
    for name in ["Proton Pass/data.json", "data.json"] {
        let input = archive(&[(name, &expanded)]);
        assert!(input.len() < expanded.len());
        inputs.push((name, input));
    }
    let importer = ProtonPassImporter::new();
    for (name, input) in inputs {
        for maximum in [0, size - 1] {
            let limits = ImportLimits {
                input_bytes: u64::try_from(input.len()).unwrap(),
                decoded_bytes: maximum,
                ..ImportLimits::default()
            };
            if name == "direct JSON" {
                assert_eq!(importer.detect(&input, &limits), Detection::Unsupported);
            }
            assert_eq!(
                importer.import(&input, &limits).err(),
                Some(ImportError::Limit),
                "{name}, maximum {maximum}"
            );
        }
        let limits = ImportLimits {
            input_bytes: u64::try_from(input.len()).unwrap(),
            decoded_bytes: size,
            ..ImportLimits::default()
        };
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
    }
}

#[test]
fn oversized_encrypted_zip_payloads_are_rejected_before_decryption() {
    let limits = ImportLimits {
        decoded_bytes: u64::try_from(ARMORED_PGP.len()).unwrap() - 1,
        ..ImportLimits::default()
    };
    let (importer, calls) = importer_with_fake_decryptor();
    for name in ["Proton Pass/data.pgp", "data.pgp"] {
        let input = archive(&[(name, ARMORED_PGP)]);
        assert_eq!(
            importer.import(&input, &limits).err(),
            Some(ImportError::Limit),
            "{name}"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn decoded_byte_limit_bounds_decrypted_output_and_accepts_exact_size() {
    let size = u64::try_from(JSON.len()).unwrap();
    for (name, input, _) in encrypted_containers() {
        let (importer, calls) = importer_with_fake_decryptor();
        let limits = ImportLimits {
            decoded_bytes: size - 1,
            ..ImportLimits::default()
        };
        assert_eq!(
            importer.import(&input, &limits).err(),
            Some(ImportError::Limit),
            "{name}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let limits = ImportLimits {
            decoded_bytes: size,
            ..limits
        };
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn archive_entry_limit_counts_directories_and_unrelated_files_and_accepts_exact_count() {
    for (name, payload, detection) in ZIP_PAYLOADS {
        let input = archive(&[
            ("Proton Pass/", b""),
            ("README.txt", b"synthetic archive"),
            (name, payload),
        ]);
        let (importer, calls) = importer_with_fake_decryptor();
        for maximum in [0, 2] {
            let limits = ImportLimits {
                archive_entries: maximum,
                ..ImportLimits::default()
            };
            assert_eq!(
                importer.detect(&input, &limits),
                Detection::Unsupported,
                "{name}, maximum {maximum}"
            );
            assert_eq!(
                importer.import(&input, &limits).err(),
                Some(ImportError::Limit),
                "{name}, maximum {maximum}"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let limits = ImportLimits {
            archive_entries: 3,
            ..ImportLimits::default()
        };
        assert_eq!(importer.detect(&input, &limits), detection, "{name}");
        assert_synthetic_import(importer.import(&input, &limits).unwrap());
    }
}

#[test]
fn bounded_sink_remembers_overruns_even_when_the_decryptor_ignores_writer_errors() {
    let limits = ImportLimits {
        decoded_bytes: u64::try_from(JSON.len()).unwrap(),
        ..ImportLimits::default()
    };
    for (name, input, _) in encrypted_containers() {
        for overrun_first in [false, true] {
            let importer =
                ProtonPassImporter::with_decryptor(IgnoringOverrunDecryptor { overrun_first });
            assert_eq!(
                importer.import(&input, &limits).err(),
                Some(ImportError::Limit),
                "{name}, overrun_first {overrun_first}"
            );
        }
    }
}
