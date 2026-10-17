#![allow(clippy::unwrap_used)]

use kryptopass_core::{CustomContent, SecretContent};
use kryptopass_import::{
    CustomEntry, Entry, EntryContent, EntryField, EntryFolder, EntryLocation, EntryStatistics,
    EntryType, EntryWarning, EntryWarningKind, FolderId, ImportError, ImportLimits,
    ImportedEntries, LoginEntry,
};
use std::collections::HashSet;
use uuid::Uuid;

fn folders() -> Vec<EntryFolder> {
    vec![
        EntryFolder {
            id: FolderId(7),
            parent: None,
            name: "Personal".into(),
            description: "".into(),
        },
        EntryFolder {
            id: FolderId(42),
            parent: Some(FolderId(7)),
            name: "Accounts".into(),
            description: "".into(),
        },
    ]
}

fn assert_records(custom: &CustomContent, expected: &[(&str, &str, bool, bool)]) {
    let actual: Vec<_> = custom
        .records
        .iter()
        .map(|record| {
            (
                record.key.as_str(),
                record.value.as_str(),
                record.concealed,
                record.multiline,
            )
        })
        .collect();
    assert_eq!(actual.as_slice(), expected);

    let mut ids = HashSet::new();
    for record in &custom.records {
        let id = Uuid::parse_str(record.id.as_str()).unwrap();
        assert!(!id.is_nil());
        assert_eq!(record.id.as_str(), id.hyphenated().to_string());
        assert!(ids.insert(id), "custom records must have distinct UUIDs");
    }
}

#[test]
fn login_preserves_concealed_fields_and_timestamps_in_a_companion_without_exposing_notes() {
    let hidden_value = "\n  synthetic-recovery-α  \nsecond code\n";
    let opaque_value = "synthetic-opaque-secret";
    let warning = EntryWarning {
        location: EntryLocation {
            collection: 0,
            entry: Some(0),
        },
        kind: EntryWarningKind::SourceFieldsPreserved,
    };
    let source = ImportedEntries::new(
        folders(),
        vec![Entry {
            title: "Synthetic account".into(),
            folder: Some(FolderId(42)),
            content: EntryContent::Login(LoginEntry {
                username: "synthetic-user".into(),
                email: "synthetic@example.invalid".into(),
                password: "synthetic-login-password".into(),
                urls: vec![
                    "https://example.invalid/login".into(),
                    "https://example.invalid/other".into(),
                ],
                notes: "Original login notes".into(),
                ..LoginEntry::default()
            }),
            favorite: true,
            trashed: true,
            created_at: Some(1_700_000_000),
            modified_at: Some(1_700_000_123),
            additional_fields: vec![
                EntryField {
                    label: "Recovery code".into(),
                    value: hidden_value.into(),
                    concealed: true,
                    multiline: true,
                },
                EntryField {
                    label: "Recovery code".into(),
                    value: opaque_value.into(),
                    concealed: true,
                    multiline: false,
                },
            ],
        }],
        vec![warning],
        1,
        1,
        &ImportLimits::default(),
    )
    .unwrap();

    let prepared = super::prepare(source).unwrap();

    assert_eq!(prepared.batch.items.len(), 2);
    let primary = prepared
        .batch
        .items
        .iter()
        .find(|item| matches!(&item.content, SecretContent::Login(_)))
        .unwrap();
    let companion = prepared
        .batch
        .items
        .iter()
        .find(|item| matches!(&item.content, SecretContent::Custom(_)))
        .unwrap();
    assert_eq!(primary.title, "Synthetic account");
    assert_eq!(primary.folder, 1);
    assert!(primary.favorite);
    assert!(primary.trashed);
    assert_eq!(companion.title, primary.title);
    assert_eq!(companion.folder, primary.folder);
    assert_eq!(companion.favorite, primary.favorite);
    assert_eq!(companion.trashed, primary.trashed);
    assert_eq!(prepared.batch.folders.len(), 2);
    assert_eq!(prepared.batch.folders[1].name, "Accounts");
    assert_eq!(prepared.batch.folders[1].parent, Some(0));

    let SecretContent::Login(login) = &primary.content else {
        panic!("the primary entry must remain a login");
    };
    assert_eq!(login.username.as_str(), "synthetic-user");
    assert_eq!(login.password.as_str(), "synthetic-login-password");
    assert_eq!(login.url.as_str(), "https://example.invalid/login");
    assert_eq!(
        login
            .urls
            .iter()
            .map(|url| url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://example.invalid/login",
            "https://example.invalid/other"
        ]
    );
    assert_eq!(login.notes.as_str(), "Original login notes");
    assert!(!login.notes.contains(hidden_value));
    assert!(!login.notes.contains(opaque_value));

    let SecretContent::Custom(custom) = &companion.content else {
        panic!("additional fields must be preserved as custom content");
    };
    assert!(custom.notes.is_empty());
    assert_records(
        custom,
        &[
            ("Recovery code", hidden_value, true, true),
            ("Recovery code", opaque_value, true, false),
            ("Fecha de creación (Unix)", "1700000000", false, false),
            ("Fecha de modificación (Unix)", "1700000123", false, false),
            (
                "Correo electrónico",
                "synthetic@example.invalid",
                false,
                false,
            ),
        ],
    );
    assert_eq!(
        prepared.source,
        EntryStatistics {
            collections: 1,
            folders: 2,
            source_entries: 1,
            imported_entries: 1,
            skipped_entries: 0,
            trashed_entries: 1,
            warnings: 1,
            entries_by_type: [(EntryType::Login, 1)].into(),
        }
    );
    assert_eq!(prepared.warnings, vec![warning]);
    assert_eq!(prepared.adaptations.additional_entries, 1);
    assert_eq!(prepared.adaptations.timestamps_as_fields, 1);
    assert_eq!(prepared.adaptations.aliases_as_notes, 0);
    assert_eq!(prepared.adaptations.folder_descriptions, 0);
    assert!(prepared.batch.validate().is_ok());
}

#[test]
fn custom_merges_concealed_additional_fields_and_dates_without_creating_a_companion() {
    let hidden_value = "synthetic-custom-secret\nsecond line";
    let source = ImportedEntries::new(
        folders(),
        vec![Entry {
            title: "Synthetic custom entry".into(),
            folder: Some(FolderId(42)),
            content: EntryContent::Custom(CustomEntry {
                fields: vec![EntryField {
                    label: "Existing field".into(),
                    value: "original value".into(),
                    concealed: false,
                    multiline: false,
                }],
                notes: "Original custom notes".into(),
            }),
            favorite: false,
            trashed: false,
            created_at: Some(0),
            modified_at: None,
            additional_fields: vec![EntryField {
                label: "Hidden extra".into(),
                value: hidden_value.into(),
                concealed: true,
                multiline: true,
            }],
        }],
        vec![],
        1,
        1,
        &ImportLimits::default(),
    )
    .unwrap();

    let prepared = super::prepare(source).unwrap();

    assert_eq!(prepared.batch.items.len(), 1);
    let item = &prepared.batch.items[0];
    assert_eq!(item.title, "Synthetic custom entry");
    assert_eq!(item.folder, 1);
    assert!(!item.favorite);
    assert!(!item.trashed);
    let SecretContent::Custom(custom) = &item.content else {
        panic!("a custom entry must retain its content type");
    };
    assert_eq!(custom.notes.as_str(), "Original custom notes");
    assert!(!custom.notes.contains(hidden_value));
    assert_records(
        custom,
        &[
            ("Existing field", "original value", false, false),
            ("Hidden extra", hidden_value, true, true),
            ("Fecha de creación (Unix)", "0", false, false),
        ],
    );
    assert_eq!(
        prepared.source,
        EntryStatistics {
            collections: 1,
            folders: 2,
            source_entries: 1,
            imported_entries: 1,
            entries_by_type: [(EntryType::Custom, 1)].into(),
            ..EntryStatistics::default()
        }
    );
    assert!(prepared.warnings.is_empty());
    assert_eq!(prepared.adaptations.additional_entries, 0);
    assert_eq!(prepared.adaptations.timestamps_as_fields, 1);
    assert_eq!(prepared.adaptations.aliases_as_notes, 0);
    assert_eq!(prepared.adaptations.folder_descriptions, 0);
    assert!(prepared.batch.validate().is_ok());
}

#[test]
fn prepare_rejects_extra_fields_that_exceed_core_custom_record_capacity() {
    let source = ImportedEntries::new(
        folders(),
        vec![Entry {
            title: "Synthetic oversized companion".into(),
            folder: Some(FolderId(42)),
            content: EntryContent::Login(LoginEntry::default()),
            favorite: false,
            trashed: false,
            created_at: None,
            modified_at: None,
            additional_fields: (0..101)
                .map(|index| EntryField {
                    label: format!("Hidden field {index}").into(),
                    value: "synthetic-secret".into(),
                    concealed: true,
                    multiline: false,
                })
                .collect(),
        }],
        vec![],
        1,
        1,
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(source.entries()[0].additional_fields.len(), 101);

    assert!(matches!(super::prepare(source), Err(ImportError::Content)));
}
