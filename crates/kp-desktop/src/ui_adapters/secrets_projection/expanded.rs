//! Detail rows built from an item's template. Concealed values stay in Rust
//! until explicitly revealed.

use anyhow::Result;
use kp_template::FieldType;
use kryptopass_app::Form;
use kryptopass_core::{FieldValue, ItemContent, SensitiveText};
use serde::Serialize;

use super::DetailView;
use super::content::template_of;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DetailRow {
    id: String,
    label: SensitiveText,
    value: SensitiveText,
    multiline: bool,
    concealed: bool,
    revealed: bool,
    has_value: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExpandedDetail<'a> {
    rows: Vec<DetailRow>,
    notes: SensitiveText,
    totp: Option<super::TotpProjection>,
    has_qr: bool,
    qr_source: Option<&'a str>,
    qr_unavailable_reason: Option<String>,
}

fn row(
    id: String,
    label: SensitiveText,
    value: SensitiveText,
    multiline: bool,
    concealed: bool,
) -> DetailRow {
    DetailRow {
        id,
        label,
        has_value: !value.is_empty(),
        value,
        multiline,
        concealed,
        revealed: false,
    }
}

fn rows(content: &ItemContent) -> Result<Vec<DetailRow>> {
    let (templates, template) = template_of(content.template().as_str())?;
    let language = crate::vault_access::language();
    let form = Form::build(templates.library(), template, &language);
    let mut result = Vec::new();
    for field in form.sections.iter().flat_map(|section| &section.fields) {
        let id = field.id.as_str();
        // Notes and codes have their own place; the private half of an SSH key
        // is only ever replaced, never shown or copied from here.
        if id == "notes"
            || id == "passphrase"
            || matches!(field.field_type, FieldType::Totp | FieldType::SshPrivateKey)
        {
            continue;
        }
        let multiline = matches!(
            field.field_type,
            FieldType::Multiline | FieldType::SecretMultiline | FieldType::SshPublicKey
        );
        let value = content.value(id);
        if field.multiple.is_some() {
            for (index, text) in value.map_or(&[][..], FieldValue::texts).iter().enumerate() {
                result.push(row(
                    format!("{id}:{index}"),
                    field.label.clone().into(),
                    text.clone(),
                    multiline,
                    field.concealed,
                ));
            }
            continue;
        }
        let text = value
            .and_then(FieldValue::first)
            .cloned()
            .unwrap_or_default();
        let shown: SensitiveText = match field.field_type {
            FieldType::Boolean => {
                crate::i18n::text(if text.as_str() == "true" { "Sí" } else { "No" }).into()
            }
            FieldType::Choice => field
                .choices
                .iter()
                .find(|choice| choice.id == text.as_str())
                .map_or(text, |choice| choice.label.clone().into()),
            _ => text,
        };
        result.push(row(
            id.to_owned(),
            field.label.clone().into(),
            shown,
            multiline,
            field.concealed,
        ));
    }
    if content.template().as_str() == "ssh-key" {
        if let Some(info) = kryptopass_core::inspect_ssh_key(
            content
                .value("private-key")
                .and_then(FieldValue::first)
                .map_or("", |key| key.as_str()),
        ) {
            result.push(row(
                "algorithm".to_owned(),
                crate::i18n::text("Algoritmo").into(),
                info.algorithm.into(),
                false,
                false,
            ));
        }
    }
    result.extend(content.custom().iter().map(|custom| {
        row(
            format!("custom:{}", custom.id()),
            custom.key.clone(),
            custom.value.clone(),
            custom.multiline,
            custom.concealed,
        )
    }));
    Ok(result)
}

pub(super) fn field(content: &ItemContent, id: &str) -> Option<(SensitiveText, bool)> {
    rows(content)
        .ok()?
        .into_iter()
        .find(|row| row.id == id)
        .map(|row| (row.value, row.concealed))
}

pub(super) fn detail<'a>(
    content: &'a ItemContent,
    view: &DetailView<'a>,
) -> Result<ExpandedDetail<'a>> {
    let kind = content.template().as_str();
    let (_templates, template) = template_of(kind)?;
    let mut rows = rows(content)?;
    for row in &mut rows {
        row.revealed = row.concealed && view.revealed_fields.contains(&row.id);
        if row.concealed && !row.revealed {
            row.value = SensitiveText::new();
        }
    }
    let wpa3 = kind == "wifi"
        && content
            .value("security")
            .and_then(FieldValue::first)
            .is_some_and(|value| value.as_str() == "wpa3");
    let totp = if kind == "totp" {
        let uri = content
            .value("uri")
            .and_then(FieldValue::first)
            .map_or("", |uri| uri.as_str());
        kryptopass_core::parse_totp_input(uri)
            .ok()
            .flatten()
            .map(|config| super::totp_projection(&config, view.unix_seconds))
            .transpose()?
    } else {
        None
    };
    Ok(ExpandedDetail {
        rows,
        notes: content
            .value("notes")
            .and_then(FieldValue::first)
            .cloned()
            .unwrap_or_default(),
        has_qr: kind == "totp" || (!template.actions.is_empty() && !wpa3),
        totp,
        qr_source: view.qr_source,
        qr_unavailable_reason: wpa3.then(|| {
            crate::i18n::text(
                "El QR de conexión está disponible para redes abiertas, WEP y WPA/WPA2.",
            )
        }),
    })
}
