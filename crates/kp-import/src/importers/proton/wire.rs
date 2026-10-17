//! Proton wire types are private to this provider.

use crate::WifiSecurity;
use kryptopass_secure_memory::{SensitiveBytes, SensitiveText};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Default, Zeroize, ZeroizeOnDrop)]
pub(super) struct SecretJson(pub(super) SensitiveText);
impl<'de> Deserialize<'de> for SecretJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Ok(Self(SensitiveText::from(String::from(Box::<str>::from(
            raw,
        )))))
    }
}

#[derive(Deserialize)]
pub(super) struct Export {
    pub(super) version: String,
    pub(super) vaults: BTreeMap<String, Vault>,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
pub(super) struct Vault {
    pub(super) name: String,
    #[serde(default)]
    pub(super) description: SensitiveText,
    #[serde(default)]
    pub(super) folders: Vec<SourceFolder>,
    pub(super) items: Vec<Item>,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceFolder {
    pub(super) folder_id: String,
    #[serde(default)]
    pub(super) parent_folder_id: Option<String>,
    pub(super) name: String,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct Item {
    pub(super) item_id: String,
    #[serde(default)]
    pub(super) share_id: String,
    #[serde(default)]
    pub(super) folder_id: Option<String>,
    pub(super) data: ItemData,
    pub(super) state: u32,
    #[serde(default)]
    pub(super) alias_email: Option<SensitiveText>,
    #[serde(default)]
    pub(super) pinned: bool,
    #[serde(default)]
    pub(super) files: Vec<SensitiveText>,
    #[serde(default)]
    pub(super) create_time: u64,
    #[serde(default)]
    pub(super) modify_time: u64,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ItemData {
    pub(super) metadata: Metadata,
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) content: SecretJson,
    #[serde(default)]
    pub(super) extra_fields: SecretJson,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
pub(super) struct Metadata {
    pub(super) name: String,
    #[serde(default)]
    pub(super) note: SensitiveText,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct Login {
    #[serde(default)]
    pub(super) item_username: SensitiveText,
    #[serde(default)]
    pub(super) item_email: SensitiveText,
    #[serde(default)]
    pub(super) username: SensitiveText,
    pub(super) password: SensitiveText,
    #[serde(default)]
    pub(super) urls: Vec<SensitiveText>,
    #[serde(default)]
    pub(super) totp_uri: SensitiveText,
    #[serde(default)]
    pub(super) passkeys: SecretJson,
    #[serde(default)]
    pub(super) autofill_urls: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonCard {
    #[serde(default)]
    pub(super) cardholder_name: SensitiveText,
    #[serde(default)]
    pub(super) number: SensitiveText,
    #[serde(default)]
    pub(super) verification_number: SensitiveText,
    #[serde(default)]
    pub(super) expiration_date: SensitiveText,
    #[serde(default)]
    pub(super) pin: SensitiveText,
    #[serde(default)]
    pub(super) card_type: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonIdentity {
    #[serde(default)]
    pub(super) first_name: SensitiveText,
    #[serde(default)]
    pub(super) last_name: SensitiveText,
    #[serde(default)]
    pub(super) email: SensitiveText,
    #[serde(default)]
    pub(super) phone_number: SensitiveText,
    #[serde(default)]
    pub(super) street_address: SensitiveText,
    #[serde(default)]
    pub(super) zip_or_postal_code: SensitiveText,
    #[serde(default)]
    pub(super) city: SensitiveText,
    #[serde(default)]
    pub(super) state_or_province: SensitiveText,
    #[serde(default)]
    pub(super) country_or_region: SensitiveText,
    #[serde(default)]
    pub(super) social_security_number: SensitiveText,
    #[serde(default)]
    pub(super) passport_number: SensitiveText,
    #[serde(default)]
    pub(super) license_number: SensitiveText,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonCustom {
    #[serde(default)]
    pub(super) sections: Vec<ProtonCustomSection>,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonCustomSection {
    #[serde(default)]
    pub(super) section_name: SensitiveText,
    #[serde(default)]
    pub(super) section_fields: Vec<ProtonCustomField>,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonCustomField {
    #[serde(default)]
    pub(super) field_name: SensitiveText,
    #[serde(default, rename = "type")]
    pub(super) kind: SensitiveText,
    #[serde(default)]
    pub(super) data: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Default, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonCustomFieldData {
    #[serde(default)]
    pub(super) content: SensitiveText,
    #[serde(default)]
    pub(super) totp_uri: SensitiveText,
    #[serde(default)]
    pub(super) timestamp: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonWifi {
    #[serde(default)]
    pub(super) ssid: SensitiveText,
    #[serde(default)]
    pub(super) password: SensitiveText,
    #[serde(default)]
    pub(super) security: SecretJson,
    #[serde(default)]
    pub(super) sections: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

impl ProtonWifi {
    pub(super) fn security(&self) -> Option<WifiSecurity> {
        let raw = self.security.0.trim();
        let value = serde_json::from_str::<String>(raw).ok();
        match value.as_deref().unwrap_or(raw) {
            "1" | "WPA" | "WPA2" => Some(WifiSecurity::Wpa),
            "2" => Some(WifiSecurity::Wpa),
            "3" | "WPA3" => Some(WifiSecurity::Wpa3),
            "4" | "WEP" => Some(WifiSecurity::Wep),
            _ => None,
        }
    }

    pub(super) fn has_unmapped_fields(&self) -> bool {
        self.sections.has_value() || !self.extra_fields.is_empty()
    }
}

#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProtonSshKey {
    #[serde(default)]
    pub(super) private_key: SensitiveText,
    #[serde(default)]
    pub(super) public_key: SensitiveText,
    #[serde(default)]
    pub(super) sections: SecretJson,
    #[serde(default, flatten)]
    pub(super) extra_fields: SecretFields,
}

impl ProtonSshKey {
    pub(super) fn has_unmapped_fields(&self) -> bool {
        self.sections.has_value() || !self.extra_fields.is_empty()
    }
}

#[derive(Default)]
pub(super) struct SecretFields(BTreeMap<SensitiveText, SecretJson>);

impl<'de> Deserialize<'de> for SecretFields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = BTreeMap::<SensitiveText, SecretValue>::deserialize(deserializer)?;
        let mut fields = BTreeMap::new();
        for (key, value) in values {
            let mut bytes = SensitiveBytes::new();
            serde_json::to_writer(&mut bytes, &value).map_err(serde::de::Error::custom)?;
            let raw = SensitiveText::from_utf8(bytes).map_err(serde::de::Error::custom)?;
            fields.insert(key, SecretJson(raw));
        }
        Ok(Self(fields))
    }
}

// Flattened serde fields cannot use RawValue's JSON-specific token. Keep the
// fallback tree's string leaves in owned sensitive memory instead of Value.
#[derive(serde::Serialize, Deserialize)]
#[serde(untagged)]
enum SecretValue {
    Null(()),
    Bool(bool),
    Number(serde_json::Number),
    Text(SensitiveText),
    Array(Vec<SecretValue>),
    Object(BTreeMap<SensitiveText, SecretValue>),
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        if let Self::Object(values) = self {
            while let Some((mut key, value)) = values.pop_first() {
                key.zeroize();
                drop(value);
            }
        }
    }
}

impl Zeroize for SecretFields {
    fn zeroize(&mut self) {
        while let Some((mut key, mut value)) = self.0.pop_first() {
            key.zeroize();
            value.zeroize();
        }
    }
}

impl Drop for SecretFields {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl std::ops::Deref for SecretFields {
    type Target = BTreeMap<SensitiveText, SecretJson>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SecretJson {
    pub(super) fn has_value(&self) -> bool {
        !matches!(self.0.trim(), "" | "[]" | "{}" | "null" | "\"\"")
    }
}
