use std::path::PathBuf;

use kryptopass_core::{FolderId, SecretsService};
use uuid::Uuid;

use super::{GnuPgOpenPgp, SqliteStorage, step_folder_in_service};

type TestService = SecretsService<SqliteStorage, GnuPgOpenPgp>;

struct PrivateDirectory(PathBuf);

impl Drop for PrivateDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct TestVault {
    service: TestService,
    _directory: PrivateDirectory,
}

fn vault() -> anyhow::Result<TestVault> {
    let directory = PrivateDirectory(
        std::env::temp_dir().join(format!("kryptopass-folder-step-{}", Uuid::new_v4())),
    );
    let storage = SqliteStorage::open(directory.0.join("vault.sqlite3"))?;
    Ok(TestVault {
        service: SecretsService::new(storage, GnuPgOpenPgp),
        _directory: directory,
    })
}

fn folder(
    service: &mut TestService,
    name: &str,
    parent: Option<FolderId>,
) -> anyhow::Result<FolderId> {
    service.create_folder(name.to_owned(), parent, None)
}

fn siblings(service: &TestService, parent: Option<FolderId>) -> anyhow::Result<Vec<FolderId>> {
    Ok(service
        .list_folders()?
        .into_iter()
        .filter(|folder| folder.parent_id == parent)
        .map(|folder| folder.id)
        .collect())
}

#[test]
fn keyboard_step_moves_one_position_in_both_directions() -> anyhow::Result<()> {
    let mut vault = vault()?;
    let service = &mut vault.service;
    let first = folder(service, "Zebra", None)?;
    let second = folder(service, "Alpha", None)?;
    let third = folder(service, "Middle", None)?;
    assert_eq!(siblings(service, None)?, vec![first, second, third]);

    step_folder_in_service(service, third, -1)?;
    assert_eq!(siblings(service, None)?, vec![first, third, second]);
    step_folder_in_service(service, third, -1)?;
    assert_eq!(siblings(service, None)?, vec![third, first, second]);
    step_folder_in_service(service, third, 1)?;
    assert_eq!(siblings(service, None)?, vec![first, third, second]);
    step_folder_in_service(service, third, 1)?;
    assert_eq!(siblings(service, None)?, vec![first, second, third]);
    Ok(())
}

#[test]
fn keyboard_step_at_boundaries_preserves_every_folder() -> anyhow::Result<()> {
    let mut vault = vault()?;
    let service = &mut vault.service;
    let first = folder(service, "First", None)?;
    let last = folder(service, "Last", None)?;
    let only_child = folder(service, "Only child", Some(first))?;
    let before = service.list_folders()?;

    for (id, direction) in [(first, -1), (last, 1), (only_child, -1), (only_child, 1)] {
        step_folder_in_service(service, id, direction)?;
        assert_eq!(service.list_folders()?, before);
    }
    Ok(())
}

#[test]
fn keyboard_step_only_reorders_children_of_the_same_parent() -> anyhow::Result<()> {
    let mut vault = vault()?;
    let service = &mut vault.service;
    let parent = folder(service, "Parent", None)?;
    let other_parent = folder(service, "Other parent", None)?;
    let first = folder(service, "Zebra child", Some(parent))?;
    let other_first = folder(service, "Alpha other child", Some(other_parent))?;
    let second = folder(service, "Middle child", Some(parent))?;
    let other_second = folder(service, "Beta other child", Some(other_parent))?;
    folder(service, "Grandchild", Some(first))?;
    let unrelated_before: Vec<_> = service
        .list_folders()?
        .into_iter()
        .filter(|folder| folder.parent_id != Some(parent))
        .collect();

    step_folder_in_service(service, first, 1)?;
    assert_eq!(siblings(service, Some(parent))?, vec![second, first]);
    assert_eq!(
        siblings(service, Some(other_parent))?,
        vec![other_first, other_second]
    );
    let unrelated_after: Vec<_> = service
        .list_folders()?
        .into_iter()
        .filter(|folder| folder.parent_id != Some(parent))
        .collect();
    assert_eq!(unrelated_after, unrelated_before);

    step_folder_in_service(service, first, -1)?;
    assert_eq!(siblings(service, Some(parent))?, vec![first, second]);
    Ok(())
}

#[test]
fn keyboard_step_rejects_invalid_directions_without_mutating_folders() -> anyhow::Result<()> {
    let mut vault = vault()?;
    let service = &mut vault.service;
    let first = folder(service, "First", None)?;
    folder(service, "Second", None)?;
    let before = service.list_folders()?;

    for direction in [i32::MIN, -2, 0, 2, i32::MAX] {
        assert!(step_folder_in_service(service, first, direction).is_err());
        assert_eq!(service.list_folders()?, before);
    }
    Ok(())
}
