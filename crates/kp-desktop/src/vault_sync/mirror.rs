//! Two-way mirror between the Desktop vault and the signed ledger shared with
//! paired phones.
//!
//! Only records encrypted exactly to the vault's primary key take part, because
//! they are the ones a phone holding that key can open. Desktop owns the folder
//! tree (phones only see the path) and links stay Desktop-only. Icons travel in
//! the object: a theme by name, an image by hash, with its PNG published once
//! as a separate asset record (UUID version 8) that every pass skips as an
//! entry. Favicons are never sent: each side derives them from the URL. A small state file remembers, per record, the Desktop
//! revision and the ledger head of the last exchange, so each run knows which
//! side changed. When both changed, the Desktop version wins and the phone
//! version is kept as a separate copy, so no edit is lost. A delete from the
//! phone only moves the Desktop record to the trash.

use super::{SyncedVault, VaultSyncError};
use crate::infrastructure::openpgp::NonInteractiveGnuPgOpenPgp;
use crate::storage::{SqliteStorage, paths};
use kryptopass_core::{
    EntryIcon, EntryIconAsset, Folder, FolderId, IconState, KeyFingerprint, OpenPgp, RecipientSet,
    Secret, SecretContent, SecretId, SecretMetadata, Storage, VAULT_ICON_MEDIA_TYPE_PNG, VaultIcon,
    VaultObject, asset_record_id, automatic_entry_icon, decode_content, decode_vault_object,
    encode_content, is_asset_record_id, parse_sha256_hex, png_sha256, sha256_hex, valid_theme_name,
};
use kryptopass_identity::{DeviceId, DeviceIdentity};
use kryptopass_pairing_lab::LabHost;
use kryptopass_sync::{Id, Ledger, Operation, RevisionHead, TrustSource};
use kryptopass_vault::{AssetWrite, read_vault_asset};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Vault ID of the fixed synthetic exercise; that selection may be replaced.
const SYNTHETIC_VAULT_ID: Id = [
    0x5c, 0x0a, 0xa3, 0x60, 0x54, 0xc3, 0x4b, 0x7c, 0xaa, 0x4e, 0xdc, 0xf6, 0x3c, 0xdd, 0x8c, 0x27,
];
const VAULT_ID_DOMAIN: &[u8] = b"org.kryptopass.vault-id.v1\0";
const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
const COPY_SUFFIX: &str = " (copia del teléfono)";
/// Internal switch for publishing theme and image icons. Phones that predate
/// the icon field refuse the whole vault, so it stays on only while every
/// paired phone runs a reader that knows it (true in the lab, where both apps
/// update together).
const PUBLISH_ICONS: bool = true;

/// Payload-free failures, each with a fixed message for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MirrorError {
    NoVault,
    NoVaultKey,
    Profile,
    OtherVaultSelected,
    Locked,
    Storage,
}

impl MirrorError {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::NoVault => "No hay una bóveda abierta en Desktop.",
            Self::NoVaultKey => {
                "La bóveda no tiene clave principal. Elígela en Ajustes antes de sincronizar."
            }
            Self::Profile => "No se pudo abrir el perfil de vinculación de este teléfono.",
            Self::OtherVaultSelected => {
                "Este teléfono ya sincroniza otra bóveda. Desactívala antes de cambiarla."
            }
            Self::Locked => "Desbloquea la bóveda en Desktop para sincronizar los cambios.",
            Self::Storage => "No se pudo leer o guardar la bóveda. Se reintentará.",
        }
    }
}

/// What one run changed; `pulled` tells the UI to reload the vault.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MirrorOutcome {
    pub(crate) pulled: usize,
    pub(crate) pushed: usize,
    pub(crate) skipped: usize,
}

/// The vault ID both apps derive from the key, so no network message selects it.
pub(crate) fn vault_id_for(fingerprint: &[u8]) -> Id {
    let mut digest = Sha256::new();
    digest.update(VAULT_ID_DOMAIN);
    digest.update(fingerprint);
    let digest = digest.finalize();
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest[..16]);
    id
}

/// Selects the vault for `peer`, replacing only the synthetic exercise.
/// The next session with the phone carries Sync.
pub(crate) fn enable(lab_directory: &Path, peer: DeviceId) -> Result<(), MirrorError> {
    let vault = ActiveVault::open()?;
    let host =
        LabHost::open_existing_system_profile(lab_directory).map_err(|_| MirrorError::Profile)?;
    let fingerprint = vault.fingerprint_bytes()?;
    let vault_id = vault_id_for(&fingerprint);
    if let Some(info) = host
        .sync_vault_info(peer)
        .map_err(|_| MirrorError::Profile)?
    {
        if info.vault_id == vault_id && info.fingerprint == fingerprint {
            return Ok(());
        }
        if info.vault_id != SYNTHETIC_VAULT_ID {
            return Err(MirrorError::OtherVaultSelected);
        }
        host.remove_sync_vault(peer)
            .map_err(|_| MirrorError::Profile)?;
    }
    host.configure_sync_vault(peer, vault_id, &fingerprint)
        .map_err(|_| MirrorError::Profile)
}

/// Stops syncing with `peer`. The ledger stays, as the history of the vault.
pub(crate) fn disable(lab_directory: &Path, peer: DeviceId) -> Result<(), MirrorError> {
    let host =
        LabHost::open_existing_system_profile(lab_directory).map_err(|_| MirrorError::Profile)?;
    host.remove_sync_vault(peer)
        .map_err(|_| MirrorError::Profile)
}

/// Whether `peer` syncs the active vault.
pub(crate) fn is_enabled(lab_directory: &Path, peer: DeviceId) -> bool {
    let Ok(vault) = ActiveVault::open() else {
        return false;
    };
    let Ok(fingerprint) = vault.fingerprint_bytes() else {
        return false;
    };
    LabHost::open_existing_system_profile(lab_directory)
        .ok()
        .and_then(|host| host.sync_vault_info(peer).ok().flatten())
        .is_some_and(|info| {
            info.vault_id == vault_id_for(&fingerprint) && info.fingerprint == fingerprint
        })
}

/// Exchanges every change since the last run between the vault and the ledger.
/// Does nothing while no trusted phone syncs the active vault.
pub(crate) fn run(lab_directory: &Path) -> Result<MirrorOutcome, MirrorError> {
    let mut vault = ActiveVault::open()?;
    let fingerprint_bytes = vault.fingerprint_bytes()?;
    let vault_id = vault_id_for(&fingerprint_bytes);
    let host =
        LabHost::open_existing_system_profile(lab_directory).map_err(|_| MirrorError::Profile)?;
    let peers = host.trusted_peers().map_err(|_| MirrorError::Profile)?;
    let Some((peer, ledger_path)) = peers.iter().find_map(|peer| {
        host.sync_vault_info(peer.device_id)
            .ok()
            .flatten()
            .filter(|info| info.vault_id == vault_id && info.fingerprint == fingerprint_bytes)
            .map(|info| (peer.device_id, info.ledger_path))
    }) else {
        tracing::debug!(
            trusted_peers = peers.len(),
            "Vault mirror idle: no trusted phone syncs the active vault key"
        );
        return Ok(MirrorOutcome::default());
    };
    let state_path = ledger_path.with_extension("mirror.json");
    host.with_sync_vault(peer, |ledger, identity, trust, _| {
        Ok(Mirror {
            ledger,
            identity,
            trust,
            vault: &mut vault,
            crypto: NonInteractiveGnuPgOpenPgp,
            state_path: &state_path,
            folder_paths: HashMap::new(),
            asset_hashes: None,
            pending_icons: HashSet::new(),
        }
        .run())
    })
    .map_err(|error| {
        tracing::warn!(error = ?error, "Vault mirror could not open the selected ledger");
        MirrorError::Storage
    })?
}

/// The active Desktop vault and its primary key.
struct ActiveVault {
    path: PathBuf,
    storage: SqliteStorage,
    primary: KeyFingerprint,
}

impl ActiveVault {
    fn open() -> Result<Self, MirrorError> {
        let app_id = crate::app_info::APP.desktop_file_id;
        let path = match paths::active_vault_path(app_id).map_err(|_| MirrorError::NoVault)? {
            Some(path) => path,
            None => paths::database_path(app_id).map_err(|_| MirrorError::NoVault)?,
        };
        if !path.exists() {
            return Err(MirrorError::NoVault);
        }
        let storage = SqliteStorage::open_existing(&path).map_err(|_| MirrorError::NoVault)?;
        let primary = storage
            .primary_key()
            .map_err(|_| MirrorError::Storage)?
            .ok_or(MirrorError::NoVaultKey)?
            .fingerprint;
        Ok(Self {
            path,
            storage,
            primary,
        })
    }

    fn fingerprint_bytes(&self) -> Result<Vec<u8>, MirrorError> {
        let text = self.primary.as_str();
        if text.len() != 40 {
            return Err(MirrorError::NoVaultKey);
        }
        (0..text.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&text[index..index + 2], 16))
            .collect::<Result<_, _>>()
            .map_err(|_| MirrorError::NoVaultKey)
    }
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
struct RecordState {
    /// Desktop revision after the last exchange; absent once deleted there.
    main: Option<u64>,
    /// Ledger head after the last exchange.
    ledger: Option<[u8; 16]>,
    /// Digest of the Desktop folder path sent with that exchange; 0 at the root.
    #[serde(default)]
    folder: u64,
    /// Digest of the Desktop icon in wire form after that exchange; 0 when automatic.
    #[serde(default)]
    icon: u64,
    /// The ledger's image icon could not be applied yet because its asset was
    /// missing or unreadable; each run retries it until it lands.
    #[serde(default)]
    icon_pending: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MirrorState {
    version: u32,
    vault_path: String,
    records: BTreeMap<uuid::Uuid, RecordState>,
}

impl MirrorState {
    fn load(path: &Path, vault_path: &Path) -> Result<Self, MirrorError> {
        let vault_path = vault_path.to_string_lossy().into_owned();
        let bytes = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_STATE_BYTES => {
                fs::read(path).map_err(|_| MirrorError::Storage)?
            }
            Ok(_) => return Err(MirrorError::Storage),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    version: 1,
                    vault_path,
                    records: BTreeMap::new(),
                });
            }
            Err(_) => return Err(MirrorError::Storage),
        };
        let state: Self = serde_json::from_slice(&bytes).map_err(|_| MirrorError::Storage)?;
        // The state belongs to one vault file; never apply it to another vault.
        if state.version != 1 || state.vault_path != vault_path {
            return Err(MirrorError::OtherVaultSelected);
        }
        Ok(state)
    }

    fn save(&self, path: &Path) -> Result<(), MirrorError> {
        let encoded = serde_json::to_vec(self).map_err(|_| MirrorError::Storage)?;
        let temporary = path.with_extension("mirror.json.new");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| MirrorError::Storage)?;
        file.write_all(&encoded)
            .and_then(|()| file.sync_all())
            .map_err(|_| MirrorError::Storage)?;
        fs::rename(&temporary, path).map_err(|_| MirrorError::Storage)
    }
}

struct Mirror<'a> {
    ledger: &'a mut Ledger,
    identity: &'a DeviceIdentity,
    trust: &'a dyn TrustSource,
    vault: &'a mut ActiveVault,
    crypto: NonInteractiveGnuPgOpenPgp,
    state_path: &'a Path,
    /// Folder names from the root for every Desktop folder, sent with each record.
    folder_paths: HashMap<FolderId, Vec<String>>,
    /// SHA-256 of each Desktop icon image, loaded on first use in a run.
    asset_hashes: Option<HashMap<uuid::Uuid, [u8; 32]>>,
    /// Records whose received image icon is waiting for its asset.
    pending_icons: HashSet<uuid::Uuid>,
}

/// What Desktop sends for one record's icon, read before the ledger is borrowed.
enum OutgoingIcon {
    Automatic,
    Theme(String),
    Image(Vec<u8>),
}

/// One record's Desktop side, decrypted on demand.
struct MainRecord {
    secret: Secret,
    content: SecretContent,
}

impl Mirror<'_> {
    fn run(mut self) -> Result<MirrorOutcome, MirrorError> {
        let mut state = MirrorState::load(self.state_path, &self.vault.path)?;
        self.folder_paths = folder_paths(
            &self
                .vault
                .storage
                .list_folders()
                .map_err(|_| MirrorError::Storage)?,
        );
        let mut heads = BTreeMap::<uuid::Uuid, Vec<RevisionHead>>::new();
        let mut assets = 0_usize;
        for head in self.ledger.manifest().map_err(|_| MirrorError::Storage)? {
            // Icon assets are not entries; they are read by hash when an object needs one.
            if is_asset_record_id(&uuid::Uuid::from_bytes(head.record_id)) {
                assets += 1;
                continue;
            }
            heads
                .entry(uuid::Uuid::from_bytes(head.record_id))
                .or_default()
                .push(head);
        }
        let main: BTreeMap<uuid::Uuid, SecretMetadata> = self
            .vault
            .storage
            .list_secret_metadata()
            .map_err(|_| MirrorError::Storage)?
            .into_iter()
            .map(|meta| (meta.id.0, meta))
            .collect();
        let ids: BTreeSet<uuid::Uuid> = main
            .keys()
            .chain(heads.keys())
            .chain(state.records.keys())
            .copied()
            .collect();

        let heads_total = heads.len();
        let main_total = main.len();
        let mut not_primary_only = 0_usize;
        let mut outcome = MirrorOutcome::default();
        let mut locked = false;
        let mut changed = false;
        for id in ids {
            let meta = main.get(&id);
            // Records shared with other keys stay Desktop-only; a phone could not open them.
            if meta.is_some_and(|meta| {
                meta.recipients.as_slice() != std::slice::from_ref(&self.vault.primary)
            }) {
                not_primary_only += 1;
                continue;
            }
            let record_heads = heads.remove(&id).unwrap_or_default();
            let folder = folder_digest(
                meta.and_then(|meta| meta.folder_id)
                    .and_then(|folder| self.folder_paths.get(&folder))
                    .map_or(&[][..], Vec::as_slice),
            );
            let icon = match meta {
                Some(meta) => icon_digest(&self.wire_icon(meta.icon.as_ref())),
                None => 0,
            };
            match self.reconcile(
                SecretId(id),
                meta,
                &record_heads,
                state.records.get(&id).copied(),
                folder,
                icon,
                &mut outcome,
            ) {
                Ok(Some(mut next)) => {
                    next.folder = if next.main.is_some() { folder } else { 0 };
                    // A pull may have changed the Desktop icon; record the one it has now.
                    next.icon = match next.main {
                        Some(_) => self.current_icon_digest(SecretId(id)),
                        None => 0,
                    };
                    next.icon_pending = self.pending_icons.remove(&id);
                    changed |= state.records.insert(id, next) != Some(next);
                }
                Ok(None) => {}
                Err(Failure::Locked) => {
                    tracing::warn!("Vault mirror stopped: gpg-agent has no cached passphrase");
                    locked = true;
                    break;
                }
                Err(Failure::Record(line)) => {
                    tracing::warn!(line, "Vault mirror skipped a record");
                    outcome.skipped += 1;
                }
            }
        }
        tracing::debug!(
            ledger_records = heads_total,
            ledger_assets = assets,
            desktop_records = main_total,
            pulled = outcome.pulled,
            pushed = outcome.pushed,
            skipped = outcome.skipped,
            not_primary_only,
            locked,
            changed,
            "Vault mirror pass"
        );
        if changed {
            state.save(self.state_path)?;
        }
        if locked && outcome.pulled == 0 && outcome.pushed == 0 {
            return Err(MirrorError::Locked);
        }
        Ok(outcome)
    }

    /// Returns the new state for one record, or None when it needs no change.
    #[allow(clippy::too_many_arguments)]
    fn reconcile(
        &mut self,
        id: SecretId,
        meta: Option<&SecretMetadata>,
        heads: &[RevisionHead],
        prior: Option<RecordState>,
        folder: u64,
        icon: u64,
        outcome: &mut MirrorOutcome,
    ) -> Result<Option<RecordState>, Failure> {
        let main_revision = meta.map(|meta| meta.revision);
        // A move between Desktop folders, or a first publication of an icon, keeps
        // the revision but must still reach the phone.
        let main_moved = main_revision.is_some()
            && prior.is_some_and(|prior| prior.folder != folder || prior.icon != icon);
        let head_ids: Vec<Id> = heads.iter().map(|head| head.revision_id).collect();

        if heads.len() > 1 {
            // Concurrent branches: the first phone branch applies when Desktop did not
            // change, the others survive as copies, then one revision merges them all.
            let ours = prior.and_then(|prior| prior.ledger);
            let desktop_changed =
                main_moved || prior.is_none_or(|prior| prior.main != main_revision);
            let mut kept_main = main_revision.is_some() && desktop_changed;
            let mut expected_main = if desktop_changed {
                None
            } else {
                prior.and_then(|prior| prior.main)
            };
            for head in heads.iter().filter(|head| Some(head.revision_id) != ours) {
                if let Some(object) = self.ledger_object(id, head.revision_id)? {
                    if kept_main {
                        self.insert_copy(id, head.revision_id, object)?;
                    } else {
                        expected_main = Some(self.upsert_main(id, object, expected_main)?);
                        kept_main = true;
                    }
                    outcome.pulled += 1;
                }
            }
            return self.push(id, &head_ids, outcome).map(Some);
        }

        let head = heads.first();
        let Some(prior) = prior else {
            return match (main_revision, head) {
                (Some(_), None) => self.push(id, &[], outcome).map(Some),
                (Some(revision), Some(head)) => {
                    // No common history yet: equal content just records the pair.
                    let object = self.ledger_object(id, head.revision_id)?;
                    let main = self.read_main(id)?;
                    if object.as_ref().is_some_and(|object| same(&main, object)) {
                        // Equal entries that differ only in the icon need no copy.
                        if object
                            .as_ref()
                            .is_some_and(|object| icon_digest(&object.icon) != icon)
                        {
                            return self.push(id, &head_ids, outcome).map(Some);
                        }
                        return Ok(Some(RecordState {
                            main: Some(revision),
                            ledger: Some(head.revision_id),
                            ..RecordState::default()
                        }));
                    }
                    if let Some(object) = object {
                        self.insert_copy(id, head.revision_id, object)?;
                        outcome.pulled += 1;
                    }
                    self.push(id, &head_ids, outcome).map(Some)
                }
                (None, Some(head)) => match self.ledger_object(id, head.revision_id)? {
                    Some(object) => {
                        let revision = self.upsert_main(id, object, None)?;
                        outcome.pulled += 1;
                        Ok(Some(RecordState {
                            main: Some(revision),
                            ledger: Some(head.revision_id),
                            ..RecordState::default()
                        }))
                    }
                    None => Ok(Some(RecordState {
                        main: None,
                        ledger: Some(head.revision_id),
                        ..RecordState::default()
                    })),
                },
                (None, None) => Ok(None),
            };
        };

        let main_changed = main_moved || prior.main != main_revision;
        let mut ledger_changed = prior.ledger != head.map(|head| head.revision_id);
        // A pending image icon reapplies the same ledger head once its asset arrived.
        if prior.icon_pending && !ledger_changed && !main_changed && main_revision.is_some() {
            match head {
                Some(head) if self.pending_asset_ready(id, head.revision_id)? => {
                    tracing::debug!("Vault mirror retries a record whose icon asset arrived");
                    ledger_changed = true;
                }
                _ => return Ok(None),
            }
        }
        match (main_changed, ledger_changed) {
            (false, false) => Ok(None),
            (true, false) => self.push(id, &head_ids, outcome).map(Some),
            (false, true) => self.pull(id, head, prior.main, outcome).map(Some),
            (true, true) => {
                let Some(head) = head else {
                    return self.push(id, &[], outcome).map(Some);
                };
                match (main_revision, self.ledger_object(id, head.revision_id)?) {
                    // Both edited: the phone version survives as a copy.
                    (Some(_), Some(object)) => {
                        self.insert_copy(id, head.revision_id, object)?;
                        outcome.pulled += 1;
                        self.push(id, &head_ids, outcome).map(Some)
                    }
                    // An edit on Desktop wins over a delete on the phone.
                    (Some(_), None) => self.push(id, &head_ids, outcome).map(Some),
                    // An edit on the phone wins over a delete on Desktop.
                    (None, Some(_)) => self.pull(id, Some(head), None, outcome).map(Some),
                    (None, None) => Ok(Some(RecordState {
                        main: None,
                        ledger: Some(head.revision_id),
                        ..RecordState::default()
                    })),
                }
            }
        }
    }

    /// Writes the Desktop side to the ledger: the record, or a tombstone once deleted.
    fn push(
        &mut self,
        id: SecretId,
        expected: &[Id],
        outcome: &mut MirrorOutcome,
    ) -> Result<RecordState, Failure> {
        let fingerprint = self.vault.primary.clone();
        let main = match self
            .vault
            .storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?
        {
            Some(_) => Some(self.read_main(id)?),
            None => None,
        };
        let outgoing = match &main {
            Some(main) if PUBLISH_ICONS => self.outgoing_icon(id, main.secret.icon.as_ref()),
            _ => OutgoingIcon::Automatic,
        };
        let mut synced = SyncedVault::new(
            self.ledger,
            fingerprint,
            self.identity,
            self.trust,
            &self.crypto,
        )
        .map_err(Failure::from)?;
        // The asset goes first, so a phone never sees an object whose image is missing.
        let icon = match outgoing {
            OutgoingIcon::Automatic => IconState::Automatic,
            OutgoingIcon::Theme(name) => IconState::Known(VaultIcon::Theme { name }),
            OutgoingIcon::Image(png) => match synced.write_asset(&png) {
                Ok(write @ (AssetWrite::Written(_) | AssetWrite::Present)) => {
                    tracing::debug!(
                        written = matches!(write, AssetWrite::Written(_)),
                        bytes = png.len(),
                        "Vault mirror published an icon asset"
                    );
                    IconState::Known(VaultIcon::Image {
                        sha256: sha256_hex(&png_sha256(&png)),
                        media_type: VAULT_ICON_MEDIA_TYPE_PNG.to_owned(),
                    })
                }
                Ok(AssetWrite::OverBudget) => {
                    tracing::warn!(
                        reason = "icon_budget",
                        bytes = png.len(),
                        "Vault mirror sends the record without its image icon"
                    );
                    IconState::Automatic
                }
                Err(error) => {
                    tracing::warn!(
                        reason = "asset_write",
                        error = ?error,
                        "Vault mirror sends the record without its image icon"
                    );
                    IconState::Automatic
                }
            },
        };
        let receipt = match &main {
            Some(main) => synced
                .save(
                    id,
                    expected,
                    main.secret.title.clone(),
                    main.secret.favorite,
                    main.secret.trashed,
                    main.secret
                        .folder_id
                        .and_then(|folder| self.folder_paths.get(&folder))
                        .cloned()
                        .unwrap_or_default(),
                    icon,
                    main.content.clone(),
                )
                .map_err(Failure::from)?,
            None if expected.is_empty() => {
                return Ok(RecordState::default());
            }
            None => synced.delete(id, expected).map_err(Failure::from)?,
        };
        outcome.pushed += 1;
        Ok(RecordState {
            main: main.map(|main| main.secret.revision),
            ledger: Some(receipt.revision_id),
            ..RecordState::default()
        })
    }

    /// Applies the ledger head to the Desktop side, only if the Desktop record is
    /// still at `expected`, the revision the decision was based on. A phone delete
    /// moves the record to the trash; only Desktop deletes permanently.
    fn pull(
        &mut self,
        id: SecretId,
        head: Option<&RevisionHead>,
        expected: Option<u64>,
        outcome: &mut MirrorOutcome,
    ) -> Result<RecordState, Failure> {
        let Some(head) = head else {
            return Ok(RecordState::default());
        };
        let main = match self.ledger_object(id, head.revision_id)? {
            Some(object) => Some(self.upsert_main(id, object, expected)?),
            None => self.trash_main(id, expected)?,
        };
        outcome.pulled += 1;
        Ok(RecordState {
            main,
            ledger: Some(head.revision_id),
            ..RecordState::default()
        })
    }

    fn read_main(&self, id: SecretId) -> Result<MainRecord, Failure> {
        let secret = self
            .vault
            .storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?
            .ok_or(Failure::Record(line!()))?;
        let plaintext = self
            .crypto
            .decrypt(&secret.ciphertext)
            .map_err(|_| Failure::Locked)?;
        let content = decode_content(plaintext.as_slice()).map_err(|_| Failure::Record(line!()))?;
        if content.kind() != secret.kind {
            return Err(Failure::Record(line!()));
        }
        Ok(MainRecord { secret, content })
    }

    fn trash_main(&mut self, id: SecretId, expected: Option<u64>) -> Result<Option<u64>, Failure> {
        let storage = &mut self.vault.storage;
        let Some(existing) = storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?
        else {
            return Ok(None);
        };
        if Some(existing.revision) != expected {
            return Err(Failure::Record(line!()));
        }
        if existing.trashed {
            return Ok(Some(existing.revision));
        }
        let revision = existing.revision;
        storage
            .replace_secret(
                Secret {
                    trashed: true,
                    ..existing
                },
                revision,
            )
            .map_err(|_| Failure::Record(line!()))?;
        storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?
            .map(|secret| Some(secret.revision))
            .ok_or(Failure::Record(line!()))
    }

    /// Decrypts one ledger revision; None for a tombstone.
    fn ledger_object(&self, id: SecretId, revision_id: Id) -> Result<Option<VaultObject>, Failure> {
        let revision = self
            .ledger
            .get(revision_id)
            .map_err(|_| Failure::Record(line!()))?
            .ok_or(Failure::Record(line!()))?;
        if revision.operation == Operation::Delete {
            return Ok(None);
        }
        // Desktop records detect a locked vault; an unreadable ledger revision
        // only skips its own record so it cannot block the others.
        let plaintext = self
            .crypto
            .decrypt(&revision.ciphertext)
            .map_err(|_| Failure::Record(line!()))?;
        decode_vault_object(plaintext.as_slice(), id, &self.vault.primary)
            .map(Some)
            .map_err(|_| Failure::Record(line!()))
    }

    /// Creates or replaces the Desktop record, keeping its folder. The icon follows
    /// the object (see `incoming_icon`). The write happens only if the record is
    /// still at `expected` (None: absent), so an edit made during the run is never
    /// overwritten; the next run sees it.
    fn upsert_main(
        &mut self,
        id: SecretId,
        object: VaultObject,
        expected: Option<u64>,
    ) -> Result<u64, Failure> {
        let recipients = RecipientSet::new(vec![self.vault.primary.clone()])
            .map_err(|_| Failure::Record(line!()))?;
        let plaintext = encode_content(&object.content).map_err(|_| Failure::Record(line!()))?;
        let ciphertext = self
            .crypto
            .encrypt(plaintext.as_slice(), &recipients)
            .map_err(|_| Failure::Record(line!()))?;
        let existing = self
            .vault
            .storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?;
        if existing.as_ref().map(|secret| secret.revision) != expected {
            return Err(Failure::Record(line!()));
        }
        let (icon, asset) = self.incoming_icon(id, &object, existing.as_ref());
        let new_asset = asset.as_ref().map(|asset| (asset.id, asset.sha256()));
        let storage = &mut self.vault.storage;
        match existing {
            Some(existing) => {
                let expected = existing.revision;
                storage
                    .replace_secret_with_icon(
                        Secret {
                            title: object.title.clone(),
                            kind: object.content.kind(),
                            recipients,
                            ciphertext,
                            favorite: object.favorite,
                            trashed: object.trashed,
                            icon,
                            ..existing
                        },
                        expected,
                        asset,
                    )
                    .map_err(|error| {
                        tracing::warn!(error = %error, stage = "replace", "Vault mirror could not apply a ledger record");
                        Failure::Record(line!())
                    })?;
            }
            None => storage
                .insert_secret_with_icon(
                    Secret {
                        icon,
                        id,
                        folder_id: None,
                        title: object.title.clone(),
                        kind: object.content.kind(),
                        recipients,
                        ciphertext,
                        favorite: object.favorite,
                        trashed: object.trashed,
                        revision: 0,
                    },
                    asset,
                )
                .map_err(|error| {
                    tracing::warn!(error = %error, stage = "insert", "Vault mirror could not apply a ledger record");
                    Failure::Record(line!())
                })?,
        }
        if let (Some((asset_id, sha256)), Some(hashes)) = (new_asset, self.asset_hashes.as_mut()) {
            hashes.insert(asset_id, sha256);
        }
        self.vault
            .storage
            .find_secret(id)
            .map_err(|_| Failure::Record(line!()))?
            .map(|secret| secret.revision)
            .ok_or(Failure::Record(line!()))
    }

    /// Desktop icon for a received object, plus a new image row when its asset is
    /// not stored yet. Automatic objects get the favicon or server icon derived
    /// from the URL. An icon this version cannot read, or an image whose asset has
    /// not arrived, keeps the current Desktop icon; the latter is retried later.
    fn incoming_icon(
        &mut self,
        id: SecretId,
        object: &VaultObject,
        existing: Option<&Secret>,
    ) -> (Option<EntryIcon>, Option<EntryIconAsset>) {
        let current = || match existing {
            Some(existing) => existing.icon.clone(),
            None => automatic_entry_icon(&object.content),
        };
        match &object.icon {
            IconState::Unsupported => {
                tracing::info!(
                    reason = "icon_unsupported",
                    "Vault mirror kept the Desktop icon"
                );
                (current(), None)
            }
            IconState::Automatic | IconState::Known(VaultIcon::Favicon { .. }) => {
                // A phone that never saw a Desktop icon (not publishable, or over the
                // asset budget) sends it back as automatic; that is not a change.
                if let Some(existing) = existing {
                    if !self.icon_visible_to_phone(existing.icon.as_ref()) {
                        return (existing.icon.clone(), None);
                    }
                }
                (automatic_entry_icon(&object.content), None)
            }
            IconState::Known(VaultIcon::Theme { name }) => {
                (Some(EntryIcon::Theme { name: name.clone() }), None)
            }
            IconState::Known(VaultIcon::Image { sha256, .. }) => {
                let Some(sha256) = parse_sha256_hex(sha256) else {
                    return (current(), None);
                };
                if let Some(asset_id) = self.local_asset_for(&sha256) {
                    return (Some(EntryIcon::Image { asset_id }), None);
                }
                let png = match read_vault_asset(self.ledger, &self.crypto, &sha256) {
                    Ok(Some(png)) => png,
                    Ok(None) => {
                        tracing::info!(
                            reason = "asset_missing",
                            "Vault mirror waits for an icon asset"
                        );
                        self.pending_icons.insert(id.0);
                        return (current(), None);
                    }
                    Err(error) => {
                        tracing::warn!(reason = "asset_unreadable", error = ?error, "Vault mirror could not read an icon asset");
                        self.pending_icons.insert(id.0);
                        return (current(), None);
                    }
                };
                // Strict PNG checks already passed; Qt must also decode it as a normalized icon.
                if !crate::infrastructure::entry_icons::valid_normalized_image(&png) {
                    tracing::warn!(
                        reason = "asset_rejected_by_decoder",
                        "Vault mirror rejected an icon asset"
                    );
                    return (current(), None);
                }
                let asset = EntryIconAsset {
                    id: uuid::Uuid::new_v4(),
                    png,
                };
                if let Err(error) = asset.validate() {
                    tracing::warn!(reason = %error, "Vault mirror rejected an icon asset");
                    return (current(), None);
                }
                (Some(EntryIcon::Image { asset_id: asset.id }), Some(asset))
            }
        }
    }

    /// Whether the phone can have received this Desktop icon as it is.
    fn icon_visible_to_phone(&mut self, icon: Option<&EntryIcon>) -> bool {
        match icon {
            None | Some(EntryIcon::Favicon { .. }) => true,
            Some(EntryIcon::Theme { name }) => PUBLISH_ICONS && valid_theme_name(name),
            Some(EntryIcon::Image { asset_id }) => {
                let Some(sha256) = self.hashes().get(asset_id).copied() else {
                    return false;
                };
                PUBLISH_ICONS && self.ledger_has_asset(&sha256)
            }
        }
    }

    fn ledger_has_asset(&self, sha256: &[u8; 32]) -> bool {
        let record_id = asset_record_id(&self.ledger.vault_id(), sha256);
        self.ledger
            .heads(*record_id.as_bytes())
            .unwrap_or_default()
            .iter()
            .any(|head| {
                self.ledger
                    .get(head.revision_id)
                    .ok()
                    .flatten()
                    .is_some_and(|revision| revision.operation == Operation::Write)
            })
    }

    /// Whether the image a pending record waits for is now available.
    fn pending_asset_ready(&mut self, id: SecretId, revision_id: Id) -> Result<bool, Failure> {
        let Some(object) = self.ledger_object(id, revision_id)? else {
            return Ok(true);
        };
        let IconState::Known(VaultIcon::Image { sha256, .. }) = &object.icon else {
            return Ok(true);
        };
        let Some(sha256) = parse_sha256_hex(sha256) else {
            return Ok(true);
        };
        Ok(self.local_asset_for(&sha256).is_some() || self.ledger_has_asset(&sha256))
    }

    fn hashes(&mut self) -> &HashMap<uuid::Uuid, [u8; 32]> {
        if self.asset_hashes.is_none() {
            let hashes = self
                .vault
                .storage
                .icon_asset_hashes()
                .unwrap_or_else(|error| {
                    tracing::warn!(error = %error, "Vault mirror could not hash the icon images");
                    HashMap::new()
                });
            self.asset_hashes = Some(hashes);
        }
        self.asset_hashes.get_or_insert_with(HashMap::new)
    }

    fn local_asset_for(&mut self, sha256: &[u8; 32]) -> Option<uuid::Uuid> {
        self.hashes()
            .iter()
            .find_map(|(id, hash)| (hash == sha256).then_some(*id))
    }

    /// Wire form of a Desktop icon, without publishing anything.
    fn wire_icon(&mut self, icon: Option<&EntryIcon>) -> IconState {
        if !PUBLISH_ICONS {
            return IconState::Automatic;
        }
        match icon {
            None | Some(EntryIcon::Favicon { .. }) => IconState::Automatic,
            Some(EntryIcon::Theme { name }) if valid_theme_name(name) => {
                IconState::Known(VaultIcon::Theme { name: name.clone() })
            }
            Some(EntryIcon::Theme { .. }) => IconState::Automatic,
            Some(EntryIcon::Image { asset_id }) => match self.hashes().get(asset_id) {
                Some(sha256) => IconState::Known(VaultIcon::Image {
                    sha256: sha256_hex(sha256),
                    media_type: VAULT_ICON_MEDIA_TYPE_PNG.to_owned(),
                }),
                None => IconState::Automatic,
            },
        }
    }

    fn current_icon_digest(&mut self, id: SecretId) -> u64 {
        match self.vault.storage.find_secret(id) {
            Ok(Some(secret)) => icon_digest(&self.wire_icon(secret.icon.as_ref())),
            _ => 0,
        }
    }

    /// Reads what `push` will send for this icon, before the ledger is borrowed.
    fn outgoing_icon(&mut self, id: SecretId, icon: Option<&EntryIcon>) -> OutgoingIcon {
        match icon {
            None | Some(EntryIcon::Favicon { .. }) => OutgoingIcon::Automatic,
            Some(EntryIcon::Theme { name }) if valid_theme_name(name) => {
                OutgoingIcon::Theme(name.clone())
            }
            Some(EntryIcon::Theme { .. }) => {
                tracing::info!(
                    reason = "theme_name",
                    "Vault mirror sends a record without its theme icon"
                );
                OutgoingIcon::Automatic
            }
            Some(EntryIcon::Image { asset_id }) => match self.vault.storage.icon_asset(*asset_id) {
                Ok(Some(asset)) => OutgoingIcon::Image(asset.png),
                Ok(None) => {
                    tracing::warn!(reason = "asset_row_missing", record = %id.0, "Vault mirror sends a record without its image icon");
                    OutgoingIcon::Automatic
                }
                Err(error) => {
                    tracing::warn!(reason = %error, record = %id.0, "Vault mirror sends a record without its image icon");
                    OutgoingIcon::Automatic
                }
            },
        }
    }

    /// Keeps a phone version that lost a conflict as a new Desktop record. Its ID
    /// derives from the record and the ledger head, so a retried run never makes
    /// a second copy. It has no mirror state yet; the next run sends it to the phone.
    fn insert_copy(
        &mut self,
        id: SecretId,
        head: Id,
        mut object: VaultObject,
    ) -> Result<(), Failure> {
        let mut digest = Sha256::new();
        digest.update(b"org.kryptopass.mirror-copy.v1\0");
        digest.update(id.0.as_bytes());
        digest.update(head);
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&digest.finalize()[..16]);
        let copy_id = SecretId(uuid::Builder::from_random_bytes(bytes).into_uuid());
        if self
            .vault
            .storage
            .find_secret(copy_id)
            .map_err(|_| Failure::Record(line!()))?
            .is_some()
        {
            return Ok(());
        }
        let mut title = std::mem::take(&mut object.title);
        let room = 512 - COPY_SUFFIX.len();
        if title.len() > room {
            let mut cut = room;
            while !title.is_char_boundary(cut) {
                cut -= 1;
            }
            title.truncate(cut);
        }
        title.push_str(COPY_SUFFIX);
        object.title = title;
        self.upsert_main(copy_id, object, None).map(|_| ())
    }
}

/// Folder names from the root for each folder; a broken parent chain stops at the break.
fn folder_paths(folders: &[Folder]) -> HashMap<FolderId, Vec<String>> {
    let by_id: HashMap<FolderId, &Folder> =
        folders.iter().map(|folder| (folder.id, folder)).collect();
    folders
        .iter()
        .map(|folder| {
            let mut path = Vec::new();
            let mut seen = HashSet::new();
            let mut current = Some(folder);
            while let Some(node) = current {
                if !seen.insert(node.id)
                    || path.len() >= kryptopass_core::MAX_VAULT_OBJECT_FOLDER_DEPTH
                {
                    break;
                }
                path.push(node.name.clone());
                current = node
                    .parent_id
                    .and_then(|parent| by_id.get(&parent).copied());
            }
            path.reverse();
            (folder.id, path)
        })
        .collect()
}

/// Stable digest of a folder path for change detection; 0 means the root.
fn folder_digest(path: &[String]) -> u64 {
    if path.is_empty() {
        return 0;
    }
    let mut digest = Sha256::new();
    for name in path {
        digest.update(name.as_bytes());
        digest.update([0]);
    }
    let digest = digest.finalize();
    u64::from_le_bytes(digest[..8].try_into().unwrap_or_default()).max(1)
}

/// Stable digest of an icon's wire form for change detection; 0 means automatic.
fn icon_digest(icon: &IconState) -> u64 {
    let mut digest = Sha256::new();
    match icon {
        IconState::Automatic | IconState::Known(VaultIcon::Favicon { .. }) => return 0,
        IconState::Unsupported => return u64::MAX,
        IconState::Known(VaultIcon::Theme { name }) => {
            digest.update(b"theme\0");
            digest.update(name.as_bytes());
        }
        IconState::Known(VaultIcon::Image { sha256, media_type }) => {
            digest.update(b"image\0");
            digest.update(sha256.as_bytes());
            digest.update([0]);
            digest.update(media_type.as_bytes());
        }
    }
    let digest = digest.finalize();
    u64::from_le_bytes(digest[..8].try_into().unwrap_or_default()).max(1)
}

fn same(main: &MainRecord, object: &VaultObject) -> bool {
    let (Ok(left), Ok(right)) = (
        encode_content(&main.content),
        encode_content(&object.content),
    ) else {
        return false;
    };
    main.secret.title == object.title
        && main.secret.favorite == object.favorite
        && main.secret.trashed == object.trashed
        && left.as_slice() == right.as_slice()
}

enum Failure {
    /// gpg-agent has no cached passphrase; stop and retry once unlocked.
    Locked,
    /// This record could not be exchanged now; the others continue. Holds the
    /// source line that rejected it, for diagnostics.
    Record(u32),
}

impl From<VaultSyncError> for Failure {
    // A ledger write failure affects only this record; the next run retries it.
    fn from(error: VaultSyncError) -> Self {
        tracing::warn!(error = ?error, "Vault mirror ledger write failed");
        Self::Record(line!())
    }
}
