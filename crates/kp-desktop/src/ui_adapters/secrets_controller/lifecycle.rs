//! Opening, unlocking, locking and switching the vault, and its setup and primary key.

use super::*;

impl qobject::SecretsController {
    pub fn load_store(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }

        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.load_operation = state.load_operation.wrapping_add(1);
            state.load_operation
        };
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::from("Cargando bóveda…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            || {
                let snapshot = load_store_snapshot()?;
                let primary = read_primary_key(&open_session()?)?;
                let setup = resolve_setup_state(&snapshot, primary.is_some())?;
                Ok((snapshot, setup, primary))
            },
            move |mut controller,
                  result: anyhow::Result<(
                StoreSnapshot,
                Option<app_paths::SetupState>,
                Option<PrimaryKey>,
            )>| {
                if controller.as_ref().rust().load_operation != operation {
                    return;
                }

                controller.as_mut().set_busy(false);
                match result {
                    Ok((snapshot, setup, primary)) => {
                        controller.as_mut().apply_setup_state(setup.is_some());
                        controller.as_mut().apply_primary_key(primary);
                        controller.as_mut().publish_snapshot(snapshot);
                        controller.as_mut().set_load_error(QString::default());
                        controller
                            .as_mut()
                            .set_status(QString::from("Bóveda cargada"));
                    }
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "the vault did not load");
                        controller
                            .as_mut()
                            .set_load_error(QString::from("No se pudo cargar la bóveda."));
                        controller.as_mut().fail("No se pudo cargar la bóveda.");
                    }
                }
            },
        );
    }

    pub fn set_active_vault(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine la operación actual antes de cambiar de bóveda.");
            return;
        }

        let Some(local_path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona un archivo local de bóveda existente.");
            return;
        };
        let path = std::path::PathBuf::from(local_path.to_string());
        if !path.is_absolute() {
            self.as_mut()
                .fail("Selecciona un archivo local de bóveda existente.");
            return;
        }

        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.active_vault_operation = state.active_vault_operation.wrapping_add(1);
            state.load_operation = state.load_operation.wrapping_add(1);
            state.active_vault_operation
        };
        self.as_mut().cancel_unlock();
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().set_locked(true);
        self.as_mut().vault_locked();
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut()
            .set_status(QString::from("Validando la bóveda restaurada…"));

        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || activate_store_snapshot(path),
            move |mut controller, result| {
                if controller.as_ref().rust().active_vault_operation != operation {
                    return;
                }
                controller.as_mut().set_busy(false);
                match result {
                    Ok((snapshot, durable, primary)) => {
                        controller.as_mut().apply_primary_key(primary);
                        controller.as_mut().publish_snapshot(snapshot);
                        let status = if durable {
                            "Bóveda restaurada lista. Desbloquéala para acceder a sus secretos."
                        } else {
                            "Bóveda seleccionada, pero no se pudo confirmar la persistencia de la preferencia."
                        };
                        controller.as_mut().set_status(QString::from(status));
                        controller.as_mut().vault_activated();
                    }
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo abrir la bóveda restaurada. La bóveda activa no cambió."),
                }
            },
        );
    }

    pub fn lock_vault(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine el guardado antes de bloquear la bóveda.");
            return;
        }
        self.as_mut().cancel_unlock();
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().clear_error();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().set_status(QString::from("Bóveda bloqueada"));
        self.as_mut().vault_locked();
    }

    pub fn unlock_vault(mut self: Pin<&mut Self>) {
        if self.as_ref().rust().session_blocked {
            self.as_mut()
                .fail("Desbloquea la sesión del escritorio antes de acceder a la bóveda.");
            return;
        }
        if *self.busy() || *self.saving() {
            self.as_mut()
                .fail("Espera a que termine la operación antes de acceder a la bóveda.");
            return;
        }
        if !*self.locked() || *self.unlocking() {
            return;
        }
        self.as_mut().clear_error();
        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.unlock_operation = state.unlock_operation.wrapping_add(1);
            state.unlock_operation
        };
        self.as_mut().set_unlocking(true);
        self.as_mut()
            .set_status(QString::from("Comprobando tu clave OpenPGP…"));
        let primary = self
            .as_ref()
            .rust()
            .primary
            .as_ref()
            .map(|key| key.fingerprint.clone());
        // The vault stays covered until GnuPG proves access; pinentry may ask here.
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || verify_vault_access(primary.as_ref()),
            move |mut controller, result| {
                if controller.as_ref().rust().unlock_operation != operation {
                    return;
                }
                controller.as_mut().set_unlocking(false);
                if controller.as_ref().rust().session_blocked {
                    return;
                }
                match result {
                    Ok(()) => {
                        controller.as_mut().set_locked(false);
                        controller.as_mut().set_status(QString::from(
                            "Bóveda disponible. El contenido se abrirá al seleccionarlo.",
                        ));
                    }
                    Err(error) if error.is::<NoUnlockKey>() => controller.as_mut().fail(
                        "No hay en este equipo una clave privada de esta bóveda. Importa tu clave para desbloquearla.",
                    ),
                    Err(_) => controller
                        .as_mut()
                        .fail("No se pudo verificar tu clave. La bóveda sigue bloqueada."),
                }
            },
        );
    }

    /// Finishes the first run, recording the chosen key as the vault's primary key.
    ///
    /// A key created only for KryptoPass is forgotten on lock; a personal key is
    /// left alone until the user opts in, because the cache is shared by its keygrip.
    pub fn complete_setup(mut self: Pin<&mut Self>, primary_key: &QString, dedicated: bool) {
        if !primary_key.to_string().trim().is_empty()
            && !self.as_mut().store_primary_key(primary_key, dedicated)
        {
            return;
        }
        let state = app_paths::SetupState {
            version: SETUP_VERSION,
        };
        if app_paths::save_setup_state(crate::app_info::APP.desktop_file_id, &state).is_err() {
            self.as_mut()
                .fail("No se pudo guardar la configuración inicial.");
            return;
        }
        self.apply_setup_state(true);
    }

    /// Changes the default recipient for new items; existing items keep their keys.
    pub fn set_primary_key(mut self: Pin<&mut Self>, primary_key: &QString) {
        self.as_mut().store_primary_key(primary_key, false);
    }

    pub fn set_forget_on_lock(mut self: Pin<&mut Self>, enabled: bool) {
        let Some(mut key) = self.as_ref().rust().primary.clone() else {
            return;
        };
        key.forget_on_lock = enabled;
        if write_primary_key(&key).is_err() {
            self.as_mut()
                .fail("No se pudo guardar la preferencia de la clave principal.");
            return;
        }
        self.apply_primary_key(Some(key));
    }

    pub(super) fn store_primary_key(
        mut self: Pin<&mut Self>,
        primary_key: &QString,
        forget_on_lock: bool,
    ) -> bool {
        let Ok(fingerprint) = KeyFingerprint::parse(&primary_key.to_string()) else {
            self.as_mut()
                .fail("La clave elegida para la bóveda no es válida.");
            return false;
        };
        let key = PrimaryKey {
            fingerprint,
            forget_on_lock,
        };
        if write_primary_key(&key).is_err() {
            self.as_mut()
                .fail("No se pudo guardar la clave principal de la bóveda.");
            return false;
        }
        self.apply_primary_key(Some(key));
        true
    }

    pub(super) fn apply_setup_state(mut self: Pin<&mut Self>, completed: bool) {
        if *self.setup_required() == completed {
            self.as_mut().rust_mut().setup_required = !completed;
            self.as_mut().setup_required_changed();
        }
    }

    pub(super) fn apply_primary_key(mut self: Pin<&mut Self>, key: Option<PrimaryKey>) {
        let value = QString::from(key.as_ref().map_or("", |key| key.fingerprint.as_str()));
        let forget = key.as_ref().is_some_and(|key| key.forget_on_lock);
        self.as_mut().rust_mut().primary = key;
        if *self.primary_key() != value {
            self.as_mut().rust_mut().primary_key = value;
            self.as_mut().primary_key_changed();
        }
        if *self.forget_on_lock() != forget {
            self.as_mut().rust_mut().forget_on_lock = forget;
            self.forget_on_lock_changed();
        }
    }

    /// Clears gpg-agent's cache for the primary key when the vault asks for it.
    pub(super) fn forget_primary_passphrase(self: Pin<&mut Self>) {
        let primary = self
            .as_ref()
            .rust()
            .primary
            .clone()
            .filter(|key| key.forget_on_lock);
        std::thread::spawn(move || {
            let mut fingerprints = std::collections::BTreeSet::new();
            if let Some(key) = primary {
                fingerprints.insert(key.fingerprint.as_str().to_owned());
            }
            match crate::keyring::catalog::read() {
                Ok(catalog) => fingerprints.extend(
                    catalog
                        .identities
                        .into_iter()
                        .filter(|(_, identity)| identity.forget_on_lock)
                        .map(|(fingerprint, _)| fingerprint),
                ),
                Err(error) => tracing::warn!(%error, "could not read identity cache preferences"),
            }
            for fingerprint in fingerprints {
                if let Ok(fingerprint) = KeyFingerprint::parse(&fingerprint)
                    && let Err(error) = GnuPgOpenPgp.forget_cached_passphrase(&fingerprint)
                {
                    tracing::warn!(%error, "could not make gpg-agent forget an identity");
                }
            }
        });
    }

    pub(super) fn cancel_unlock(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.unlock_operation = state.unlock_operation.wrapping_add(1);
        }
        self.set_unlocking(false);
    }

    pub fn set_session_blocked(mut self: Pin<&mut Self>, blocked: bool) {
        self.as_mut().rust_mut().session_blocked = blocked;
    }

    pub fn lock_for_session(mut self: Pin<&mut Self>) {
        // In-flight writes may finish, but access and read projections expire now.
        self.as_mut().cancel_unlock();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().clear_sensitive_state_contents();
        unsafe { super::super::native_ffi::ffi::clear_owned() };
        self.as_mut().clear_error();
        self.as_mut().set_status(QString::from("Bóveda bloqueada"));
        self.as_mut().vault_locked();
    }

    pub fn clear_sensitive_state(mut self: Pin<&mut Self>) {
        if *self.busy() || *self.saving() {
            return;
        }
        self.as_mut().cancel_unlock();
        self.as_mut().clear_sensitive_state_contents();
        self.as_mut().set_locked(true);
        self.as_mut().forget_primary_passphrase();
        self.as_mut().vault_locked();
    }
}

#[derive(Debug)]
pub(super) struct NoUnlockKey;

impl std::fmt::Display for NoUnlockKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("no local secret key protects this vault")
    }
}

impl std::error::Error for NoUnlockKey {}

/// Proves access with a fresh in-memory challenge for the vault's local secret keys.
///
/// No entry is read, so a damaged credential or one encrypted only for an
/// absent key cannot block the vault. When the primary key is usable here the
/// challenge goes to it alone, so another cached key cannot skip a forgotten
/// passphrase. Without one, an empty vault has nothing to guard.
pub(super) fn verify_vault_access(primary: Option<&KeyFingerprint>) -> anyhow::Result<()> {
    let crypto = GnuPgOpenPgp;
    let local = crypto.usable_secret_fingerprints()?;
    let owned: Vec<KeyFingerprint> = match primary.filter(|key| local.contains(key)) {
        Some(primary) => vec![primary.clone()],
        None => {
            let metadata = open_session()?.vault().list_items()?;
            if metadata.is_empty() {
                return Ok(());
            }
            metadata
                .iter()
                .flat_map(|entry| entry.state.recipients.as_slice().iter())
                .filter(|fingerprint| local.contains(fingerprint))
                .cloned()
                .collect()
        }
    };
    let recipients = RecipientSet::new(owned).map_err(|_| NoUnlockKey)?;
    let mut challenge = Vec::with_capacity(32);
    challenge.extend_from_slice(Uuid::new_v4().as_bytes());
    challenge.extend_from_slice(Uuid::new_v4().as_bytes());
    let ciphertext = crypto.encrypt(&challenge, &recipients)?;
    let plaintext = crypto.decrypt(&ciphertext)?;
    // What arrived through Sync while locked becomes listable with the key open.
    let indexed = crate::vault_access::open_background()
        .and_then(|mut vault| Ok(kryptopass_app::index_received(&mut vault)?));
    if let Err(error) = indexed {
        tracing::warn!(error = %format!("{error:#}"), "versions were not indexed after unlocking");
    }
    anyhow::ensure!(
        plaintext.as_slice() == challenge.as_slice(),
        "unlock challenge mismatch"
    );
    Ok(())
}

/// The vault's primary key: its single default recipient, with its settings.
pub(super) fn read_primary_key(session: &VaultSession) -> anyhow::Result<Option<PrimaryKey>> {
    let Some(recipients) = session.vault().default_recipients()? else {
        return Ok(None);
    };
    let Some(fingerprint) = recipients.as_slice().first().cloned() else {
        return Ok(None);
    };
    let forget_on_lock = session.vault().settings_of(&fingerprint)?.forget_on_lock;
    Ok(Some(PrimaryKey {
        fingerprint,
        forget_on_lock,
    }))
}

pub(super) fn write_primary_key(key: &PrimaryKey) -> anyhow::Result<()> {
    let mut session = open_session()?;
    let vault = session.vault_mut();
    vault.set_default_recipients(Some(RecipientSet::new(vec![key.fingerprint.clone()])?))?;
    let mut settings = KeySettings::new(key.fingerprint.clone());
    settings.forget_on_lock = key.forget_on_lock;
    vault.set_key_settings(settings)?;
    Ok(())
}

pub(super) fn load_store_snapshot() -> anyhow::Result<StoreSnapshot> {
    let snapshot = read_store_snapshot()?;
    crate::infrastructure::browser::configure(&crate::vault_access::vault_path()?)?;
    Ok(snapshot)
}

/// Bumped when the vault format changes: a mark from before it belongs to a
/// vault this version does not open, so the first run must be offered again.
pub(super) const SETUP_VERSION: u32 = 2;

/// Setup is pending on a fresh installation; a vault with data predates it.
pub(super) fn resolve_setup_state(
    snapshot: &StoreSnapshot,
    has_primary: bool,
) -> anyhow::Result<Option<app_paths::SetupState>> {
    let app_id = crate::app_info::APP.desktop_file_id;
    match app_paths::setup_state(app_id) {
        Ok(Some(state)) if state.version >= SETUP_VERSION => return Ok(Some(state)),
        Ok(_) => {}
        // A damaged preference must not lock the user out of the vault.
        Err(error) => {
            tracing::warn!(%error, "ignoring unreadable setup state");
            return Ok(Some(app_paths::SetupState {
                version: SETUP_VERSION,
            }));
        }
    }
    if snapshot.entries.is_empty() && snapshot.folders.is_empty() && !has_primary {
        return Ok(None);
    }
    let state = app_paths::SetupState {
        version: SETUP_VERSION,
    };
    app_paths::save_setup_state(app_id, &state)?;
    Ok(Some(state))
}
