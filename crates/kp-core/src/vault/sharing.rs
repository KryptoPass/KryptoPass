//! Who can decrypt an item.

use super::error::VaultError;
use super::record::{ItemMeta, ItemRecord, VersionId};
use super::store::{Base, Change, LinkChange, Parents, PutItem};
use super::{Cipher, Store, Vault, logged, next_revision};
use crate::domain::{RecipientSet, SecretId};
use crate::util::unix_millis;

impl<S: Store, C: Cipher> Vault<S, C> {
    /// Re-encrypts an item for a new recipient set, only at revision `base`.
    ///
    /// The payload is re-encrypted as opaque bytes, so an item whose template
    /// version is no longer supported can still be shared or rescued.
    pub fn change_recipients(
        &mut self,
        id: SecretId,
        base: u64,
        recipients: RecipientSet,
    ) -> Result<ItemMeta, VaultError> {
        logged(
            "change_recipients",
            self.try_change_recipients(id, base, recipients),
        )
    }

    fn try_change_recipients(
        &mut self,
        id: SecretId,
        base: u64,
        recipients: RecipientSet,
    ) -> Result<ItemMeta, VaultError> {
        let ItemRecord {
            mut meta,
            ciphertext,
        } = self.record(id)?;
        if meta.revision != base {
            return Err(VaultError::StaleRevision);
        }
        if meta.state.recipients == recipients {
            return Ok(meta);
        }
        self.cipher.check_recipients(&recipients)?;
        let plaintext = self.cipher.decrypt(&ciphertext)?;
        let ciphertext = self.cipher.encrypt(plaintext.as_slice(), &recipients)?;
        meta.state.recipients = recipients;
        meta.revision = next_revision(base)?;
        meta.head = VersionId::new();

        self.store.commit(Change::PutItem(Box::new(PutItem {
            record: ItemRecord {
                meta: meta.clone(),
                ciphertext,
            },
            base: Base::Revision(base),
            parents: Parents::Head,
            now_millis: unix_millis(),
            icon_asset: None,
            link: LinkChange::Keep,
        })))?;
        tracing::debug!(
            item = %id.0,
            revision = meta.revision,
            recipients = meta.state.recipients.as_slice().len(),
            "item recipients changed"
        );
        Ok(meta)
    }
}
