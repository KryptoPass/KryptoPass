//! Folders, moving items, favorites, trash and removal.

use super::*;

impl qobject::SecretsController {
    pub fn save_folder(
        mut self: Pin<&mut Self>,
        id: &QString,
        name: &QString,
        parent_id: &QString,
        icon: &QString,
        color: &QString,
    ) {
        if *self.busy() {
            return;
        }
        let id = id.to_string();
        let name = name.to_string();
        let parent_id = parent_id.to_string();
        let icon = icon.to_string();
        let color = color.to_string();
        let appearance = match FolderAppearance::new(icon, color) {
            Ok(appearance) => appearance,
            Err(_) => {
                self.as_mut()
                    .fail("El icono o el color de la carpeta no son válidos.");
                return;
            }
        };
        let folder_id = if id.is_empty() {
            None
        } else {
            match parse_folder_id(&id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta seleccionada no es válida.");
                    return;
                }
            }
        };
        let parent_id = if folder_id.is_some() || parent_id.is_empty() {
            None
        } else {
            match parse_folder_id(&parent_id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta de destino no es válida.");
                    return;
                }
            }
        };

        self.as_mut().start_mutation(
            "Guardando carpeta…",
            false,
            "No se pudo guardar la carpeta.",
            move || {
                let mut session = open_session()?;
                let vault = session.vault_mut();
                let saved_id = match folder_id {
                    Some(id) => {
                        vault
                            .update_folder(FolderEdit::new(id).name(name).appearance(appearance))?;
                        id
                    }
                    None => {
                        vault
                            .create_folder(NewFolder {
                                name,
                                parent: parent_id,
                                appearance,
                                default_recipients: None,
                            })?
                            .id
                    }
                };
                drop(session);
                let snapshot = snapshot_after_mutation();
                Ok(MutationResult {
                    snapshot,
                    signal: MutationSignal::FolderSaved(saved_id.0.to_string()),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn step_folder(mut self: Pin<&mut Self>, id: &QString, direction: i32) {
        if *self.busy() || *self.locked() || ![-1, 1].contains(&direction) {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };
        self.as_mut().start_mutation(
            "Ordenando carpeta…",
            false,
            "No se pudo cambiar el orden de la carpeta.",
            move || {
                step_folder(&mut open_session()?, id, direction)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
                    editor_operation: None,
                })
            },
        );
    }

    pub fn move_folder(
        mut self: Pin<&mut Self>,
        id: &QString,
        parent_id: &QString,
        before_id: &QString,
    ) {
        if *self.busy() || *self.locked() {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };
        let optional_id = |value: &QString| -> anyhow::Result<Option<FolderId>> {
            let value = value.to_string();
            if value.is_empty() {
                Ok(None)
            } else {
                parse_folder_id(&value)
                    .map(Some)
                    .ok_or_else(|| anyhow::anyhow!("invalid folder"))
            }
        };
        let (Ok(parent), Ok(before)) = (optional_id(parent_id), optional_id(before_id)) else {
            self.as_mut().fail("La carpeta de destino no es válida.");
            return;
        };
        self.as_mut().start_mutation(
            "Moviendo carpeta…", false,
            "No se pudo mover la carpeta. No puede colocarse dentro de sí misma ni de sus descendientes.",
            move || {
                open_session()?.vault_mut().move_folder(id, parent, before)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::None,
                    editor_operation: None,
                })
            },
        );
    }

    pub fn move_entry(mut self: Pin<&mut Self>, id: &QString, folder_id: &QString) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };
        let folder_id = folder_id.to_string();
        let folder_id = if folder_id.is_empty() {
            None
        } else {
            match parse_folder_id(&folder_id) {
                Some(id) => Some(id),
                None => {
                    self.as_mut().fail("La carpeta de destino no es válida.");
                    return;
                }
            }
        };

        self.as_mut().start_mutation(
            "Moviendo elemento…",
            false,
            "No se pudo mover el elemento.",
            move || {
                let mut session = open_session()?;
                let before_revision = item_meta(&session, id)?.revision;
                session
                    .vault_mut()
                    .update_item(ItemEdit::new(id, before_revision).folder(folder_id))?;
                drop(session);
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::MetadataChanged(id, before_revision),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn set_favorite(mut self: Pin<&mut Self>, id: &QString, favorite: bool) {
        self.as_mut().update_flags(id, Some(favorite), None);
    }

    pub fn set_trashed(mut self: Pin<&mut Self>, id: &QString, trashed: bool) {
        self.as_mut().update_flags(id, None, Some(trashed));
    }

    pub fn remove_entry(mut self: Pin<&mut Self>, id: &QString) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut().fail("El elemento seleccionado no es válido.");
            return;
        };

        // Invalidate an outstanding read before deleting, rather than waiting
        // for the mutation callback to remove its plaintext projection.
        self.as_mut().invalidate_selection();
        self.as_mut().start_mutation(
            "Eliminando elemento…",
            false,
            "No se pudo eliminar el elemento.",
            move || {
                let mut session = open_session()?;
                let revision = item_meta(&session, id)?.revision;
                session.vault_mut().remove_item(id, revision)?;
                drop(session);
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::EntryRemoved(id),
                    editor_operation: None,
                })
            },
        );
    }

    pub fn remove_folder(mut self: Pin<&mut Self>, id: &QString, delete_contents: bool) {
        if *self.busy() {
            return;
        }
        let Some(id) = parse_folder_id(&id.to_string()) else {
            self.as_mut().fail("La carpeta seleccionada no es válida.");
            return;
        };

        let selected_id = self.as_ref().rust().selected_entry;
        if delete_contents {
            // Cancel outstanding reads before their source entries can disappear.
            self.as_mut().invalidate_selection();
        }
        self.as_mut().start_mutation(
            "Eliminando carpeta…",
            false,
            "No se pudo eliminar la carpeta.",
            move || {
                let mut session = open_session()?;
                let affected_selection =
                    selected_id
                        .filter(|_| !delete_contents)
                        .and_then(|selected| {
                            item_meta(&session, selected)
                                .ok()
                                .filter(|entry| entry.state.folder == Some(id))
                                .map(|entry| (selected, entry.revision))
                        });
                session.vault_mut().remove_folder(
                    id,
                    if delete_contents {
                        FolderContents::Delete
                    } else {
                        FolderContents::Reparent
                    },
                )?;
                drop(session);
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: affected_selection
                        .map_or(MutationSignal::None, |(selected, revision)| {
                            MutationSignal::MetadataChanged(selected, revision)
                        }),
                    editor_operation: None,
                })
            },
        );
    }
}

pub(super) fn step_folder(
    session: &mut VaultSession,
    id: FolderId,
    direction: i32,
) -> anyhow::Result<()> {
    anyhow::ensure!([-1, 1].contains(&direction), "invalid folder direction");
    let folders = session.vault().list_folders()?;
    let folder = folders
        .iter()
        .find(|folder| folder.id == id)
        .ok_or_else(|| anyhow::anyhow!("folder not found"))?;
    let mut siblings: Vec<_> = folders
        .iter()
        .filter(|candidate| candidate.parent_id == folder.parent_id)
        .collect();
    siblings.sort_by_key(|candidate| {
        (
            candidate.sort_position,
            candidate.name.as_str(),
            candidate.id.0,
        )
    });
    let index = siblings
        .iter()
        .position(|candidate| candidate.id == id)
        .ok_or_else(|| anyhow::anyhow!("folder not found"))?;
    let before = if direction < 0 {
        index.checked_sub(1).map(|previous| siblings[previous].id)
    } else {
        siblings.get(index + 2).map(|next| next.id)
    };
    if (direction < 0 && index > 0) || (direction > 0 && index + 1 < siblings.len()) {
        session
            .vault_mut()
            .move_folder(id, folder.parent_id, before)?;
    }
    Ok(())
}
