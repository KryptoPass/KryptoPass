//! The selected item: reading it, showing its fields and its QR code.

use super::*;

impl qobject::SecretsController {
    pub fn select_entry(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        let operation = {
            let mut state = self.as_mut().rust_mut();
            state.selection_operation = state.selection_operation.wrapping_add(1);
            state
                .selection_cancel_epoch
                .store(state.selection_operation, Ordering::Release);
            state.selection_operation
        };
        self.as_mut().clear_detail();
        self.as_mut().clear_detail_error();

        if *self.locked() {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            return;
        }

        if id.is_empty() {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            return;
        }

        let Some(secret_id) = parse_secret_id(&id) else {
            self.as_mut().rust_mut().pending_selection = None;
            self.as_mut().set_detail_busy(false);
            self.as_mut()
                .fail_detail("No se pudo abrir el elemento seleccionado.");
            return;
        };
        self.as_mut().rust_mut().selected_entry = Some(secret_id);
        self.as_mut().set_detail_busy(true);

        if self.as_ref().rust().selection_worker_active {
            self.as_mut().rust_mut().pending_selection = Some((secret_id, operation));
            return;
        }

        self.as_mut().rust_mut().selection_worker_active = true;
        self.start_selection_read(secret_id, operation);
    }

    pub(super) fn start_selection_read(self: Pin<&mut Self>, secret_id: SecretId, operation: u64) {
        let cancel_epoch = self.as_ref().rust().selection_cancel_epoch.clone();
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<LoadedDetail>> {
                let detail = load_detail(secret_id)?;
                if cancel_epoch.load(Ordering::Acquire) == operation {
                    Ok(Some(detail))
                } else {
                    drop(detail);
                    Ok(None)
                }
            },
            move |mut controller, result| {
                let pending = {
                    let mut state = controller.as_mut().rust_mut();
                    state.selection_worker_active = false;
                    state.pending_selection.take()
                };

                if let Some((pending_id, pending_operation)) = pending {
                    controller.as_mut().rust_mut().selection_worker_active = true;
                    controller
                        .as_mut()
                        .start_selection_read(pending_id, pending_operation);
                    return;
                }

                if controller.as_ref().rust().selection_operation != operation
                    || controller.as_ref().rust().selected_entry != Some(secret_id)
                {
                    return;
                }

                controller.as_mut().set_detail_busy(false);
                match result {
                    Ok(Some(loaded)) => {
                        let detail = content_projection(
                            secret_id,
                            &loaded.content,
                            &DetailView {
                                reveal_password: false,
                                unix_seconds: unix_seconds(),
                                revealed_fields: &std::collections::BTreeSet::new(),
                                qr_source: None,
                                versions: &loaded.versions,
                            },
                        );
                        match detail {
                            Ok(detail) => {
                                controller.as_mut().clear_detail_error();
                                {
                                    let mut state = controller.as_mut().rust_mut();
                                    state.detail_content = Some(loaded.content);
                                    state.detail_versions = loaded.versions;
                                    state.selected_revision = Some(loaded.revision);
                                }
                                controller
                                    .as_mut()
                                    .set_detail_json(QString::from(detail.as_str()));
                                controller
                                    .as_mut()
                                    .set_status(QString::from("Elemento cargado"));
                            }
                            Err(error) => {
                                tracing::warn!(error = %format!("{error:#}"), "item detail projection failed");
                                controller
                                    .as_mut()
                                    .fail_detail("No se pudo mostrar el contenido del elemento.");
                            }
                        }
                    }
                    Ok(None) => (),
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "item did not open");
                        let message = if is_unreadable(&error) {
                            "Este elemento está cifrado para una clave que no está en este dispositivo."
                        } else {
                            "No se pudo abrir el contenido del elemento."
                        };
                        controller.as_mut().fail_detail(message);
                    }
                }
            },
        );
    }

    pub(super) fn can_read_detail(self: Pin<&Self>) -> bool {
        let state = self.rust();
        !*self.locked()
            && !*self.busy()
            && !*self.saving()
            && !*self.detail_busy()
            && state.editor_session.is_none()
            && state.selected_revision.is_some()
            && state.detail_content.is_some()
    }

    pub(super) fn publish_detail(mut self: Pin<&mut Self>) {
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .selected_entry
                .zip(state.detail_content.as_ref())
                .map(|(id, content)| {
                    content_projection(
                        id,
                        content,
                        &DetailView {
                            reveal_password: state.password_revealed,
                            unix_seconds: unix_seconds(),
                            revealed_fields: &state.revealed_fields,
                            qr_source: state.qr_source.as_ref().map(|value| value.as_str()),
                            versions: &state.detail_versions,
                        },
                    )
                })
        };
        if let Some(Ok(projection)) = projection {
            self.as_mut()
                .set_detail_json(QString::from(projection.as_str()));
        }
    }

    pub fn toggle_field(mut self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let id = field.to_string();
        let allowed = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| content_field(content, &id))
            .is_some_and(|(_, concealed)| concealed);
        if !allowed {
            return;
        }
        {
            let mut state = self.as_mut().rust_mut();
            if !state.revealed_fields.remove(&id) {
                state.revealed_fields.insert(id);
            }
        }
        self.publish_detail();
    }

    pub fn copy_field(mut self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        let value = self
            .as_ref()
            .rust()
            .detail_content
            .as_ref()
            .and_then(|content| content_field(content, &field.to_string()))
            .filter(|(value, _)| !value.is_empty())
            .map(|(value, _)| value);
        if let Some(value) = value {
            self.as_mut().request_copy(value);
        }
    }

    pub fn show_qr(mut self: Pin<&mut Self>) {
        if !self.as_ref().can_read_detail() {
            return;
        }
        if self.as_ref().rust().qr_source.is_some() {
            self.as_mut().rust_mut().qr_source = None;
            self.publish_detail();
            return;
        }
        let result = (|| -> anyhow::Result<SensitiveText> {
            let object = self.as_ref();
            let state = object.rust();
            let content = state
                .detail_content
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no item is open"))?;
            let payload = if kind_of(content.template().as_str()) == "totp" {
                super::super::secrets_projection::totp_uri(content)?
            } else {
                let (_templates, template) = template_of(content.template().as_str())?;
                qr_payload(template, content)?
                    .ok_or_else(|| anyhow::anyhow!("QR unavailable for this item"))?
            };
            super::super::secret_qr::source(&payload)
        })();
        match result {
            Ok(source) => {
                self.as_mut().rust_mut().qr_source = Some(source);
                self.publish_detail();
            }
            Err(_) => self.as_mut().fail(
                "No se pudo generar el QR. El contenido puede superar la capacidad del código.",
            ),
        }
    }
}

pub(super) struct LoadedDetail {
    content: ItemContent,
    revision: u64,
    versions: Vec<(VersionId, u64)>,
}

pub(super) fn load_detail(id: SecretId) -> anyhow::Result<LoadedDetail> {
    let session = open_session()?;
    let Item { meta, content, .. } = session.vault().open_item(id)?;
    let versions = earlier_versions(meta.head, session.vault().item_history(id)?);
    Ok(LoadedDetail {
        content,
        revision: meta.revision,
        versions,
    })
}

/// Whether opening failed only because no key on this device can read the item.
pub(super) fn is_unreadable(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<VaultError>(),
        Some(
            VaultError::Cipher(kryptopass_core::vault::CipherError::NotARecipient)
                | VaultError::ItemNotFound
        )
    )
}
