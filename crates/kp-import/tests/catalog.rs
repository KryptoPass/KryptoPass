use kryptopass_import::{
    DetectedImporter, Detection, EncryptionSupport, EntryType, FeatureSupport, ImportError,
    ImportFormat, ImportLimits, ImportedEntries, Importer, ImporterFeatures, ImporterInfo,
    ImporterRegistry, RegisteredImporter, RegistryError, available_importers,
    importers::proton::{OpenPgpDecryptor, PROTON_PASS_INFO, ProtonPassImporter},
};
use std::{collections::BTreeSet, io::Write};

struct NeverDecrypt;

impl OpenPgpDecryptor for NeverDecrypt {
    fn decrypt(&self, _ciphertext: &[u8], _plaintext: &mut dyn Write) -> Result<(), ImportError> {
        panic!("catalog enumeration must not decrypt");
    }
}

const FAKE_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.test.z-match",
    name: "Test match importer",
    formats: &[ImportFormat {
        extension: "test",
        media_type: "application/x-kryptopass-test",
    }],
    features: ImporterFeatures {
        entry_types: &[EntryType::Note],
        folders: false,
        folder_descriptions: false,
        favorites: false,
        trash: false,
        timestamps: false,
        additional_fields: false,
        passkeys: FeatureSupport::Unsupported,
        autofill_rules: FeatureSupport::Unsupported,
        attachments: FeatureSupport::Unsupported,
        encrypted_exports: EncryptionSupport::RequiresDecryptor,
    },
};

static MATCH_INFO: ImporterInfo = FAKE_INFO;
static DUPLICATE_INFO: ImporterInfo = ImporterInfo {
    name: "Test duplicate importer",
    ..FAKE_INFO
};
static POSSIBLE_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.test.a-possible",
    name: "Test possible importer",
    ..FAKE_INFO
};
static UNSUPPORTED_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.test.b-unsupported",
    name: "Test unsupported importer",
    ..FAKE_INFO
};

struct StaticImporter {
    info: &'static ImporterInfo,
    detection: Detection,
    decryption_available: bool,
}

impl Importer for StaticImporter {
    fn info(&self) -> &'static ImporterInfo {
        self.info
    }

    fn decryption_available(&self) -> bool {
        self.decryption_available
    }

    fn detect(&self, _input: &[u8], _limits: &ImportLimits) -> Detection {
        self.detection
    }

    fn import(
        &self,
        _input: &[u8],
        _limits: &ImportLimits,
    ) -> Result<ImportedEntries, ImportError> {
        panic!("catalog queries must not import");
    }
}

#[test]
fn available_importers_have_nonempty_unique_identifiers() {
    let importers: &'static [&'static ImporterInfo] = available_importers();
    assert!(!importers.is_empty());

    let mut identifiers = BTreeSet::new();
    for info in importers {
        assert!(
            !info.id.is_empty(),
            "catalog importer has an empty identifier"
        );
        assert!(
            identifiers.insert(info.id),
            "duplicate importer: {}",
            info.id
        );
    }
}

#[test]
fn available_importers_include_proton_formats_and_capabilities() {
    let Some(info) = available_importers()
        .iter()
        .copied()
        .find(|info| info.id == "org.kryptopass.importer.proton")
    else {
        panic!("Proton Pass must be available in the library catalog");
    };

    assert_eq!(info, &PROTON_PASS_INFO);
    assert_eq!(info, ProtonPassImporter::new().info());
    assert_eq!(info.name, "Proton Pass");
    assert_eq!(
        info.formats,
        &[
            ImportFormat {
                extension: "json",
                media_type: "application/json",
            },
            ImportFormat {
                extension: "zip",
                media_type: "application/zip",
            },
            ImportFormat {
                extension: "pgp",
                media_type: "application/pgp-encrypted",
            },
        ]
    );
    assert_eq!(
        info.features,
        ImporterFeatures {
            entry_types: &[
                EntryType::Login,
                EntryType::Note,
                EntryType::Alias,
                EntryType::Card,
                EntryType::Identity,
                EntryType::Custom,
                EntryType::Wifi,
                EntryType::SshKey,
            ],
            folders: true,
            folder_descriptions: true,
            favorites: true,
            trash: true,
            timestamps: true,
            additional_fields: true,
            passkeys: FeatureSupport::ReferenceOnly,
            autofill_rules: FeatureSupport::ReferenceOnly,
            attachments: FeatureSupport::Unsupported,
            encrypted_exports: EncryptionSupport::RequiresDecryptor,
        }
    );
}

#[test]
fn registered_proton_descriptor_reports_instance_decryption_without_decrypting() {
    for (importer, decryption_available) in [
        (ProtonPassImporter::new(), false),
        (ProtonPassImporter::with_decryptor(NeverDecrypt), true),
    ] {
        let mut registry = ImporterRegistry::default();
        assert_eq!(registry.register(Box::new(importer)), Ok(()));

        assert_eq!(
            registry.importers().collect::<Vec<_>>(),
            [RegisteredImporter {
                info: &PROTON_PASS_INFO,
                decryption_available,
            }]
        );
    }
}

#[test]
fn duplicate_registration_preserves_the_original_instance() {
    let mut registry = ImporterRegistry::default();
    assert_eq!(
        registry.register(Box::new(StaticImporter {
            info: &MATCH_INFO,
            detection: Detection::Match,
            decryption_available: true,
        })),
        Ok(())
    );

    assert_eq!(
        registry.register(Box::new(StaticImporter {
            info: &DUPLICATE_INFO,
            detection: Detection::Possible,
            decryption_available: false,
        })),
        Err(RegistryError::DuplicateIdentifier)
    );

    assert_eq!(
        registry
            .get(MATCH_INFO.id)
            .map(|importer| (importer.info(), importer.decryption_available())),
        Some((&MATCH_INFO, true))
    );
    assert_eq!(
        registry.importers().collect::<Vec<_>>(),
        [RegisteredImporter {
            info: &MATCH_INFO,
            decryption_available: true,
        }]
    );
    assert_eq!(
        registry.detect(b"synthetic catalog probe", &ImportLimits::default()),
        [DetectedImporter {
            info: &MATCH_INFO,
            detection: Detection::Match,
        }]
    );
}

#[test]
fn custom_descriptors_and_detection_preserve_metadata_and_confidence() {
    let fixtures = [
        (&POSSIBLE_INFO, Detection::Possible, false),
        (&UNSUPPORTED_INFO, Detection::Unsupported, false),
        (&MATCH_INFO, Detection::Match, true),
    ];
    let mut registry = ImporterRegistry::default();
    for (info, detection, decryption_available) in fixtures {
        assert_eq!(
            registry.register(Box::new(StaticImporter {
                info,
                detection,
                decryption_available,
            })),
            Ok(())
        );
    }

    let registered = registry.importers().collect::<Vec<_>>();
    assert_eq!(registered.len(), fixtures.len());
    for (info, _, decryption_available) in fixtures {
        assert!(registered.contains(&RegisteredImporter {
            info,
            decryption_available,
        }));
    }
    assert_eq!(
        registry.detect(b"synthetic catalog probe", &ImportLimits::default()),
        [
            DetectedImporter {
                info: &MATCH_INFO,
                detection: Detection::Match,
            },
            DetectedImporter {
                info: &POSSIBLE_INFO,
                detection: Detection::Possible,
            },
        ]
    );
}
