//! Public secret relationships and atomic revision updates for their endpoints.

use super::records::{parse_bool, parse_secret_id};
use anyhow::{Context, Result, ensure};
use kryptopass_core::{Secret, SecretId, SecretLink, SecretLinkError};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::HashSet;

pub(super) fn read_secret_links(connection: &Connection) -> Result<Vec<SecretLink>> {
    let mut statement = connection
        .prepare(
            "SELECT first_secret_id, second_secret_id FROM secret_links \
             ORDER BY first_secret_id, second_secret_id",
        )
        .context("could not read linked item metadata")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("could not read linked item metadata")?;
    let raw_links = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("could not read linked item metadata")?;
    drop(statement);

    raw_links
        .into_iter()
        .map(|(first, second)| parse_secret_link(&first, &second))
        .collect()
}

fn parse_secret_link(first: &str, second: &str) -> Result<SecretLink> {
    let first = parse_secret_id(first)?;
    let second = parse_secret_id(second)?;
    ensure!(
        first.0 < second.0,
        "stored linked item endpoints are not in canonical order"
    );
    Ok(SecretLink { first, second })
}

fn canonical_secret_link(first: SecretId, second: SecretId) -> SecretLink {
    if first.0 < second.0 {
        SecretLink { first, second }
    } else {
        SecretLink {
            first: second,
            second: first,
        }
    }
}

pub(super) fn stored_link_for_secret(
    connection: &Connection,
    id: SecretId,
) -> Result<Option<SecretLink>> {
    let raw = connection
        .query_row(
            "SELECT first_secret_id, second_secret_id FROM secret_links \
             WHERE first_secret_id = ?1 OR second_secret_id = ?1",
            [id.0.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("could not read linked item metadata")?;
    raw.map(|(first, second)| parse_secret_link(&first, &second))
        .transpose()
}

pub(super) fn other_endpoint(link: SecretLink, endpoint: SecretId) -> SecretId {
    if link.first == endpoint {
        link.second
    } else {
        link.first
    }
}

pub(super) fn increment_secret_revision(
    connection: &Connection,
    id: SecretId,
    message: &str,
) -> Result<()> {
    let changed = connection.execute(
        "UPDATE secrets SET revision = revision + 1 \
         WHERE id = ?1 AND revision < 9223372036854775807",
        [id.0.to_string()],
    )?;
    ensure!(changed == 1, "{message}");
    Ok(())
}

pub(super) fn apply_secret_link_change(
    transaction: &Transaction<'_>,
    source: SecretId,
    related: Option<SecretId>,
) -> Result<()> {
    let source_trashed = transaction
        .query_row(
            "SELECT trashed FROM secrets WHERE id = ?1",
            [source.0.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("could not inspect linked item")?
        .ok_or(SecretLinkError::InvalidTarget)?;
    let current =
        stored_link_for_secret(transaction, source)?.map(|link| other_endpoint(link, source));

    if current == related {
        return Ok(());
    }

    if let Some(target) = related {
        if parse_bool(source_trashed)? {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        if target == source {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        let target_trashed = transaction
            .query_row(
                "SELECT trashed FROM secrets WHERE id = ?1",
                [target.0.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("could not inspect linked item")?
            .ok_or(SecretLinkError::InvalidTarget)?;
        if parse_bool(target_trashed)? {
            return Err(SecretLinkError::InvalidTarget.into());
        }
        if let Some(target_link) = stored_link_for_secret(transaction, target)? {
            if other_endpoint(target_link, target) != source {
                return Err(SecretLinkError::Occupied.into());
            }
        }
    }

    if let Some(previous) = current {
        increment_secret_revision(
            transaction,
            previous,
            "a linked item's revision is exhausted; relationship update was cancelled",
        )?;
        let deleted = transaction.execute(
            "DELETE FROM secret_links WHERE first_secret_id = ?1 OR second_secret_id = ?1",
            [source.0.to_string()],
        )?;
        ensure!(
            deleted == 1,
            "linked item relationship changed unexpectedly"
        );
    }

    if let Some(target) = related {
        increment_secret_revision(
            transaction,
            target,
            "a linked item's revision is exhausted; relationship update was cancelled",
        )?;
        let link = canonical_secret_link(source, target);
        transaction
            .execute(
                "INSERT INTO secret_links(first_secret_id, second_secret_id) VALUES (?1, ?2)",
                params![link.first.0.to_string(), link.second.0.to_string()],
            )
            .context("could not save linked item metadata")?;
    }

    Ok(())
}

pub(super) fn validate_backup_links(links: &[SecretLink], secrets: &[Secret]) -> Result<()> {
    let secret_ids = secrets
        .iter()
        .map(|secret| secret.id)
        .collect::<HashSet<_>>();
    let mut linked_ids = HashSet::with_capacity(links.len().saturating_mul(2));
    for link in links {
        ensure!(
            link.first != link.second && link.first.0 < link.second.0,
            "backup linked item endpoints are invalid"
        );
        ensure!(
            secret_ids.contains(&link.first) && secret_ids.contains(&link.second),
            "backup linked item target is missing"
        );
        ensure!(
            linked_ids.insert(link.first) && linked_ids.insert(link.second),
            "backup item is linked more than once"
        );
    }
    Ok(())
}

pub(super) fn insert_imported_secret_links(
    transaction: &Transaction<'_>,
    links: &[SecretLink],
) -> Result<()> {
    for link in links {
        transaction
            .execute(
                "INSERT INTO secret_links(first_secret_id, second_secret_id) VALUES (?1, ?2)",
                params![link.first.0.to_string(), link.second.0.to_string()],
            )
            .context("could not restore linked item metadata")?;
    }
    Ok(())
}
