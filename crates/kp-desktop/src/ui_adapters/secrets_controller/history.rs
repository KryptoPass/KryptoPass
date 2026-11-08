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
                        controller
                            .as_mut()
                            .fail_detail("No se pudo abrir esa versión del elemento.");
                        return;
                    }
                };
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct VersionProjection<'a> {
                    at: u64,
                    title: &'a str,
                    username: SensitiveText,
                    urls: &'a [SensitiveText],
                    notes: SensitiveText,
                    has_password: bool,
                    has_totp: bool,
                }
                let projection =
                    super::super::secrets_projection::sensitive_json(&VersionProjection {
                        at: version.1,
                        title: &item.meta.state.title,
                        username: field_text(&item.content, "username"),
                        urls: item.content.value("urls").map_or(&[], FieldValue::texts),
                        notes: field_text(&item.content, "notes"),
                        has_password: !field_text(&item.content, "password").is_empty(),
                        has_totp: !field_text(&item.content, "totp").is_empty(),
                    });
                if let Ok(projection) = projection {
                    controller.as_mut().rust_mut().history_version_index = Some(index);
                    controller.as_mut().rust_mut().history_item = Some(item);
                    controller.set_version_json(QString::from(projection.as_str()));
                }
            },
        );
    }

    pub fn close_history_version(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().history_version_index = None;
        self.as_mut().rust_mut().history_item = None;
        self.set_version_json(QString::default());
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
            Err(_) => self.fail("No se pudo generar el archivo de actividad. Comprueba el espacio disponible e inténtalo de nuevo."),
        }
    }
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
    let mut report = String::from(
        "Actividad en KryptoPass\nVersiones anteriores, de la más reciente a la más antigua.\nNo incluye contraseñas ni valores de credenciales.\n\n",
    );
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
        report.push_str(&format!("{date}  Datos editados\n"));
    }
    Ok(report)
}
