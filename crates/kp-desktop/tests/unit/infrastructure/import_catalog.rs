use anyhow::{Context, Result};
use kryptopass_import::{
    Detection, EncryptionSupport, EntryType, FeatureSupport, ImportError, ImportFormat,
    ImportLimits, ImportedEntries, Importer, ImporterFeatures, ImporterInfo, ImporterRegistry,
};
use serde_json::{Value, json};

use crate::i18n::text;

const SYNTHETIC_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.synthetic",
    name: "Synthetic Importer",
    formats: &[
        ImportFormat {
            extension: "csv",
            media_type: "text/csv",
        },
        ImportFormat {
            extension: "tsv",
            media_type: "text/tab-separated-values",
        },
    ],
    features: ImporterFeatures {
        entry_types: &[EntryType::Wifi, EntryType::SshKey, EntryType::License],
        folders: false,
        folder_descriptions: true,
        favorites: false,
        trash: true,
        timestamps: false,
        additional_fields: true,
        passkeys: FeatureSupport::Supported,
        autofill_rules: FeatureSupport::ReferenceOnly,
        attachments: FeatureSupport::Unsupported,
        encrypted_exports: EncryptionSupport::RequiresDecryptor,
    },
};

const SECONDARY_INFO: ImporterInfo = ImporterInfo {
    id: "org.kryptopass.importer.synthetic.secondary",
    name: "Secondary Importer",
    formats: &[ImportFormat {
        extension: "ini",
        media_type: "text/plain",
    }],
    features: ImporterFeatures {
        entry_types: &[EntryType::Note, EntryType::Totp],
        folders: true,
        folder_descriptions: false,
        favorites: true,
        trash: false,
        timestamps: true,
        additional_fields: false,
        passkeys: FeatureSupport::Unsupported,
        autofill_rules: FeatureSupport::Supported,
        attachments: FeatureSupport::ReferenceOnly,
        encrypted_exports: EncryptionSupport::BuiltIn,
    },
};

struct SyntheticImporter {
    info: &'static ImporterInfo,
    decryption_available: bool,
}

impl Importer for SyntheticImporter {
    fn info(&self) -> &'static ImporterInfo {
        self.info
    }

    fn decryption_available(&self) -> bool {
        self.decryption_available
    }

    fn detect(&self, _input: &[u8], _limits: &ImportLimits) -> Detection {
        panic!("catalog projection must not detect input")
    }

    fn import(
        &self,
        _input: &[u8],
        _limits: &ImportLimits,
    ) -> std::result::Result<ImportedEntries, ImportError> {
        panic!("catalog projection must not import entries")
    }
}

fn catalog(importers: impl IntoIterator<Item = SyntheticImporter>) -> Result<Vec<Value>> {
    let mut registry = ImporterRegistry::default();
    for importer in importers {
        registry.register(Box::new(importer))?;
    }
    Ok(serde_json::from_str(&super::json_for_registry(&registry)?)?)
}

fn expected_capabilities(statuses: &[(&str, &str)]) -> Value {
    json!(
        statuses
            .iter()
            .map(|(label, support)| json!({ "label": text(label), "support": support }))
            .collect::<Vec<_>>()
    )
}

#[test]
fn catalog_projects_synthetic_importer_metadata() -> Result<()> {
    let descriptions = catalog([SyntheticImporter {
        info: &SYNTHETIC_INFO,
        decryption_available: true,
    }])?;

    assert_eq!(descriptions.len(), 1);
    assert_eq!(
        descriptions[0],
        json!({
            "id": "org.kryptopass.importer.synthetic",
            "name": "Synthetic Importer",
            "formats": ["csv", "tsv"],
            "fileFilters": [
                "Synthetic Importer (*.csv *.tsv)",
                text("Todos los archivos (*)"),
            ],
            "entryTypes": [text("Redes Wi-Fi"), text("Claves SSH"), text("Licencias")],
            "capabilities": expected_capabilities(&[
                ("Carpetas", "unsupported"),
                ("Descripciones de carpetas", "supported"),
                ("Favoritos", "unsupported"),
                ("Papelera", "supported"),
                ("Fechas originales", "unsupported"),
                ("Campos adicionales", "supported"),
                ("Passkeys", "supported"),
                ("Reglas de autocompletado", "reference"),
                ("Adjuntos", "unsupported"),
                ("Archivos cifrados", "supported"),
            ]),
            "decryptionAvailable": true,
            "requiresDecryptor": true,
        })
    );

    let filters = descriptions[0]["fileFilters"].to_string();
    for pattern in ["*.json", "*.zip", "*.pgp"] {
        assert!(
            !filters.contains(pattern),
            "unexpected Proton filter: {pattern}"
        );
    }
    Ok(())
}

#[test]
fn catalog_separates_decryption_availability_from_encryption_requirements() -> Result<()> {
    const BUILT_IN_INFO: ImporterInfo = ImporterInfo {
        features: ImporterFeatures {
            encrypted_exports: EncryptionSupport::BuiltIn,
            ..SYNTHETIC_INFO.features
        },
        ..SYNTHETIC_INFO
    };
    const UNENCRYPTED_INFO: ImporterInfo = ImporterInfo {
        features: ImporterFeatures {
            encrypted_exports: EncryptionSupport::Unsupported,
            ..SYNTHETIC_INFO.features
        },
        ..SYNTHETIC_INFO
    };

    for (info, requires_decryptor) in [
        (&SYNTHETIC_INFO, true),
        (&BUILT_IN_INFO, false),
        (&UNENCRYPTED_INFO, false),
    ] {
        for available in [false, true] {
            let descriptions = catalog([SyntheticImporter {
                info,
                decryption_available: available,
            }])?;
            assert_eq!(descriptions.len(), 1);
            let description = &descriptions[0];
            assert_eq!(description["decryptionAvailable"], json!(available));
            assert_eq!(description["requiresDecryptor"], json!(requires_decryptor));

            let capabilities = description["capabilities"]
                .as_array()
                .context("catalog capabilities must be an array")?;
            let encrypted = capabilities
                .iter()
                .find(|capability| capability["label"] == text("Archivos cifrados"))
                .context("catalog must describe encrypted-file support")?;
            assert_eq!(
                encrypted["support"],
                json!(if available {
                    "supported"
                } else {
                    "unsupported"
                })
            );
        }
    }
    Ok(())
}

#[test]
fn empty_registry_has_an_empty_catalog() -> Result<()> {
    assert!(catalog([])?.is_empty());
    Ok(())
}

#[test]
fn catalog_keeps_each_registered_importers_metadata_separate() -> Result<()> {
    let descriptions = catalog([
        SyntheticImporter {
            info: &SECONDARY_INFO,
            decryption_available: true,
        },
        SyntheticImporter {
            info: &SYNTHETIC_INFO,
            decryption_available: false,
        },
    ])?;

    assert_eq!(descriptions.len(), 2);
    let primary = descriptions
        .iter()
        .find(|description| description["id"] == "org.kryptopass.importer.synthetic")
        .context("synthetic importer is missing from the catalog")?;
    let single = catalog([SyntheticImporter {
        info: &SYNTHETIC_INFO,
        decryption_available: false,
    }])?;
    assert_eq!(single.len(), 1);
    assert_eq!(primary, &single[0]);

    let secondary = descriptions
        .iter()
        .find(|description| description["id"] == "org.kryptopass.importer.synthetic.secondary")
        .context("secondary importer is missing from the catalog")?;
    assert_eq!(
        secondary,
        &json!({
            "id": "org.kryptopass.importer.synthetic.secondary",
            "name": "Secondary Importer",
            "formats": ["ini"],
            "fileFilters": ["Secondary Importer (*.ini)", text("Todos los archivos (*)")],
            "entryTypes": [text("Notas"), text("Autenticadores TOTP")],
            "capabilities": expected_capabilities(&[
                ("Carpetas", "supported"),
                ("Descripciones de carpetas", "unsupported"),
                ("Favoritos", "supported"),
                ("Papelera", "unsupported"),
                ("Fechas originales", "supported"),
                ("Campos adicionales", "unsupported"),
                ("Passkeys", "unsupported"),
                ("Reglas de autocompletado", "supported"),
                ("Adjuntos", "reference"),
                ("Archivos cifrados", "supported"),
            ]),
            "decryptionAvailable": true,
            "requiresDecryptor": false,
        })
    );
    Ok(())
}
