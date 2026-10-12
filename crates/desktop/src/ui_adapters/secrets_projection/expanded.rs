//! Field-level projections keep concealed values in Rust until explicitly revealed.

use kryptopass_core::SecretContent;
use serde::Serialize;
use std::{borrow::Cow, collections::BTreeSet};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DetailRow<'a> {
    id: String,
    label: String,
    value: Cow<'a, str>,
    multiline: bool,
    concealed: bool,
    revealed: bool,
    has_value: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExpandedDetail<'a> {
    rows: Vec<DetailRow<'a>>,
    notes: &'a str,
    totp: Option<super::TotpProjection>,
    has_qr: bool,
    qr_source: Option<&'a str>,
    qr_unavailable_reason: Option<String>,
}

fn row<'a>(
    id: impl Into<String>,
    label: &str,
    value: impl Into<Cow<'a, str>>,
    multiline: bool,
    concealed: bool,
) -> DetailRow<'a> {
    let value = value.into();
    DetailRow {
        id: id.into(),
        label: crate::i18n::text(label),
        has_value: !value.is_empty(),
        value,
        multiline,
        concealed,
        revealed: false,
    }
}

fn rows(content: &SecretContent) -> Vec<DetailRow<'_>> {
    match content {
        SecretContent::Custom(value) => value
            .records
            .iter()
            .map(|record| DetailRow {
                id: format!("record:{}", record.id),
                label: record.key.clone(),
                value: Cow::Borrowed(&record.value),
                multiline: record.multiline,
                concealed: record.concealed,
                revealed: false,
                has_value: !record.value.is_empty(),
            })
            .collect(),
        SecretContent::Card(value) => vec![
            row("holder", "Titular", value.holder.as_str(), false, false),
            row(
                "number",
                "Número de tarjeta",
                value.number.as_str(),
                false,
                true,
            ),
            row(
                "expiry",
                "Vencimiento",
                format!("{:02}/{}", value.expiry_month, value.expiry_year),
                false,
                false,
            ),
            row(
                "cvv",
                "Código de seguridad",
                value.cvv.as_str(),
                false,
                true,
            ),
            row("pin", "PIN", value.pin.as_str(), false, true),
        ],
        SecretContent::Identity(value) => {
            let mut result = vec![
                row(
                    "firstName",
                    "Nombre",
                    value.first_name.as_str(),
                    false,
                    false,
                ),
                row(
                    "lastName",
                    "Apellidos",
                    value.last_name.as_str(),
                    false,
                    false,
                ),
                row("address", "Dirección", value.address.as_str(), true, true),
            ];
            for (index, email) in value.emails.iter().enumerate() {
                result.push(row(
                    format!("email:{index}"),
                    "Correo",
                    email.as_str(),
                    false,
                    true,
                ));
            }
            for (index, phone) in value.phones.iter().enumerate() {
                result.push(row(
                    format!("phone:{index}"),
                    "Teléfono",
                    phone.as_str(),
                    false,
                    true,
                ));
            }
            for doc in &value.documents {
                result.push(row(
                    format!("document:{}:type", doc.id),
                    "Tipo de documento",
                    doc.document_type.as_str(),
                    false,
                    false,
                ));
                result.push(row(
                    format!("document:{}:number", doc.id),
                    "Número de documento",
                    doc.number.as_str(),
                    false,
                    true,
                ));
                result.push(row(
                    format!("document:{}:country", doc.id),
                    "País",
                    doc.country.as_str(),
                    false,
                    false,
                ));
                result.push(row(
                    format!("document:{}:expires", doc.id),
                    "Vencimiento",
                    doc.expires.as_str(),
                    false,
                    false,
                ));
            }
            result
        }
        SecretContent::Wifi(value) => vec![
            row("ssid", "Nombre de red", value.ssid.as_str(), false, false),
            row(
                "security",
                "Seguridad",
                match value.security {
                    kryptopass_core::WifiSecurity::Open => "Abierta",
                    kryptopass_core::WifiSecurity::Wep => "WEP",
                    kryptopass_core::WifiSecurity::Wpa => "WPA/WPA2",
                    kryptopass_core::WifiSecurity::Wpa3 => "WPA3",
                },
                false,
                false,
            ),
            row(
                "password",
                "Contraseña",
                value.password.as_str(),
                false,
                true,
            ),
            row(
                "hidden",
                "Red oculta",
                crate::i18n::text(if value.hidden { "Sí" } else { "No" }),
                false,
                false,
            ),
        ],
        SecretContent::License(value) => vec![
            row("product", "Producto", value.product.as_str(), false, false),
            row(
                "licenseKey",
                "Clave de licencia",
                value.license_key.as_str(),
                true,
                true,
            ),
            row("holder", "Titular", value.holder.as_str(), false, false),
            row("email", "Correo", value.email.as_str(), false, true),
            row("version", "Versión", value.version.as_str(), false, false),
            row(
                "purchased",
                "Fecha de compra",
                value.purchased.as_str(),
                false,
                false,
            ),
            row(
                "expires",
                "Vencimiento",
                value.expires.as_str(),
                false,
                false,
            ),
        ],
        SecretContent::SshKey(value) => {
            let mut result = vec![
                row(
                    "publicKey",
                    "Clave pública",
                    value.public_key.as_str(),
                    true,
                    false,
                ),
                row(
                    "fingerprint",
                    "Huella",
                    value.fingerprint.as_str(),
                    false,
                    false,
                ),
                row(
                    "algorithm",
                    "Algoritmo",
                    value.algorithm.as_str(),
                    false,
                    false,
                ),
                row("user", "Usuario", value.user.as_str(), false, false),
            ];
            for (index, host) in value.hosts.iter().enumerate() {
                result.push(row(
                    format!("host:{index}"),
                    "Host",
                    host.as_str(),
                    false,
                    false,
                ));
            }
            result
        }
        SecretContent::Login(_) | SecretContent::Note(_) | SecretContent::Totp(_) => vec![],
    }
}

pub(super) fn field<'a>(content: &'a SecretContent, id: &str) -> Option<(Cow<'a, str>, bool)> {
    rows(content)
        .into_iter()
        .find(|row| row.id == id)
        .map(|row| (row.value, row.concealed))
}

pub(super) fn detail<'a>(
    content: &'a SecretContent,
    unix_seconds: u64,
    revealed: &BTreeSet<String>,
    qr_source: Option<&'a str>,
) -> anyhow::Result<ExpandedDetail<'a>> {
    let (notes, totp, has_qr) = match content {
        SecretContent::Custom(value) => (value.notes.as_str(), None, false),
        SecretContent::Card(value) => (value.notes.as_str(), None, false),
        SecretContent::Identity(value) => (value.notes.as_str(), None, false),
        SecretContent::Wifi(value) => (
            value.notes.as_str(),
            None,
            value.security != kryptopass_core::WifiSecurity::Wpa3,
        ),
        SecretContent::License(value) => (value.notes.as_str(), None, false),
        SecretContent::SshKey(value) => (value.notes.as_str(), None, false),
        SecretContent::Totp(value) => (
            value.notes.as_str(),
            Some(super::totp_projection(&value.config, unix_seconds)?),
            true,
        ),
        _ => anyhow::bail!("unsupported field projection"),
    };
    let mut rows = rows(content);
    for row in &mut rows {
        row.revealed = row.concealed && revealed.contains(&row.id);
        if row.concealed && !row.revealed {
            row.value = Cow::Borrowed("");
        }
    }
    let qr_unavailable_reason = matches!(content, SecretContent::Wifi(value) if value.security == kryptopass_core::WifiSecurity::Wpa3)
        .then(|| crate::i18n::text("El QR de conexión está disponible para redes abiertas, WEP y WPA/WPA2."));
    Ok(ExpandedDetail {
        rows,
        notes,
        totp,
        has_qr,
        qr_source,
        qr_unavailable_reason,
    })
}
