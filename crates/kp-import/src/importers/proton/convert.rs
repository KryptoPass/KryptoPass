//! Translation of Proton wire records into the public entry model.

use super::wire::{
    Item, Login, ProtonCard, ProtonCustom, ProtonCustomFieldData, ProtonIdentity, ProtonSshKey,
    ProtonWifi, SecretFields, SecretJson,
};
use crate::{
    AliasEntry, CardEntry, CustomEntry, EntryContent, EntryField, EntryLocation, EntryWarning,
    EntryWarningKind, IdentityDocument, IdentityEntry, ImportError, LoginEntry, NoteEntry,
    OneTimePassword, SshKeyEntry, WifiEntry,
};
use kryptopass_secure_memory::SensitiveText;
use std::fmt::Write;

pub(super) struct ConvertedContent {
    pub(super) content: EntryContent,
    pub(super) additional_fields: Vec<EntryField>,
}

pub(super) fn convert(
    item: &Item,
    location: EntryLocation,
    warnings: &mut Vec<EntryWarning>,
) -> Result<Option<ConvertedContent>, ImportError> {
    let notes = item.data.metadata.note.clone();
    let mut fields = Vec::new();
    let content = match item.data.kind.as_str() {
        "login" => {
            let login: Login = serde_json::from_str(&item.data.content.0)
                .map_err(|_| ImportError::InvalidExport)?;
            if append_extra_fields(&mut fields, "Proton Pass login", &login.extra_fields) {
                warn(warnings, location, EntryWarningKind::SourceFieldsPreserved);
            }
            preserve_reference(
                &mut fields,
                "Proton Pass passkeys",
                &login.passkeys,
                warnings,
                location,
                EntryWarningKind::PasskeysAsReferences,
            );
            preserve_reference(
                &mut fields,
                "Proton Pass autofill rules",
                &login.autofill_urls,
                warnings,
                location,
                EntryWarningKind::AutofillRulesAsReferences,
            );
            let username = if login.item_username.is_empty() {
                login.username.clone()
            } else {
                login.item_username.clone()
            };
            let otp = if login.totp_uri.trim().is_empty() {
                None
            } else if login
                .totp_uri
                .get(.."otpauth://".len())
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("otpauth://"))
            {
                Some(OneTimePassword::Uri(login.totp_uri.clone()))
            } else {
                Some(OneTimePassword::Base32(login.totp_uri.clone()))
            };
            EntryContent::Login(LoginEntry {
                username,
                email: login.item_email.clone(),
                password: login.password.clone(),
                urls: login.urls.clone(),
                otp,
                notes,
            })
        }
        "note" => {
            preserve_reference(
                &mut fields,
                "Proton Pass note data",
                &item.data.content,
                warnings,
                location,
                EntryWarningKind::SourceFieldsPreserved,
            );
            EntryContent::Note(NoteEntry { value: notes })
        }
        "alias" => {
            preserve_reference(
                &mut fields,
                "Proton Pass alias data",
                &item.data.content,
                warnings,
                location,
                EntryWarningKind::SourceFieldsPreserved,
            );
            EntryContent::Alias(AliasEntry {
                email: item.alias_email.clone().unwrap_or_default(),
                notes,
            })
        }
        "creditCard" => match serde_json::from_str::<ProtonCard>(&item.data.content.0) {
            Ok(card)
                if card.extra_fields.is_empty()
                    && !item.data.extra_fields.has_value()
                    && (!card.card_type.has_value() || is_scalar(&card.card_type)) =>
            {
                if let Some((expiry_month, expiry_year)) = parse_expiration(&card.expiration_date) {
                    preserve_reference(
                        &mut fields,
                        "Proton Pass card type",
                        &card.card_type,
                        warnings,
                        location,
                        EntryWarningKind::SourceFieldsPreserved,
                    );
                    EntryContent::Card(CardEntry {
                        holder: card.cardholder_name.clone(),
                        number: card.number.clone(),
                        expiry_month,
                        expiry_year,
                        cvv: card.verification_number.clone(),
                        pin: card.pin.clone(),
                        notes,
                    })
                } else {
                    source_reference(item, warnings, location)
                }
            }
            _ => source_reference(item, warnings, location),
        },
        "identity" => match serde_json::from_str::<ProtonIdentity>(&item.data.content.0) {
            Ok(identity)
                if identity.extra_fields.is_empty() && !item.data.extra_fields.has_value() =>
            {
                EntryContent::Identity(identity_entry(identity, notes))
            }
            _ => source_reference(item, warnings, location),
        },
        "custom" => match serde_json::from_str::<ProtonCustom>(&item.data.content.0) {
            Ok(custom) => EntryContent::Custom(custom_entry(custom, notes)),
            Err(_) => source_reference(item, warnings, location),
        },
        "wifi" => match serde_json::from_str::<ProtonWifi>(&item.data.content.0) {
            Ok(wifi) if !wifi.has_unmapped_fields() && !item.data.extra_fields.has_value() => {
                match wifi.security() {
                    Some(security) => EntryContent::Wifi(WifiEntry {
                        ssid: wifi.ssid.clone(),
                        security,
                        password: wifi.password.clone(),
                        hidden: false,
                        notes,
                    }),
                    None => source_reference(item, warnings, location),
                }
            }
            _ => source_reference(item, warnings, location),
        },
        "sshKey" => {
            let source = serde_json::from_str::<ProtonSshKey>(&item.data.content.0);
            let key = match source {
                Ok(source)
                    if !source.has_unmapped_fields() && !item.data.extra_fields.has_value() =>
                {
                    ssh_entry(source, notes, &mut fields, warnings, location)
                }
                _ => Err(ImportError::Content),
            };
            match key {
                Ok(key) => EntryContent::SshKey(key),
                Err(_) => {
                    warn(warnings, location, EntryWarningKind::UnreadableSshKey);
                    return Ok(None);
                }
            }
        }
        _ => {
            warn(warnings, location, EntryWarningKind::UnsupportedEntryType);
            return Ok(None);
        }
    };
    if append_source_fields(
        &mut fields,
        "Proton Pass item data",
        &item.data.extra_fields,
    ) {
        warn(warnings, location, EntryWarningKind::SourceFieldsPreserved);
    }
    Ok(Some(ConvertedContent {
        content,
        additional_fields: fields,
    }))
}

pub(super) fn warn(
    warnings: &mut Vec<EntryWarning>,
    location: EntryLocation,
    kind: EntryWarningKind,
) {
    let warning = EntryWarning { location, kind };
    if !warnings
        .iter()
        .rev()
        .take_while(|warning| warning.location == location)
        .any(|previous| previous.kind == kind)
    {
        warnings.push(warning);
    }
}

fn source_reference(
    item: &Item,
    warnings: &mut Vec<EntryWarning>,
    location: EntryLocation,
) -> EntryContent {
    warn(warnings, location, EntryWarningKind::SourceFieldsPreserved);
    EntryContent::Custom(CustomEntry {
        fields: vec![field(
            sensitive_format(format_args!("{} source", item.data.kind)),
            item.data.content.0.clone(),
            true,
        )],
        notes: item.data.metadata.note.clone(),
    })
}

fn preserve_reference(
    fields: &mut Vec<EntryField>,
    label: &str,
    value: &SecretJson,
    warnings: &mut Vec<EntryWarning>,
    location: EntryLocation,
    kind: EntryWarningKind,
) {
    if value.has_value() {
        fields.push(field(label.into(), value.0.clone(), true));
        warn(warnings, location, kind);
    }
}

fn field(label: SensitiveText, value: SensitiveText, concealed: bool) -> EntryField {
    EntryField {
        label,
        multiline: value.contains(['\n', '\r']),
        value,
        concealed,
    }
}

fn append_extra_fields(fields: &mut Vec<EntryField>, prefix: &str, source: &SecretFields) -> bool {
    let count = fields.len();
    for (name, value) in source.iter() {
        fields.push(field(
            sensitive_format(format_args!("{prefix} / {}", name.as_str())),
            value.0.clone(),
            true,
        ));
    }
    fields.len() > count
}

fn append_source_fields(fields: &mut Vec<EntryField>, prefix: &str, source: &SecretJson) -> bool {
    if !source.has_value() {
        return false;
    }
    if let Ok(values) = serde_json::from_str::<SecretFields>(&source.0) {
        if append_extra_fields(fields, prefix, &values) {
            return true;
        }
    }
    fields.push(field(
        sensitive_format(format_args!("{prefix} source")),
        source.0.clone(),
        true,
    ));
    true
}

fn identity_entry(mut source: ProtonIdentity, notes: SensitiveText) -> IdentityEntry {
    let mut address = SensitiveText::new();
    for (label, value) in [
        ("streetAddress", source.street_address.as_str()),
        ("city", source.city.as_str()),
        ("stateOrProvince", source.state_or_province.as_str()),
        ("zipOrPostalCode", source.zip_or_postal_code.as_str()),
        ("countryOrRegion", source.country_or_region.as_str()),
    ] {
        if !value.is_empty() {
            if !address.is_empty() {
                address.push('\n');
            }
            address.push_str(label);
            address.push_str(": ");
            address.push_str(value);
        }
    }
    let mut documents = Vec::new();
    for (document_type, number) in [
        (
            "socialSecurityNumber",
            std::mem::take(&mut source.social_security_number),
        ),
        (
            "passportNumber",
            std::mem::take(&mut source.passport_number),
        ),
        ("licenseNumber", std::mem::take(&mut source.license_number)),
    ] {
        if !number.is_empty() {
            documents.push(IdentityDocument {
                document_type: document_type.into(),
                number,
                country: source.country_or_region.clone(),
                expires: SensitiveText::new(),
            });
        }
    }
    let email = std::mem::take(&mut source.email);
    let phone = std::mem::take(&mut source.phone_number);
    IdentityEntry {
        first_name: std::mem::take(&mut source.first_name),
        last_name: std::mem::take(&mut source.last_name),
        documents,
        emails: (!email.is_empty()).then_some(email).into_iter().collect(),
        phones: (!phone.is_empty()).then_some(phone).into_iter().collect(),
        address,
        notes,
    }
}

fn custom_entry(mut source: ProtonCustom, notes: SensitiveText) -> CustomEntry {
    let mut fields = Vec::new();
    append_extra_fields(&mut fields, "Custom", &source.extra_fields);
    for mut section in std::mem::take(&mut source.sections) {
        let name = std::mem::take(&mut section.section_name);
        let prefix = if name.is_empty() {
            "Section"
        } else {
            name.as_str()
        };
        append_extra_fields(&mut fields, prefix, &section.extra_fields);
        for mut source_field in std::mem::take(&mut section.section_fields) {
            let label = match (name.is_empty(), source_field.field_name.is_empty()) {
                (false, false) => sensitive_format(format_args!(
                    "{} / {}",
                    name.as_str(),
                    source_field.field_name.as_str()
                )),
                (false, true) => name.clone(),
                (true, false) => std::mem::take(&mut source_field.field_name),
                (true, true) => sensitive_format(format_args!("Field {}", fields.len() + 1)),
            };
            let mut data = serde_json::from_str::<ProtonCustomFieldData>(&source_field.data.0);
            let kind = source_field.kind.as_str();
            let known = matches!(kind, "text" | "hidden" | "totp" | "timestamp");
            let (value, concealed) = match &mut data {
                Ok(value) if kind == "text" => (std::mem::take(&mut value.content), false),
                Ok(value) if kind == "hidden" => (std::mem::take(&mut value.content), true),
                Ok(value) if kind == "totp" && !value.totp_uri.is_empty() => {
                    (std::mem::take(&mut value.totp_uri), true)
                }
                Ok(value) if kind == "timestamp" && value.timestamp.has_value() => {
                    (std::mem::take(&mut value.timestamp.0), true)
                }
                _ => (std::mem::take(&mut source_field.data.0), true),
            };
            fields.push(field(label.clone(), value, concealed));
            if known && let Ok(value) = data {
                append_extra_fields(
                    &mut fields,
                    &sensitive_format(format_args!("{} / data", label.as_str())),
                    &value.extra_fields,
                );
            }
            append_extra_fields(&mut fields, &label, &source_field.extra_fields);
        }
    }
    CustomEntry { fields, notes }
}

fn ssh_entry(
    mut source: ProtonSshKey,
    notes: SensitiveText,
    fields: &mut Vec<EntryField>,
    warnings: &mut Vec<EntryWarning>,
    location: EntryLocation,
) -> Result<SshKeyEntry, ImportError> {
    let private_key = std::mem::take(&mut source.private_key);
    let parsed = ssh_key::PrivateKey::from_openssh(private_key.as_bytes())
        .map_err(|_| ImportError::Content)?;
    if parsed.is_encrypted() {
        return Err(ImportError::Content);
    }
    let public_key: SensitiveText = parsed
        .public_key()
        .to_openssh()
        .map_err(|_| ImportError::Content)?
        .into();
    if !source.public_key.is_empty() && source.public_key != public_key {
        if source.public_key.contains("PRIVATE KEY") || source.public_key.contains("openssh-key-v1")
        {
            return Err(ImportError::Content);
        }
        fields.push(field(
            "Proton Pass source public key".into(),
            std::mem::take(&mut source.public_key),
            true,
        ));
        warn(warnings, location, EntryWarningKind::ConflictingPublicKey);
    }
    Ok(SshKeyEntry {
        private_key,
        public_key,
        fingerprint: sensitive_format(format_args!(
            "{}",
            parsed.fingerprint(ssh_key::HashAlg::Sha256)
        )),
        algorithm: sensitive_format(format_args!("{}", parsed.algorithm())),
        user: parsed.comment().into(),
        notes,
    })
}

fn parse_expiration(value: &str) -> Option<(u8, u16)> {
    let (year, month) = value.split_once('-')?;
    if year.len() != 4 || month.len() != 2 {
        return None;
    }
    let year = year.parse::<u16>().ok()?;
    let month = month.parse::<u8>().ok()?;
    ((1..=12).contains(&month) && (1900..=9999).contains(&year)).then_some((month, year))
}

fn is_scalar(value: &SecretJson) -> bool {
    let value = value.0.trim();
    !value.is_empty() && !matches!(value.as_bytes()[0], b'{' | b'[') && value != "null"
}

fn sensitive_format(arguments: std::fmt::Arguments<'_>) -> SensitiveText {
    let mut text = SensitiveText::new();
    if text.write_fmt(arguments).is_err() {
        std::alloc::handle_alloc_error(std::alloc::Layout::new::<u8>());
    }
    text
}
