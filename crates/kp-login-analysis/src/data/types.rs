use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    #[serde(default)]
    pub user_id: String,
    pub vaults: BTreeMap<String, Vault>,
    pub version: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vault {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub display: Display,
    #[serde(default)]
    pub folders: Vec<Folder>,
    pub items: Vec<Item>,
}

#[derive(Clone, Default, Deserialize)]
pub struct Display {
    #[serde(default)]
    pub color: i64,
    #[serde(default)]
    pub icon: i64,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub folder_id: String,
    #[serde(default)]
    pub parent_folder_id: Option<String>,
    pub name: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub item_id: String,
    #[serde(default)]
    pub share_id: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub data: ItemData,
    #[serde(default)]
    pub state: u32,
    #[serde(default)]
    pub alias_email: Option<String>,
    #[serde(default)]
    pub content_format_version: u32,
    #[serde(default)]
    pub create_time: u64,
    #[serde(default)]
    pub modify_time: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub share_count: u32,
    #[serde(default)]
    pub files: Vec<String>,
}

impl Item {
    pub fn login(&self) -> Option<&LoginContent> {
        if !matches!(self.data.kind, ItemType::Login) {
            return None;
        }
        match &self.data.content {
            ItemContent::Login(login) => Some(login),
            ItemContent::Empty(_) | ItemContent::Other(_) => None,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemData {
    pub metadata: Metadata,
    #[serde(default)]
    pub extra_fields: Vec<ExtraField>,
    #[serde(rename = "type")]
    pub kind: ItemType,
    pub content: ItemContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_specific: Option<PlatformSpecific>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum ItemType {
    Login,
    Note,
    Alias,
    CreditCard,
    Identity,
    Custom,
    Wifi,
    SshKey,
    Other(String),
}

impl ItemType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Login => "login",
            Self::Note => "note",
            Self::Alias => "alias",
            Self::CreditCard => "creditCard",
            Self::Identity => "identity",
            Self::Custom => "custom",
            Self::Wifi => "wifi",
            Self::SshKey => "sshKey",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ItemType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ItemType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "login" => Self::Login,
            "note" => Self::Note,
            "alias" => Self::Alias,
            "creditCard" => Self::CreditCard,
            "identity" => Self::Identity,
            "custom" => Self::Custom,
            "wifi" => Self::Wifi,
            "sshKey" => Self::SshKey,
            _ => Self::Other(value),
        })
    }
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum ItemContent {
    Login(LoginContent),
    Empty(EmptyContent),
    Other(BTreeMap<String, Value>),
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyContent {}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginContent {
    #[serde(default)]
    pub item_email: String,
    pub password: Zeroizing<String>,
    #[serde(default)]
    pub totp_uri: String,
    #[serde(default)]
    pub passkeys: Vec<Passkey>,
    #[serde(default)]
    pub item_username: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub autofill_urls: Vec<AutofillUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passkey {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutofillUrl {
    pub url: String,
    #[serde(default)]
    pub mode: i64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraField {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub name: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub item_uuid: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSpecific {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub android: Option<AndroidSpecific>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidSpecific {
    #[serde(default)]
    pub allowed_apps: Vec<AndroidAllowedApp>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAllowedApp {
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub app_name: String,
    #[serde(default)]
    pub hashes: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
