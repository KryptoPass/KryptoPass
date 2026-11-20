//! Reviewing and resolving a Sync conflict field by field.

use super::*;

impl qobject::SecretsController {
    /// Opens the merge of a conflicted item. Decrypting may ask for a passphrase.
    pub fn open_conflict(mut self: Pin<&mut Self>, id: &QString) {
        if *self.locked() || *self.busy() || *self.saving() {
            return;
        }
        let Some(id) = parse_secret_id(&id.to_string()) else {
            self.as_mut()
                .fail(crate::i18n::tr_noop("The selected item is not valid."));
            return;
        };
        self.as_mut().close_conflict();
        self.as_mut().clear_error();
        self.as_mut().set_busy(true);
        self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
            "Opening both versions…",
        )));
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Option<Conflict>> {
                Ok(open_session()?.vault().open_conflict(id)?)
            },
            |mut controller, result| {
                controller.as_mut().set_busy(false);
                match result {
                    Ok(Some(conflict)) => {
                        controller.as_mut().rust_mut().conflict = Some(conflict);
                        controller.as_mut().publish_conflict();
                        controller
                            .as_mut()
                            .set_status(QString::from(crate::i18n::tr_noop(
                                "Review the differences",
                            )));
                    }
                    Ok(None) => controller
                        .as_mut()
                        .fail(crate::i18n::tr_noop("This item no longer has conflicts.")),
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "conflict did not open");
                        let message = match error.downcast_ref::<VaultError>() {
                            Some(VaultError::ConflictUnreadable) => crate::i18n::tr_noop(
                                "One of the versions uses a key that is not on this device. Resolve it from a device that has it.",
                            ),
                            _ => crate::i18n::tr_noop(
                                "The conflicting versions could not be opened.",
                            ),
                        };
                        controller.as_mut().fail(message);
                    }
                }
            },
        );
    }

    pub fn close_conflict(mut self: Pin<&mut Self>) {
        {
            let mut state = self.as_mut().rust_mut();
            state.conflict = None;
            state.conflict_revealed.clear();
        }
        self.set_conflict_json(QString::default());
    }

    /// Shows or hides the two values of a concealed difference.
    pub fn toggle_conflict_value(mut self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        {
            let mut state = self.as_mut().rust_mut();
            if !state.conflict_revealed.remove(&key) {
                state.conflict_revealed.insert(key);
            }
        }
        self.publish_conflict();
    }

    /// Saves the merge with one side per difference, as `{"key": "ours"|"theirs"}`.
    pub fn resolve_conflict(mut self: Pin<&mut Self>, choices_json: &QString) {
        if *self.busy() || self.as_ref().rust().conflict.is_none() {
            return;
        }
        let Ok(picked) = serde_json::from_str::<std::collections::HashMap<String, String>>(
            &choices_json.to_string(),
        ) else {
            self.as_mut().fail(crate::i18n::tr_noop(
                "The combination selection is not valid.",
            ));
            return;
        };
        let choices = {
            let object = self.as_ref();
            let Some(conflict) = object.rust().conflict.as_ref() else {
                return;
            };
            conflict
                .differences
                .iter()
                .filter_map(|difference| {
                    let side = match picked.get(&conflict_key(&difference.target))?.as_str() {
                        "ours" => Side::Ours,
                        "theirs" => Side::Theirs,
                        _ => return None,
                    };
                    Some((difference.target.clone(), side))
                })
                .collect::<std::collections::HashMap<_, _>>()
        };
        let Some(conflict) = self.as_mut().rust_mut().conflict.take() else {
            return;
        };
        let operation = self.as_ref().rust().editor_operation;
        self.as_mut().set_conflict_json(QString::default());
        self.as_mut().start_mutation(
            crate::i18n::tr_noop("Saving the combination…"),
            true,
            crate::i18n::tr_noop("The combination could not be saved. The conflict is still open."),
            move || {
                let meta = open_session()?
                    .vault_mut()
                    .resolve_conflict(&conflict, &choices)?;
                Ok(MutationResult {
                    snapshot: snapshot_after_mutation(),
                    signal: MutationSignal::EntrySaved(meta.id.0.to_string()),
                    editor_operation: Some(operation),
                })
            },
        );
    }

    pub(super) fn publish_conflict(mut self: Pin<&mut Self>) {
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .conflict
                .as_ref()
                .map(|conflict| conflict_projection(conflict, &state.conflict_revealed))
        };
        match projection {
            Some(Ok(projection)) => self
                .as_mut()
                .set_conflict_json(QString::from(projection.as_str())),
            Some(Err(error)) => {
                tracing::warn!(error = %format!("{error:#}"), "conflict projection failed");
                self.as_mut().rust_mut().conflict = None;
                self.as_mut().fail(crate::i18n::tr_noop(
                    "The conflicting versions could not be shown.",
                ));
            }
            None => {}
        }
    }

    pub(super) fn set_conflict_json(mut self: Pin<&mut Self>, value: QString) {
        if self.conflict_json() != &value {
            self.as_mut().rust_mut().conflict_json = value;
            self.conflict_json_changed();
        }
    }
}

/// The stable name of a difference, for the dialog to answer with.
pub(super) fn conflict_key(target: &DiffTarget) -> String {
    match target {
        DiffTarget::Title => "title".to_owned(),
        DiffTarget::Folder => "folder".to_owned(),
        DiffTarget::Favorite => "favorite".to_owned(),
        DiffTarget::Trashed => "trashed".to_owned(),
        DiffTarget::Icon => "icon".to_owned(),
        DiffTarget::Field(field) => format!("field:{}", field.as_str()),
        DiffTarget::Custom(id) => format!("custom:{id}"),
    }
}

/// What the merge dialog shows. Concealed values stay out until revealed, and
/// values come from the versions the conflict already holds.
pub(super) fn conflict_projection(
    conflict: &Conflict,
    revealed: &std::collections::BTreeSet<String>,
) -> anyhow::Result<SensitiveText> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Row {
        key: String,
        label: SensitiveText,
        needs_review: bool,
        concealed: bool,
        revealed: bool,
        suggested: &'static str,
        ours: SensitiveText,
        theirs: SensitiveText,
    }
    #[derive(serde::Serialize)]
    struct Projection {
        id: String,
        title: String,
        rows: Vec<Row>,
    }
    let (templates, template) = template_of(conflict.ours.meta.state.template.as_str())?;
    let form = kryptopass_app::Form::build(
        templates.library(),
        template,
        &crate::vault_access::language(),
    );
    let yes_no = |value: bool| {
        crate::i18n::tr(if value {
            crate::i18n::tr_noop("Yes")
        } else {
            "No"
        })
    };
    let value_of = |item: &Item, target: &DiffTarget| -> SensitiveText {
        let state = &item.meta.state;
        match target {
            DiffTarget::Title => state.title.clone().into(),
            DiffTarget::Folder => state
                .folder
                .map_or_else(String::new, |folder| folder.0.to_string())
                .into(),
            DiffTarget::Favorite => yes_no(state.favorite).into(),
            DiffTarget::Trashed => yes_no(state.trashed).into(),
            DiffTarget::Icon => crate::i18n::tr("Chosen icon").into(),
            DiffTarget::Field(field) => {
                let mut text = SensitiveText::new();
                let values = item
                    .content
                    .value(field.as_str())
                    .map_or(&[][..], FieldValue::texts);
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        text.push('\n');
                    }
                    text.push_str(value);
                }
                text
            }
            DiffTarget::Custom(id) => item
                .content
                .custom()
                .iter()
                .find(|custom| custom.id() == *id)
                .map(|custom| custom.value.clone())
                .unwrap_or_default(),
        }
    };
    let label_of = |target: &DiffTarget| -> SensitiveText {
        match target {
            DiffTarget::Title => crate::i18n::tr("Name").into(),
            DiffTarget::Folder => crate::i18n::tr("Folder").into(),
            DiffTarget::Favorite => crate::i18n::tr("Favorite").into(),
            DiffTarget::Trashed => crate::i18n::tr("In the trash").into(),
            DiffTarget::Icon => crate::i18n::tr("Icon").into(),
            DiffTarget::Field(field) => form
                .sections
                .iter()
                .flat_map(|section| &section.fields)
                .find(|candidate| candidate.id == *field)
                .map_or_else(
                    || field.as_str().to_owned(),
                    |candidate| candidate.label.clone(),
                )
                .into(),
            DiffTarget::Custom(id) => [&conflict.ours, &conflict.theirs]
                .iter()
                .find_map(|item| {
                    item.content
                        .custom()
                        .iter()
                        .find(|custom| custom.id() == *id)
                        .map(|custom| custom.key.clone())
                })
                .unwrap_or_default(),
        }
    };
    let rows = conflict
        .differences
        .iter()
        .map(|difference| {
            let key = conflict_key(&difference.target);
            let show = !difference.concealed || revealed.contains(&key);
            let value = |item: &Item| {
                if show {
                    value_of(item, &difference.target)
                } else {
                    SensitiveText::new()
                }
            };
            Row {
                label: label_of(&difference.target),
                needs_review: difference.kind == DiffKind::Conflict,
                concealed: difference.concealed,
                revealed: difference.concealed && show,
                suggested: match difference.suggested {
                    Some(Side::Ours) => "ours",
                    Some(Side::Theirs) => "theirs",
                    _ => "",
                },
                ours: value(&conflict.ours),
                theirs: value(&conflict.theirs),
                key,
            }
        })
        .collect();
    super::super::secrets_projection::sensitive_json(&Projection {
        id: conflict.ours.meta.id.0.to_string(),
        title: conflict.ours.meta.state.title.clone(),
        rows,
    })
}
