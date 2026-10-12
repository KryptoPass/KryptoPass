use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::content::SecretContent;

pub const MAX_ENTRY_ICON_BYTES: usize = 512 * 1024;
pub const MAX_ENTRY_ICON_DIMENSION: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum EntryIcon {
    Favicon {
        host: String,
    },
    Theme {
        name: String,
    },
    Image {
        #[serde(rename = "assetId")]
        asset_id: Uuid,
    },
}

impl EntryIcon {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Favicon { host } => ensure!(valid_favicon_host(host), "invalid favicon host"),
            Self::Theme { name } => ensure!(
                !name.is_empty()
                    && name.len() <= 128
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
                "invalid entry theme icon"
            ),
            Self::Image { asset_id } => ensure!(!asset_id.is_nil(), "invalid entry icon asset ID"),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryIconAsset {
    pub id: Uuid,
    pub png: Vec<u8>,
}

impl EntryIconAsset {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.id.is_nil(), "invalid entry icon asset ID");
        ensure!(
            self.png.len() >= 45 && self.png.len() <= MAX_ENTRY_ICON_BYTES,
            "invalid entry icon size"
        );
        ensure!(
            self.png.starts_with(b"\x89PNG\r\n\x1a\n")
                && self.png[8..12] == 13u32.to_be_bytes()
                && &self.png[12..16] == b"IHDR",
            "invalid entry icon PNG"
        );
        let width = u32::from_be_bytes(self.png[16..20].try_into()?);
        let height = u32::from_be_bytes(self.png[20..24].try_into()?);
        ensure!(
            width > 0
                && height > 0
                && width <= MAX_ENTRY_ICON_DIMENSION
                && height <= MAX_ENTRY_ICON_DIMENSION,
            "invalid entry icon dimensions"
        );
        ensure!(
            self.png[24] == 8 && matches!(self.png[25], 2 | 6) && self.png[26..29] == [0, 0, 0],
            "unsupported normalized PNG format"
        );
        let mut offset = 8usize;
        let mut has_data = false;
        while offset < self.png.len() {
            ensure!(self.png.len() - offset >= 12, "truncated icon PNG chunk");
            let length = u32::from_be_bytes(self.png[offset..offset + 4].try_into()?) as usize;
            let end = offset
                .checked_add(12)
                .and_then(|value| value.checked_add(length))
                .ok_or_else(|| anyhow::anyhow!("invalid icon PNG chunk"))?;
            ensure!(end <= self.png.len(), "truncated icon PNG chunk");
            let kind = &self.png[offset + 4..offset + 8];
            ensure!(kind != b"acTL", "animated icons are unsupported");
            has_data |= kind == b"IDAT" && length > 0;
            if kind == b"IEND" {
                ensure!(
                    length == 0 && end == self.png.len() && has_data,
                    "invalid icon PNG end"
                );
                return Ok(());
            }
            offset = end;
        }
        anyhow::bail!("icon PNG is missing its end")
    }
}

pub struct EntryAppearanceUpdate {
    pub icon: Option<EntryIcon>,
    pub asset: Option<EntryIconAsset>,
}

impl EntryAppearanceUpdate {
    pub fn validate(&self) -> Result<()> {
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        if let Some(asset) = &self.asset {
            asset.validate()?;
            ensure!(
                self.icon == Some(EntryIcon::Image { asset_id: asset.id }),
                "entry icon and resource do not match"
            );
        }
        Ok(())
    }
}

pub fn valid_favicon_host(host: &str) -> bool {
    if host.len() > 253 || !host.contains('.') || host != host.to_ascii_lowercase() {
        return false;
    }
    let labels: Vec<_> = host.split('.').collect();
    let last = labels.last().copied().unwrap_or_default();
    if matches!(
        last,
        "localhost" | "local" | "internal" | "test" | "invalid" | "example" | "onion"
    ) || host == "example.com"
        || host == "example.net"
        || host == "example.org"
        || last.len() < 2
        || !last.bytes().all(|b| b.is_ascii_lowercase())
    {
        return false;
    }
    labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

pub fn favicon_for_url(value: &str) -> Option<EntryIcon> {
    let url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.port().is_some() {
        return None;
    }
    let host = url.host_str()?.trim_end_matches('.').to_owned();
    valid_favicon_host(&host).then_some(EntryIcon::Favicon { host })
}

pub fn automatic_entry_icon(content: &SecretContent) -> Option<EntryIcon> {
    match content {
        SecretContent::Login(login) => {
            let url = login
                .urls
                .first()
                .map_or(login.url.as_str(), String::as_str);
            favicon_for_url(url).or_else(|| server_theme_for_url(url))
        }
        SecretContent::Note(_) => None,
    }
}

/// IP literals never yield favicons; routers and self-hosted services get the
/// server theme icon instead of the generic login one. Ports are allowed here:
/// unlike favicons, no network identity is derived from the URL.
fn server_theme_for_url(value: &str) -> Option<EntryIcon> {
    let url = url::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    match url.host()? {
        url::Host::Ipv4(_) | url::Host::Ipv6(_) => Some(EntryIcon::Theme {
            name: "network-server".to_owned(),
        }),
        _ => None,
    }
}
