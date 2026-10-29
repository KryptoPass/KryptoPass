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
        validate_normalized_png(&self.png).map(|_| ())
    }

    /// SHA-256 of the normalized PNG; the content address used by Sync.
    pub fn sha256(&self) -> [u8; 32] {
        png_sha256(&self.png)
    }
}

/// Ancillary chunks a normalized icon may carry; any other chunk is rejected.
const ALLOWED_PNG_CHUNKS: [&[u8; 4]; 6] = [b"IHDR", b"IDAT", b"IEND", b"sRGB", b"gAMA", b"pHYs"];

/// SHA-256 of an icon's exact bytes.
pub fn png_sha256(png: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(png).into()
}

/// Strictly validates a normalized icon PNG and returns its width and height.
///
/// Accepts only 8-bit RGB or RGBA, non-interlaced, 1 to 256 px, a single
/// contiguous run of `IDAT`, correct CRCs and an allowlist of chunks, so no
/// text, ICC profile, EXIF or animation can travel inside an icon.
pub fn validate_normalized_png(png: &[u8]) -> Result<(u32, u32)> {
    ensure!(
        png.len() >= 45 && png.len() <= MAX_ENTRY_ICON_BYTES,
        "invalid entry icon size"
    );
    ensure!(
        png.starts_with(b"\x89PNG\r\n\x1a\n")
            && png[8..12] == 13u32.to_be_bytes()
            && &png[12..16] == b"IHDR",
        "invalid entry icon PNG"
    );
    let width = u32::from_be_bytes(png[16..20].try_into()?);
    let height = u32::from_be_bytes(png[20..24].try_into()?);
    ensure!(
        width > 0
            && height > 0
            && width <= MAX_ENTRY_ICON_DIMENSION
            && height <= MAX_ENTRY_ICON_DIMENSION,
        "invalid entry icon dimensions"
    );
    ensure!(
        png[24] == 8 && matches!(png[25], 2 | 6) && png[26..29] == [0, 0, 0],
        "unsupported normalized PNG format"
    );
    let mut offset = 8usize;
    let mut has_data = false;
    let mut data_closed = false;
    let mut seen_ancillary = Vec::<[u8; 4]>::new();
    while offset < png.len() {
        ensure!(png.len() - offset >= 12, "truncated icon PNG chunk");
        let length = u32::from_be_bytes(png[offset..offset + 4].try_into()?) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .ok_or_else(|| anyhow::anyhow!("invalid icon PNG chunk"))?;
        ensure!(end <= png.len(), "truncated icon PNG chunk");
        let kind: [u8; 4] = png[offset + 4..offset + 8].try_into()?;
        ensure!(
            ALLOWED_PNG_CHUNKS.iter().any(|allowed| **allowed == kind),
            "icon PNG carries a chunk outside the allowlist"
        );
        let stored_crc = u32::from_be_bytes(png[end - 4..end].try_into()?);
        ensure!(
            crc32fast::hash(&png[offset + 4..end - 4]) == stored_crc,
            "icon PNG chunk CRC mismatch"
        );
        if offset == 8 {
            ensure!(&kind == b"IHDR", "icon PNG must start with IHDR");
        } else {
            ensure!(&kind != b"IHDR", "icon PNG repeats IHDR");
        }
        if &kind == b"IDAT" {
            ensure!(!data_closed, "icon PNG image data is not contiguous");
            has_data |= length > 0;
        } else if has_data {
            data_closed = true;
        }
        if matches!(&kind, b"sRGB" | b"gAMA" | b"pHYs") {
            ensure!(!has_data, "icon PNG ancillary chunk after image data");
            ensure!(!seen_ancillary.contains(&kind), "icon PNG repeats a chunk");
            seen_ancillary.push(kind);
        }
        if &kind == b"IEND" {
            ensure!(
                length == 0 && end == png.len() && has_data,
                "invalid icon PNG end"
            );
            return Ok((width, height));
        }
        offset = end;
    }
    anyhow::bail!("icon PNG is missing its end")
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

/// Icon derived from a web address: the site's favicon, or a server theme icon
/// when the address has no usable favicon host.
pub fn automatic_icon_for_url(value: &str) -> Option<EntryIcon> {
    favicon_for_url(value).or_else(|| server_theme_for_url(value))
}

pub fn automatic_entry_icon(content: &SecretContent) -> Option<EntryIcon> {
    match content {
        SecretContent::Login(login) => {
            let url = login.urls.first().map_or("", crate::SensitiveText::as_str);
            automatic_icon_for_url(url)
        }
        SecretContent::Note(_)
        | SecretContent::Custom(_)
        | SecretContent::Card(_)
        | SecretContent::Totp(_)
        | SecretContent::Identity(_)
        | SecretContent::Wifi(_)
        | SecretContent::License(_)
        | SecretContent::SshKey(_) => None,
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
