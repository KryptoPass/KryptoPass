//! Reading and replacing public recipient fingerprints within store transactions.

use anyhow::{Result, ensure};
use kryptopass_core::{FolderId, KeyFingerprint, RecipientSet, SecretId};
use rusqlite::{Connection, Transaction, params};

pub(super) fn load_folder_recipients(
    connection: &Connection,
    id: FolderId,
) -> Result<Option<RecipientSet>> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM folder_recipients WHERE folder_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored folder recipient metadata is invalid"))?;
    if fingerprints.is_empty() {
        return Ok(None);
    }

    parse_recipient_set(fingerprints).map(Some)
}

pub(super) fn load_secret_recipients(
    connection: &Connection,
    id: SecretId,
) -> Result<RecipientSet> {
    let fingerprints = connection
        .prepare(
            "SELECT fingerprint FROM secret_recipients WHERE secret_id = ?1 ORDER BY fingerprint",
        )
        .and_then(|mut statement| {
            let rows = statement.query_map([id.0.to_string()], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(|_| anyhow::anyhow!("stored secret recipient metadata is invalid"))?;
    ensure!(
        !fingerprints.is_empty(),
        "stored secret recipient metadata is invalid",
    );
    parse_recipient_set(fingerprints)
}

fn parse_recipient_set(values: Vec<String>) -> Result<RecipientSet> {
    let fingerprints = values
        .iter()
        .map(|value| {
            KeyFingerprint::parse(value)
                .map_err(|_| anyhow::anyhow!("stored recipient fingerprint is invalid"))
        })
        .collect::<Result<Vec<_>>>()?;
    RecipientSet::new(fingerprints)
        .map_err(|_| anyhow::anyhow!("stored recipient metadata is invalid"))
}

pub(super) fn replace_folder_recipients(
    transaction: &Transaction<'_>,
    id: FolderId,
    recipients: Option<&RecipientSet>,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM folder_recipients WHERE folder_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
    if let Some(recipients) = recipients {
        for fingerprint in recipients.as_slice() {
            transaction
                .execute(
                    "INSERT INTO folder_recipients(folder_id, fingerprint) VALUES (?1, ?2)",
                    params![id.0.to_string(), fingerprint.as_str()],
                )
                .map_err(|_| anyhow::anyhow!("could not update folder recipients"))?;
        }
    }
    Ok(())
}

pub(super) fn replace_secret_recipients(
    transaction: &Transaction<'_>,
    id: SecretId,
    recipients: &RecipientSet,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM secret_recipients WHERE secret_id = ?1",
            [id.0.to_string()],
        )
        .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    for fingerprint in recipients.as_slice() {
        transaction
            .execute(
                "INSERT INTO secret_recipients(secret_id, fingerprint) VALUES (?1, ?2)",
                params![id.0.to_string(), fingerprint.as_str()],
            )
            .map_err(|_| anyhow::anyhow!("could not update secret recipients"))?;
    }
    Ok(())
}
