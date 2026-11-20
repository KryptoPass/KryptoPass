//! Vault Sync per device: which keys it syncs, its rounds and the key it waits for.

use super::*;

const SYNC_PROFILE_MESSAGE: &str =
    crate::i18n::tr_noop("This computer's Sync settings could not be accessed.");

/// The Sync binding of `peer`: the vault's store and the keys chosen for it.
/// `None` runs the session without Sync, as it does for a peer without keys.
pub(super) fn sync_binding(
    directory: &std::path::Path,
    peer: DeviceId,
) -> Option<kryptopass_devices::SyncSessionBinding> {
    let keys = match SyncSelection::in_profile(directory).keys(&peer.to_string()) {
        Ok(keys) => keys,
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "Sync selection unreadable; session runs without Sync");
            return None;
        }
    };
    if keys.is_empty() {
        return None;
    }
    let keys: Vec<kryptopass_sync::Fingerprint> = keys
        .iter()
        .filter_map(|key| {
            KeyFingerprint::parse(key)
                .ok()
                .and_then(|key| hex_bytes(key.as_str()))
                .and_then(|bytes| kryptopass_sync::Fingerprint::new(bytes).ok())
        })
        .collect();
    let binding = (|| -> anyhow::Result<_> {
        let identity = crate::vault_access::identity()?;
        let trust = kryptopass_peers::ProfileTrust::new(
            identity.public_identity(),
            directory.to_path_buf(),
        );
        Ok(kryptopass_devices::SyncSessionBinding::new(
            Box::new(crate::vault_access::open_sync_store()?),
            Box::new(trust),
            keys,
        )
        .with_automatic_rounds())
    })();
    match binding {
        Ok(binding) => Some(binding),
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "Sync store unavailable; session runs without Sync");
            None
        }
    }
}

pub(super) fn sync_binder(directory: PathBuf) -> kryptopass_peers::SyncBinder {
    Arc::new(move |peer| sync_binding(&directory, peer))
}

impl qobject::PeersController {
    /// Makes `device_id` sync the active vault. The phone renegotiates the
    /// session when the user enables it there too.
    pub fn enable_vault_sync(self: Pin<&mut Self>, device_id: QString, fingerprints: QString) {
        let keys: Vec<KeyFingerprint> = fingerprints
            .to_string()
            .split(',')
            .filter(|text| !text.is_empty())
            .filter_map(|text| KeyFingerprint::parse(text).ok())
            .collect();
        self.change_vault_sync(device_id, Some(keys));
    }

    /// Stops syncing with `device_id`; the vault and the ledger history stay.
    pub fn disable_vault_sync(self: Pin<&mut Self>, device_id: QString) {
        self.change_vault_sync(device_id, None);
    }

    /// `keys` enables Sync for those vault keys; `None` disables it.
    fn change_vault_sync(
        mut self: Pin<&mut Self>,
        device_id: QString,
        keys: Option<Vec<KeyFingerprint>>,
    ) {
        let enabled = keys.is_some();
        let Some(peer) = parse_device_id(&device_id.to_string()) else {
            return;
        };
        let Ok(directory) = peers_directory() else {
            self.as_mut().set_vault_sync_failed(true);
            self.as_mut()
                .set_vault_sync_message(QString::from(SYNC_PROFILE_MESSAGE));
            return;
        };
        self.as_mut().set_vault_sync_failed(false);
        self.as_mut().set_vault_sync_message(QString::default());
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let keys = keys
                .unwrap_or_default()
                .iter()
                .map(|key| key.as_str().to_owned())
                .collect();
            let result = SyncSelection::in_profile(&directory)
                .set(&peer.to_string(), keys)
                .map_err(|error| {
                    tracing::warn!(error = %format!("{error:#}"), "Vault sync selection not saved");
                    SYNC_PROFILE_MESSAGE
                });
            tracing::info!(
                enabled,
                failed = result.is_err(),
                "Vault sync selection changed"
            );
            let failed = result.is_err();
            let _ = thread.queue(move |mut object| {
                object.as_mut().set_vault_sync_failed(failed);
                object.as_mut().set_vault_sync_message(match result {
                    Ok(()) if enabled => QString::from(
                        crate::i18n::tr_noop("Sync enabled. If the phone does not have it enabled yet, enable it there: Devices → this Desktop → Sync my vault."),
                    ),
                    Ok(()) => QString::from(crate::i18n::tr_noop("Sync turned off. The vault did not change.")),
                    Err(message) => QString::from(message),
                });
                object.as_mut().refresh_vault_sync_peers();
                // A live session only reads its Sync binding when it starts.
                if !failed {
                    if let Some(owner) = object.rust().resume_owner.as_ref() {
                        owner.renegotiate_peer(peer);
                    }
                }
            });
        });
    }

    /// Makes what Sync sessions received listable. The exchange itself runs in
    /// each session; QML calls this periodically and overlapping calls are ignored.
    pub fn run_vault_mirror(mut self: Pin<&mut Self>) {
        if self.rust().vault_mirror_running || self.rust().vault_sync_peers.is_empty() {
            return;
        }
        self.as_mut().rust_mut().vault_mirror_running = true;
        self.as_mut().set_vault_mirror_active(true);
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = crate::vault_access::open_background()
                .and_then(|mut vault| Ok(kryptopass_app::index_received_report(&mut vault)?));
            let _ = thread.queue(move |mut object| {
                object.as_mut().rust_mut().vault_mirror_running = false;
                object.as_mut().set_vault_mirror_active(false);
                match result {
                    Ok(report) => {
                        let indexed = report.indexed;
                        object.as_mut().set_vault_sync_failed(false);
                        // A locked key does not stop Sync; it leaves what arrived for it unlisted.
                        // Say which key once when that starts, and once when it ends.
                        let waiting = report
                            .waiting_for
                            .as_ref()
                            .map(|key| key.as_str().to_uppercase())
                            .unwrap_or_default();
                        if waiting != object.rust().vault_sync_locked_key.to_string() {
                            if waiting.is_empty() {
                                tracing::info!("Vault sync: received items no longer wait for a passphrase");
                            } else {
                                tracing::warn!(
                                    key = %&waiting[waiting.len().saturating_sub(16)..],
                                    indexed,
                                    "Vault sync: received items wait for a key passphrase; Sync continues"
                                );
                            }
                            object
                                .as_mut()
                                .set_vault_sync_locked_key(QString::from(waiting));
                        }
                        if indexed > 0 {
                            tracing::info!(indexed, "Vault sync: received versions indexed");
                            object.as_mut().vault_mirror_pulled();
                            let activity = object.rust().vault_sync_activity.wrapping_add(1);
                            object.as_mut().set_vault_sync_activity(activity);
                            object.as_mut().set_vault_sync_message(QString::from(crate::i18n::trf(
                                "Vault synced: %1 received.",
                                &[indexed.to_string()],
                            )));
                        }
                    }
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "Vault sync: indexing failed");
                        object.as_mut().set_vault_sync_failed(true);
                        object
                            .as_mut()
                            .set_vault_sync_message(QString::from(SYNC_PROFILE_MESSAGE));
                    }
                }
            });
        });
    }

    /// A vault key reached the phone: if Sync is on for it, the items of that key now sync
    /// too, with no second step. Sending the private key is the user's explicit consent.
    pub(super) fn sync_key_after_send(mut self: Pin<&mut Self>, peer: DeviceId) {
        let Some((sent_peer, fingerprint)) = self.rust().vault_key_send.clone() else {
            return;
        };
        if sent_peer != peer {
            return;
        }
        self.as_mut().rust_mut().vault_key_send = None;
        let (Ok(key), Ok(directory)) = (KeyFingerprint::parse(&fingerprint), peers_directory())
        else {
            return;
        };
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = SyncSelection::in_profile(&directory).add(&peer.to_string(), key.as_str());
            tracing::info!(added = ?result.as_ref().ok(), failed = result.is_err(), "Vault sync: key added after sending it to the phone");
            if let Ok(true) = result {
                let _ =
                    thread.queue(move |mut object| {
                        object.as_mut().set_vault_sync_failed(false);
                        object.as_mut().set_vault_sync_message(QString::from(
                            crate::i18n::tr_noop("Key sent: its items now sync with the phone."),
                        ));
                        object.as_mut().refresh_vault_sync_peers();
                    });
            }
        });
    }

    /// Asks the live session with `device_id` for one more Sync round. Resyncing is not
    /// reconnecting: the session stays up. A changed key binding renegotiates on its own.
    pub fn retry_vault_sync(self: Pin<&mut Self>, device_id: QString) {
        let Some(peer) = parse_device_id(&device_id.to_string()) else {
            return;
        };
        tracing::info!("Vault sync: round requested by the user");
        if let Some(owner) = self.rust().resume_owner.as_ref() {
            owner.resync_peer(peer);
        }
    }

    /// Asks gpg-agent for the passphrase of one vault key, so a Sync that stopped because
    /// that key was locked can continue. Pinentry shows the prompt.
    pub fn unlock_sync_key(mut self: Pin<&mut Self>, fingerprint: QString) {
        let fingerprint = fingerprint.to_string();
        if fingerprint.is_empty() {
            return;
        }
        tracing::info!(key = %&fingerprint[fingerprint.len().saturating_sub(16)..], "Vault sync: asking for a key passphrase");
        self.as_mut().set_vault_sync_failed(false);
        self.as_mut().set_vault_sync_message(QString::default());
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || crate::keyring::commands::probe_agent_for(&fingerprint),
            |controller, result| match result {
                Ok(()) => {
                    tracing::info!("Vault sync: key passphrase cached");
                    let mut controller = controller;
                    controller
                        .as_mut()
                        .set_vault_sync_locked_key(QString::default());
                    controller.as_mut().set_vault_sync_message(QString::from(
                        crate::i18n::tr_noop(
                            "Key unlocked. Syncing will continue in a few seconds.",
                        ),
                    ));
                }
                Err(error) => {
                    tracing::warn!(error = %error, "Vault sync: could not unlock the key");
                    let mut controller = controller;
                    controller.as_mut().set_vault_sync_failed(true);
                    controller
                        .as_mut()
                        .set_vault_sync_message(QString::from(error));
                }
            },
        );
    }

    /// Keys (upper-case hex) that `peer` declared holding in this connection;
    /// `None` while it is offline or runs a version that does not declare them.
    pub(super) fn refresh_vault_sync_peers(self: Pin<&mut Self>) {
        let Ok(directory) = peers_directory() else {
            return;
        };
        let peers: Vec<DeviceId> = self
            .rust()
            .known_peers
            .iter()
            .map(|peer| peer.device_id)
            .collect();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let selections: Vec<(DeviceId, Vec<String>)> = peers
                .into_iter()
                .map(|peer| {
                    let keys = SyncSelection::in_profile(&directory)
                        .keys(&peer.to_string())
                        .unwrap_or_default();
                    (peer, keys)
                })
                .filter(|(_, keys)| !keys.is_empty())
                .collect();
            let keys_by_device: serde_json::Map<String, serde_json::Value> = selections
                .iter()
                .map(|(peer, keys)| (peer.to_string(), serde_json::json!(keys)))
                .collect();
            let enabled: QStringList = selections
                .iter()
                .map(|(peer, _)| QString::from(peer.to_string()))
                .collect();
            let keys_json = serde_json::Value::Object(keys_by_device).to_string();
            let _ = thread.queue(move |mut object| {
                object
                    .as_mut()
                    .set_vault_sync_keys(QString::from(keys_json));
                object.as_mut().set_vault_sync_peers(enabled);
            });
        });
    }
}
