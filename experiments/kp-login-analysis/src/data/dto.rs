use email_address::EmailAddress;
use serde::{Deserialize, Serialize};
use url::Url;

use super::Item;

/// A login reduced to the fields needed for URL analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginEntry {
    pub id: String,
    pub vault: String,
    pub title: String,
    pub user: UserIdentifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub urls: Vec<Url>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid_urls: Vec<String>,
    pub has_password: bool,
    pub has_totp: bool,
    pub pinned: bool,
    pub trashed: bool,
    pub created_at_ms: u64,
    pub modified_at_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl LoginEntry {
    pub fn from_item(vault: &str, item: &Item) -> Option<Self> {
        let content = item.login()?;
        let (urls, invalid_urls) = split_urls(&content.urls);
        Some(Self {
            id: item.item_id.clone(),
            vault: vault.to_owned(),
            title: item.data.metadata.name.clone(),
            user: UserIdentifier::detect(&content.item_username),
            email: non_empty(&content.item_email),
            urls,
            invalid_urls,
            has_password: !content.password.is_empty(),
            has_totp: !content.totp_uri.trim().is_empty(),
            pinned: item.pinned,
            trashed: item.state == 2,
            created_at_ms: item.create_time.saturating_mul(1000),
            modified_at_ms: item.modify_time.saturating_mul(1000),
            note: item.data.metadata.note.clone(),
        })
    }
}

/// What the login's user field contains: an email, a plain username, or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum UserIdentifier {
    Email(EmailAddress),
    Username(String),
    Missing,
}

impl UserIdentifier {
    pub fn detect(value: &str) -> Self {
        let value = value.trim();
        match value.parse::<EmailAddress>() {
            Ok(email) => Self::Email(email),
            Err(_) if value.is_empty() => Self::Missing,
            Err(_) => Self::Username(value.to_owned()),
        }
    }

    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Email(value) => Some(value.as_ref()),
            Self::Username(value) => Some(value),
            Self::Missing => None,
        }
    }

    pub fn is_email(&self) -> bool {
        matches!(self, Self::Email(_))
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn split_urls(raw: &[String]) -> (Vec<Url>, Vec<String>) {
    let mut urls = Vec::with_capacity(raw.len());
    let mut invalid = Vec::new();
    for value in raw {
        match Url::parse(value) {
            Ok(url) if matches!(url.scheme(), "http" | "https") && url.host().is_some() => {
                urls.push(url);
            }
            _ => invalid.push(value.clone()),
        }
    }
    (urls, invalid)
}
