//! Opening an item for editing and saving it, with its icon, SSH key and TOTP.

use super::*;

impl qobject::SecretsController {
    pub fn open_entry(mut self: Pin<&mut Self>, id: &QString) {
        if *self.busy() || *self.saving() || *self.locked() {
            return;
        }
        let id = id.to_string();
        let operation = self.as_mut().begin_editor_operation();
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
        self.as_mut().clear_error();

        if id.is_empty() {
            let draft = match new_editor_projection() {
                Ok(draft) => draft,
                Err(_) => {
                    self.as_mut()
                        .fail_detail("No se pudo preparar el elemento para editarlo.");
                    return;
                }
            };
            self.as_mut().rust_mut().editor_session = Some(EditorSession {
                id: None,
                revision: None,
            });
            self.as_mut().set_editor_json(QString::from(draft.as_str()));
            self.as_mut().set_editor_busy(false);
            self.as_mut().editor_ready();
            return;
        }

        let Some(secret_id) = parse_secret_id(&id) else {
            self.as_mut()
                .fail_detail("No se pudo abrir el elemento para editarlo.");
            return;
        };
        self.as_mut().set_editor_busy(true);
        self.as_mut()
            .set_status(QString::from("Abriendo elemento…"));

        let cancel_epoch = self.as_ref().rust().editor_cancel_epoch.clone();
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<OpenedEntry>> {
                let entry = load_editor_entry(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some(entry))
                } else {
                    drop(entry);
                    Ok(None)
                }
            },
            move |mut controller, result| {
                if controller.as_ref().rust().editor_operation != operation {
                    return;
                }

                controller.as_mut().set_editor_busy(false);
                match result {
                    Ok(Some(entry)) => {
                        let draft = editor_projection(&entry);
                        match draft {
                            Ok(draft) => {
                                controller.as_mut().rust_mut().editor_content =
                                    Some(entry.content.clone());
                                controller.as_mut().rust_mut().editor_session =
                                    Some(EditorSession {
                                        id: Some(entry.id),
                                        revision: Some(entry.revision),
                                    });
                                controller
                                    .as_mut()
                                    .set_editor_json(QString::from(draft.as_str()));
                                controller
                                    .as_mut()
                                    .set_status(QString::from("Elemento listo para editar"));
                                controller.as_mut().editor_ready();
                            }
                            Err(_) => controller
                                .as_mut()
                                .fail_detail("No se pudo preparar el elemento para editarlo."),
                        }
                    }
                    Ok(None) => (),
                    Err(_) => controller
                        .as_mut()
                        .fail_detail("No se pudo abrir el elemento para editarlo."),
                }
            },
        );
    }

    pub fn save_entry(mut self: Pin<&mut Self>, draft_json: &QString) {
        if *self.locked() {
            self.as_mut()
                .fail("Desbloquea la bóveda antes de guardar un elemento.");
            return;
        }
        if *self.busy() {
            return;
        }

        let Some(session) = self.as_ref().rust().editor_session else {
            self.as_mut()
                .fail("No hay un borrador abierto para guardar.");
            return;
        };
        if *self.editor_busy() || *self.icon_busy() {
            return;
        }

        let draft_json = SensitiveText::from(draft_json.to_string());
        if draft_json.len() > kryptopass_core::MAX_CONTENT_BYTES {
            self.as_mut()
                .fail("El borrador supera el tamaño permitido.");
            return;
        }
        let mut draft: EntryDraft = match serde_json::from_str(draft_json.as_str()) {
            Ok(draft) => draft,
            Err(_) => {
                self.as_mut().fail("El borrador del elemento no es válido.");
                return;
            }
        };

        if !draft_matches_session(&draft, session.id, session.revision) {
            self.as_mut()
                .fail("El borrador ya no corresponde al elemento abierto.");
            return;
        }

        if let Some(token) = draft.ssh_import_token() {
            let object = self.as_ref();
            let state = object.rust();
            let staged = state
                .staged_ssh
                .as_ref()
                .filter(|(staged_token, _)| staged_token == token)
                .map(|(_, key)| key.clone());
            let has_saved_key = state.editor_content.is_some();
            if token.is_empty() && !has_saved_key {
                self.as_mut()
                    .fail("Importa una clave SSH antes de guardar el elemento.");
                return;
            }
            match staged {
                Some(key) => draft.attach_ssh(key),
                None if token.is_empty() => {}
                None => {
                    self.as_mut()
                        .fail("Importa una clave SSH antes de guardar el elemento.");
                    return;
                }
            }
        }
        let base = self.as_ref().rust().editor_content.clone();
        let prepared = match prepare_save(&mut draft, session.id, session.revision, base) {
            Ok(prepared) => prepared,
            Err(error) => {
                tracing::debug!(error = %format!("{error:#}"), "draft refused before saving");
                self.as_mut()
                    .fail("Revisa los datos del elemento y quién tiene acceso.");
                return;
            }
        };
        let operation = self.as_ref().rust().editor_operation;
        let staged_icon = self.as_ref().rust().staged_icon.clone();
        // A refusal for the item's fields is explained with this template's labels.
        let template = prepared.content.template().as_str().to_owned();
        self.as_mut().start_mutation(
            "Guardando elemento…",
            true,
            "No se pudo guardar el elemento.",
            move || {
                let mut session = open_session()?;
                let vault = session.vault_mut();
                let icon = prepared.icon_update.and_then(|icon| match icon {
                    Some(EntryIcon::Theme { name }) => Some(IconChoice::Theme(name)),
                    Some(EntryIcon::Image { asset_id }) => staged_icon
                        .filter(|asset| asset.id == asset_id)
                        .map(IconChoice::Image),
                    Some(EntryIcon::Favicon { .. }) | None => Some(IconChoice::Automatic),
                });
                let id = match (prepared.id, prepared.expected_revision) {
                    (Some(id), Some(revision)) => {
                        let mut edit = ItemEdit::new(id, revision)
                            .title(prepared.title)
                            .folder(prepared.folder)
                            .content(prepared.content);
                        if let Some(icon) = icon {
                            edit = edit.icon(icon);
                        }
                        if let Some(link) = prepared.link_update {
                            edit = edit.link(link);
                        }
                        let meta = vault
                            .update_item(edit)
                            .map_err(|error| item_issues::explain(error, &template))?;
                        if meta.state.recipients != prepared.recipients {
                            vault.change_recipients(id, meta.revision, prepared.recipients)?;
                        }
                        id
                    }
                    _ => {
                        vault
                            .create_item(NewItem {
                                folder: prepared.folder,
                                title: prepared.title,
                                content: prepared.content,
                                recipients: Recipients::Explicit(prepared.recipients),
                                icon: icon.unwrap_or(IconChoice::Automatic),
                                link: prepared.link_update.flatten(),
                            })
                            .map_err(|error| item_issues::explain(error, &template))?
                            .id
                    }
                };
                drop(session);
                let snapshot = snapshot_after_mutation();
                Ok(MutationResult {
                    snapshot,
                    signal: MutationSignal::EntrySaved(id.0.to_string()),
                    editor_operation: Some(operation),
                })
            },
        );
    }

    pub fn prepare_entry_icon(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.locked()
            || *self.busy()
            || *self.editor_busy()
            || *self.icon_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut().fail("Elige una imagen local.");
            return;
        };
        let operation = self.as_ref().rust().editor_operation;
        self.as_mut().set_icon_busy(true);
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = crate::infrastructure::entry_icons::prepare_image(std::path::Path::new(
                &path.to_string(),
            ));
            let _ = qt_thread.queue(move |mut controller| {
                if controller.as_ref().rust().editor_operation != operation { return; }
                controller.as_mut().set_icon_busy(false);
                match result {
                    Ok(asset) => {
                        crate::infrastructure::entry_icons::stage(&asset);
                        let icon = EntryIcon::Image { asset_id: asset.id };
                        let json = serde_json::json!({ "icon": icon, "iconSource": crate::infrastructure::entry_icons::source_for(Some(&icon)) }).to_string();
                        controller.as_mut().rust_mut().staged_icon = Some(asset);
                        controller.as_mut().icon_prepared(&QString::from(json));
                    }
                    Err(_) => controller.as_mut().fail("No se pudo usar esa imagen. Elige un PNG, JPEG o WebP estático de hasta 4 MB, con lados de hasta 4096 píxeles y un máximo de 4 millones de píxeles."),
                }
            });
        });
    }

    pub fn prepare_ssh_key(mut self: Pin<&mut Self>, path: &QUrl, passphrase: &QString) {
        if *self.locked()
            || *self.busy()
            || *self.saving()
            || *self.editor_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona un archivo local de clave privada SSH.");
            return;
        };
        let path = std::path::PathBuf::from(path.to_string());
        let passphrase = SensitiveText::from(passphrase.to_string());
        let (editor_operation, prepare_operation) = {
            let mut state = self.as_mut().rust_mut();
            state.prepare_operation = state.prepare_operation.wrapping_add(1);
            (state.editor_operation, state.prepare_operation)
        };
        self.as_mut().clear_error();
        self.as_mut().set_editor_busy(true);
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<_> {
                use std::io::Read;
                let file = std::fs::File::open(path)?;
                anyhow::ensure!(file.metadata()?.is_file(), "select a regular key file");
                let mut raw = SensitiveBytes::default();
                std::io::copy(&mut file.take(256 * 1024 + 1), &mut raw)?;
                let private_key = SensitiveText::from_utf8(raw)?;
                anyhow::ensure!(
                    private_key.len() <= 256 * 1024,
                    "SSH key exceeds size limit"
                );
                let info = inspect_ssh_key(&private_key)
                    .ok_or_else(|| anyhow::anyhow!("not an OpenSSH private key"))?;
                Ok((
                    info,
                    SshKeyMaterial {
                        private_key,
                        passphrase,
                    },
                ))
            },
            move |mut controller, result| {
                let object = controller.as_ref();
                let state = object.rust();
                if state.editor_operation != editor_operation
                    || state.prepare_operation != prepare_operation
                    || state.editor_session.is_none()
                    || *controller.locked()
                {
                    return;
                }
                controller.as_mut().set_editor_busy(false);
                match result {
                Ok((info, key)) => {
                    let token = Uuid::new_v4().to_string();
                    let json = serde_json::json!({"importToken": token, "publicKey": info.public_key,
                        "fingerprint": info.fingerprint, "algorithm": info.algorithm}).to_string();
                    controller.as_mut().rust_mut().staged_ssh = Some((token, key));
                    controller.as_mut().ssh_key_prepared(&QString::from(json));
                }
                Err(_) => controller.as_mut().fail("No se pudo importar la clave SSH. Usa una clave privada OpenSSH válida y revisa su frase de contraseña."),
            }
            },
        );
    }

    pub fn prepare_totp_qr(mut self: Pin<&mut Self>, path: &QUrl) {
        if *self.locked()
            || *self.busy()
            || *self.saving()
            || *self.editor_busy()
            || self.as_ref().rust().editor_session.is_none()
        {
            return;
        }
        let Some(path) = path.to_local_file() else {
            self.as_mut()
                .fail("Selecciona una imagen local con el QR del autenticador.");
            return;
        };
        let path = std::path::PathBuf::from(path.to_string());
        let (editor_operation, prepare_operation) = {
            let mut state = self.as_mut().rust_mut();
            state.prepare_operation = state.prepare_operation.wrapping_add(1);
            (state.editor_operation, state.prepare_operation)
        };
        self.as_mut().clear_error();
        self.as_mut().set_editor_busy(true);
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || super::super::secret_qr::read_totp(&path),
            move |mut controller, result| {
                let object = controller.as_ref();
                let state = object.rust();
                if state.editor_operation != editor_operation
                    || state.prepare_operation != prepare_operation
                    || state.editor_session.is_none()
                    || *controller.locked()
                {
                    return;
                }
                controller.as_mut().set_editor_busy(false);
                match result {
                Ok(uri) => {
                    #[derive(serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct PreparedTotp<'a> {
                        totp_input: &'a str,
                    }
                    let prepared = PreparedTotp { totp_input: uri.as_str() };
                    let Ok(json) = super::super::secrets_projection::sensitive_json(&prepared) else {
                        controller.as_mut().fail("No se pudo preparar la configuración TOTP.");
                        return;
                    };

                    controller.as_mut().totp_qr_prepared(&QString::from(json.as_str()));
                }
                Err(_) => controller.as_mut().fail("No se pudo leer el autenticador. Elige una imagen PNG o JPEG de hasta 8 MB con un único QR TOTP válido."),
            }
            },
        );
    }

    pub fn clear_editor(mut self: Pin<&mut Self>) {
        if *self.saving() {
            return;
        }
        self.as_mut().begin_editor_operation();
        self.as_mut().clear_editor_projection();
        self.as_mut().set_editor_busy(false);
    }
}

pub(super) fn load_editor_entry(id: SecretId) -> anyhow::Result<OpenedEntry> {
    let session = open_session()?;
    let item = session.vault().open_item(id)?;
    let related_entry = session.vault().list_links()?.into_iter().find_map(|link| {
        if link.first == id {
            Some(link.second)
        } else if link.second == id {
            Some(link.first)
        } else {
            None
        }
    });
    let Item { meta, content, .. } = item;
    Ok(OpenedEntry {
        id,
        folder: meta.state.folder,
        title: meta.state.title,
        revision: meta.revision,
        recipients: meta
            .state
            .recipients
            .as_slice()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
        icon: meta.state.icon,
        related_entry,
        content,
    })
}
