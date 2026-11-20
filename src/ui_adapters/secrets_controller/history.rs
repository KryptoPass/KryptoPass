//! Earlier versions of an item and the activity report.

use super::*;

impl qobject::SecretsController {
    pub fn open_history_version(mut self: Pin<&mut Self>, index: i32) {
        self.as_mut().close_history_version();
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let (id, selection, version) = {
            let object = self.as_ref();
            let state = object.rust();
            let (Some(id), Some(version)) =
                (state.selected_entry, state.detail_versions.get(index))
            else {
                return;
            };
            (id, state.selection_operation, *version)
        };
        crate::qt::jobs::run_blocking(
            self.qt_thread(),
            move || -> anyhow::Result<Item> {
                Ok(open_session()?.vault().open_version(id, version.0)?)
            },
            move |mut controller, result| {
                if controller.as_ref().rust().selection_operation != selection
                    || *controller.locked()
                {
                    return;
                }
                let item = match result {
                    Ok(item) => item,
                    Err(error) => {
                        tracing::warn!(error = %format!("{error:#}"), "item version did not open");
                        controller.as_mut().fail_detail(crate::i18n::tr_noop(
                            "That version of the item could not be opened.",
                        ));
                        return;
                    }
                };
                controller.as_mut().rust_mut().history_password_revealed = false;
                controller.as_mut().rust_mut().history_version_index = Some(index);
                controller.as_mut().rust_mut().history_item = Some(item);
                controller.as_mut().publish_history_version();
            },
        );
    }

    pub fn close_history_version(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history_version_index = None;
        self.as_mut().rust_mut().history_item = None;
        self.as_mut().rust_mut().history_password_revealed = false;
        self.set_version_json(QString::default());
    }

    fn publish_history_version(mut self: Pin<&mut Self>) {
        let projection = {
            let object = self.as_ref();
            let state = object.rust();
            let (Some(item), Some(index)) =
                (state.history_item.as_ref(), state.history_version_index)
            else {
                return;
            };
            let Some((_, at)) = state.detail_versions.get(index) else {
                return;
            };
            version_projection(
                item,
                *at,
                index,
                state.history_password_revealed,
                state.detail_content.as_ref(),
            )
        };
        if let Ok(projection) = projection {
            self.as_mut()
                .set_version_json(QString::from(projection.as_str()));
        }
    }

    pub fn toggle_history_password(mut self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let has_password = {
            let object = self.as_ref();
            let state = object.rust();
            state
                .history_item
                .as_ref()
                .is_some_and(|item| !field_text(&item.content, "password").is_empty())
        };
        if !has_password {
            return;
        }
        let revealed = self.as_ref().rust().history_password_revealed;
        self.as_mut().rust_mut().history_password_revealed = !revealed;
        self.as_mut().publish_history_version();
    }

    pub fn copy_history_url(self: Pin<&mut Self>, index: i32) {
        if index < 0 || !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = self.as_ref().rust().history_item.as_ref().and_then(|item| {
            item.content
                .value("urls")
                .map(FieldValue::texts)
                .unwrap_or(&[])
                .get(index as usize)
                .filter(|value| !value.is_empty())
                .cloned()
        });
        if let Some(value) = value {
            self.clipboard_requested(&QString::from(value.as_str()));
        }
    }

    pub fn copy_history_value(self: Pin<&mut Self>, field: &QString) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let value = {
            let object = self.as_ref();
            let state = object.rust();
            let Some(item) = state.history_item.as_ref() else {
                return;
            };
            match field.to_string().as_str() {
                "title" => SensitiveText::from(item.meta.state.title.clone()),
                "urls" => {
                    let mut text = SensitiveText::new();
                    let urls = item
                        .content
                        .value("urls")
                        .map_or(&[][..], FieldValue::texts);
                    for (index, url) in urls.iter().enumerate() {
                        if index != 0 {
                            text.push('\n');
                        }
                        text.push_str(url);
                    }
                    text
                }
                name @ ("password" | "username" | "notes" | "totp") => {
                    field_text(&item.content, name)
                }
                _ => return,
            }
        };
        if value.is_empty() {
            return;
        }
        self.clipboard_requested(&QString::from(value.as_str()));
    }

    pub fn export_activity(self: Pin<&mut Self>) {
        if !self.as_ref().can_copy_login_value() {
            return;
        }
        let report = activity_report(&self.as_ref().rust().detail_versions);
        match report.and_then(|report| write_activity_report(&report)) {
            Ok(path) => self.activity_exported(&QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()))),
            Err(_) => self.fail(crate::i18n::tr_noop("The activity file could not be generated. Check the available space and try again.")),
        }
    }
}

fn version_projection(
    item: &Item,
    at: u64,
    index: usize,
    reveal_password: bool,
    current: Option<&ItemContent>,
) -> anyhow::Result<SensitiveText> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct VersionProjection<'a> {
        at: u64,
        index: usize,
        title: &'a str,
        username: SensitiveText,
        password: SensitiveText,
        password_revealed: bool,
        urls: &'a [SensitiveText],
        notes: SensitiveText,
        has_password: bool,
        has_totp: bool,
        changed_fields: Vec<&'static str>,
    }

    let content = &item.content;
    let has_password = !field_text(content, "password").is_empty();
    let password = if reveal_password && has_password {
        field_text(content, "password")
    } else {
        SensitiveText::new()
    };
    let changed_fields = ["username", "password", "urls", "notes", "totp"]
        .into_iter()
        .filter(|field| current.is_none_or(|current| current.value(field) != content.value(field)))
        .collect();
    super::super::secrets_projection::sensitive_json(&VersionProjection {
        at,
        index,
        title: &item.meta.state.title,
        username: field_text(content, "username"),
        password,
        password_revealed: reveal_password && has_password,
        urls: content.value("urls").map_or(&[], FieldValue::texts),
        notes: field_text(content, "notes"),
        has_password,
        has_totp: !field_text(content, "totp").is_empty(),
        changed_fields,
    })
}

pub(super) fn write_activity_report(report: &str) -> anyhow::Result<std::path::PathBuf> {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kryptopass-actividad-{}.txt", Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(&path)?;
    if let Err(error) = file
        .write_all(report.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(path)
}

pub(super) fn activity_report(versions: &[(VersionId, u64)]) -> anyhow::Result<String> {
    let zone = QTimeZone::system_time_zone();
    let zone = zone
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("local time zone unavailable"))?;
    let mut report = String::from(crate::i18n::tr_noop(
        "KryptoPass activity\nPrevious versions, from the most recent to the oldest.\nIt does not include passwords or credential values.\n\n",
    ));
    for (_, at) in versions {
        let date = i64::try_from(*at)
            .ok()
            .map(|timestamp| QDateTime::from_secs_since_epoch(timestamp, zone))
            .filter(QDateTime::is_valid)
            .map(|date| {
                date.format_enum(cxx_qt_lib::DateFormat::ISODate)
                    .to_string()
            })
            .unwrap_or_else(|| format!("Unix {at}"));
        report.push_str(&crate::i18n::trf("%1  Data edited\n", &[date]));
    }
    Ok(report)
}
