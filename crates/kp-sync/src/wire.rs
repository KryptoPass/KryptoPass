//! Bounded Protocol Buffers encoding for immutable revision batches.

use crate::model::{
    Id, Operation, Revision, SYNC_MAX_BATCH_BYTES, SYNC_MAX_CIPHERTEXT_BYTES, SYNC_MAX_PARENTS,
    SYNC_MAX_REVISIONS_PER_BATCH, SyncError, is_zero_id,
};
use kryptopass_protocol::proto::{SyncRevision as WireRevision, SyncRevisionBatch as WireBatch};
use prost::Message;

/// Encodes a bounded batch of structurally valid revisions for one vault.
pub fn encode_batch(vault_id: Id, revisions: &[Revision]) -> Result<Vec<u8>, SyncError> {
    if is_zero_id(&vault_id) {
        return Err(SyncError::InvalidRevision);
    }
    if revisions.len() > SYNC_MAX_REVISIONS_PER_BATCH {
        return Err(SyncError::LimitExceeded);
    }
    let expected_len = encoded_batch_len(vault_id, revisions)?;

    let mut wire_revisions = Vec::new();
    wire_revisions
        .try_reserve_exact(revisions.len())
        .map_err(|_| SyncError::Allocation)?;
    for revision in revisions {
        if revision.vault_id != vault_id {
            return Err(SyncError::VaultMismatch);
        }
        wire_revisions.push(WireRevision {
            vault_id: revision.vault_id.to_vec(),
            record_id: revision.record_id.to_vec(),
            revision_id: revision.revision_id.to_vec(),
            parent_revision_ids: revision
                .parent_revision_ids
                .iter()
                .map(|parent| parent.to_vec())
                .collect(),
            author_device_id: revision.author_device_id.as_bytes().to_vec(),
            operation: i32::from(revision.operation as u8),
            ciphertext: revision.ciphertext.clone(),
            content_hash: revision.content_hash.to_vec(),
            signature: revision.signature.to_vec(),
        });
    }

    let batch = WireBatch {
        vault_id: vault_id.to_vec(),
        revisions: wire_revisions,
    };
    let encoded_len = batch.encoded_len();
    if encoded_len > SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    if encoded_len != expected_len {
        return Err(SyncError::Encode);
    }
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(encoded_len)
        .map_err(|_| SyncError::Allocation)?;
    batch.encode(&mut encoded).map_err(|_| SyncError::Encode)?;
    if encoded.len() != encoded_len {
        return Err(SyncError::Encode);
    }
    Ok(encoded)
}

fn encoded_batch_len(vault_id: Id, revisions: &[Revision]) -> Result<usize, SyncError> {
    let mut batch_len = bytes_field_len(vault_id.len())?;
    for revision in revisions {
        if revision.vault_id != vault_id {
            return Err(SyncError::VaultMismatch);
        }
        revision.validate()?;
        let mut revision_len = 0;
        revision_len = add(revision_len, bytes_field_len(16)?)?; // vault_id
        revision_len = add(revision_len, bytes_field_len(16)?)?; // record_id
        revision_len = add(revision_len, bytes_field_len(16)?)?; // revision_id
        for _ in &revision.parent_revision_ids {
            revision_len = add(revision_len, bytes_field_len(16)?)?;
        }
        revision_len = add(revision_len, bytes_field_len(32)?)?; // author_device_id
        revision_len = add(revision_len, 2)?; // enum field tag and one-byte value
        if !revision.ciphertext.is_empty() {
            revision_len = add(revision_len, bytes_field_len(revision.ciphertext.len())?)?;
        }
        revision_len = add(revision_len, bytes_field_len(32)?)?; // content_hash
        revision_len = add(revision_len, bytes_field_len(64)?)?; // signature
        let nested_len = add(1 + varint_len(revision_len), revision_len)?;
        batch_len = add(batch_len, nested_len)?;
        if batch_len > SYNC_MAX_BATCH_BYTES {
            return Err(SyncError::LimitExceeded);
        }
    }
    Ok(batch_len)
}

fn bytes_field_len(value_len: usize) -> Result<usize, SyncError> {
    add(1 + varint_len(value_len), value_len)
}

fn varint_len(mut value: usize) -> usize {
    let mut length = 1;
    while value >= 0x80 {
        value >>= 7;
        length += 1;
    }
    length
}

fn add(left: usize, right: usize) -> Result<usize, SyncError> {
    left.checked_add(right).ok_or(SyncError::LimitExceeded)
}

/// Decodes and validates one bounded batch before protobuf allocates its fields.
pub fn decode_batch(bytes: &[u8]) -> Result<(Id, Vec<Revision>), SyncError> {
    preflight_batch(bytes)?;
    let batch = WireBatch::decode(bytes).map_err(|_| SyncError::Decode)?;
    let vault_id = fixed::<16>(&batch.vault_id)?;
    if is_zero_id(&vault_id) {
        return Err(SyncError::Decode);
    }
    if batch.revisions.len() > SYNC_MAX_REVISIONS_PER_BATCH {
        return Err(SyncError::LimitExceeded);
    }

    let mut revisions = Vec::new();
    revisions
        .try_reserve_exact(batch.revisions.len())
        .map_err(|_| SyncError::Allocation)?;
    for wire in batch.revisions {
        let revision_vault_id = fixed::<16>(&wire.vault_id)?;
        if revision_vault_id != vault_id {
            return Err(SyncError::VaultMismatch);
        }
        if wire.parent_revision_ids.len() > SYNC_MAX_PARENTS
            || wire.ciphertext.len() > SYNC_MAX_CIPHERTEXT_BYTES
        {
            return Err(SyncError::LimitExceeded);
        }
        let mut parents = Vec::new();
        parents
            .try_reserve_exact(wire.parent_revision_ids.len())
            .map_err(|_| SyncError::Allocation)?;
        for parent in &wire.parent_revision_ids {
            parents.push(fixed::<16>(parent)?);
        }
        if parents.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(SyncError::Decode);
        }
        let operation = Operation::from_wire(wire.operation).ok_or(SyncError::Decode)?;
        let revision = Revision {
            vault_id,
            record_id: fixed::<16>(&wire.record_id)?,
            revision_id: fixed::<16>(&wire.revision_id)?,
            parent_revision_ids: parents,
            author_device_id: kryptopass_identity::DeviceId::from_bytes(fixed::<32>(
                &wire.author_device_id,
            )?),
            operation,
            ciphertext: wire.ciphertext,
            content_hash: fixed::<32>(&wire.content_hash)?,
            signature: fixed::<64>(&wire.signature)?,
        };
        revision.canonical_bytes().map_err(|error| match error {
            SyncError::LimitExceeded => SyncError::LimitExceeded,
            SyncError::Allocation => SyncError::Allocation,
            _ => SyncError::Decode,
        })?;
        revisions.push(revision);
    }
    Ok((vault_id, revisions))
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], SyncError> {
    bytes.try_into().map_err(|_| SyncError::Decode)
}

fn preflight_batch(bytes: &[u8]) -> Result<(), SyncError> {
    if bytes.len() > SYNC_MAX_BATCH_BYTES {
        return Err(SyncError::LimitExceeded);
    }
    let mut cursor = 0;
    let mut revision_count = 0;
    let mut seen_fields = 0_u16;
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field = key >> 3;
        let wire_type = (key & 0x07) as u8;
        if field == 0 || field > 0x1fff_ffff {
            return Err(SyncError::Decode);
        }
        match field {
            1 => {
                mark_singular(&mut seen_fields, 1)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() != 16 {
                    return Err(SyncError::Decode);
                }
            }
            2 => {
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                revision_count += 1;
                if revision_count > SYNC_MAX_REVISIONS_PER_BATCH {
                    return Err(SyncError::LimitExceeded);
                }
                preflight_revision(value)?;
            }
            _ => skip_field(bytes, &mut cursor, wire_type)?,
        }
    }
    Ok(())
}

fn preflight_revision(bytes: &[u8]) -> Result<(), SyncError> {
    let mut cursor = 0;
    let mut parent_count = 0;
    let mut seen_fields = 0_u16;
    while cursor < bytes.len() {
        let key = read_varint(bytes, &mut cursor)?;
        let field = key >> 3;
        let wire_type = (key & 0x07) as u8;
        if field == 0 || field > 0x1fff_ffff {
            return Err(SyncError::Decode);
        }
        match field {
            1..=3 => {
                mark_singular(&mut seen_fields, field as u8)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() != 16 {
                    return Err(SyncError::Decode);
                }
            }
            4 => {
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                parent_count += 1;
                if parent_count > SYNC_MAX_PARENTS {
                    return Err(SyncError::LimitExceeded);
                }
                if value.len() != 16 {
                    return Err(SyncError::Decode);
                }
            }
            5 => {
                mark_singular(&mut seen_fields, 5)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() != 32 {
                    return Err(SyncError::Decode);
                }
            }
            6 => {
                mark_singular(&mut seen_fields, 6)?;
                if wire_type != 0 {
                    return Err(SyncError::Decode);
                }
                match read_varint(bytes, &mut cursor)? {
                    1 | 2 => {}
                    _ => return Err(SyncError::Decode),
                }
            }
            7 => {
                mark_singular(&mut seen_fields, 7)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() > SYNC_MAX_CIPHERTEXT_BYTES {
                    return Err(SyncError::LimitExceeded);
                }
            }
            8 => {
                mark_singular(&mut seen_fields, 8)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() != 32 {
                    return Err(SyncError::Decode);
                }
            }
            9 => {
                mark_singular(&mut seen_fields, 9)?;
                let value = read_bytes_field(bytes, &mut cursor, wire_type)?;
                if value.len() != 64 {
                    return Err(SyncError::Decode);
                }
            }
            _ => skip_field(bytes, &mut cursor, wire_type)?,
        }
    }
    Ok(())
}

fn mark_singular(seen_fields: &mut u16, field: u8) -> Result<(), SyncError> {
    let mask = 1_u16 << field;
    if *seen_fields & mask != 0 {
        return Err(SyncError::Decode);
    }
    *seen_fields |= mask;
    Ok(())
}

fn read_bytes_field<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    wire_type: u8,
) -> Result<&'a [u8], SyncError> {
    if wire_type != 2 {
        return Err(SyncError::Decode);
    }
    let len = usize::try_from(read_varint(bytes, cursor)?).map_err(|_| SyncError::Decode)?;
    let end = cursor.checked_add(len).ok_or(SyncError::Decode)?;
    let value = bytes.get(*cursor..end).ok_or(SyncError::Decode)?;
    *cursor = end;
    Ok(value)
}

fn skip_field(bytes: &[u8], cursor: &mut usize, wire_type: u8) -> Result<(), SyncError> {
    match wire_type {
        0 => {
            read_varint(bytes, cursor)?;
        }
        1 => advance(bytes, cursor, 8)?,
        2 => {
            let len =
                usize::try_from(read_varint(bytes, cursor)?).map_err(|_| SyncError::Decode)?;
            advance(bytes, cursor, len)?;
        }
        5 => advance(bytes, cursor, 4)?,
        _ => return Err(SyncError::Decode),
    }
    Ok(())
}

fn advance(bytes: &[u8], cursor: &mut usize, count: usize) -> Result<(), SyncError> {
    let end = cursor.checked_add(count).ok_or(SyncError::Decode)?;
    if end > bytes.len() {
        return Err(SyncError::Decode);
    }
    *cursor = end;
    Ok(())
}

fn read_varint(bytes: &[u8], cursor: &mut usize) -> Result<u64, SyncError> {
    let mut value = 0_u64;
    for shift in (0..=63).step_by(7) {
        let byte = *bytes.get(*cursor).ok_or(SyncError::Decode)?;
        *cursor += 1;
        if shift == 63 && byte > 1 {
            return Err(SyncError::Decode);
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(SyncError::Decode)
}
