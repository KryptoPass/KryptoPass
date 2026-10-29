//! Wi-Fi network payloads and the standard provisioning QR text format.

use crate::SensitiveText;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ensure_field_size;
use crate::domain::wifi;

const MAX_NOTES_BYTES: usize = 256 * 1024;

/// Wi-Fi authentication used in QR provisioning records.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
#[serde(rename_all = "lowercase")]
pub enum WifiSecurity {
    Open,
    Wep,
    #[default]
    Wpa,
    Wpa3,
}

/// Wireless network credentials stored inside the encrypted payload.
#[derive(Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WifiContent {
    pub ssid: SensitiveText,
    pub security: WifiSecurity,
    pub password: SensitiveText,
    pub hidden: bool,
    pub notes: SensitiveText,
}

impl WifiContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.ssid.is_empty() && self.ssid.len() <= 32,
            "Wi-Fi SSID must contain 1 to 32 bytes"
        );
        ensure_field_size("Wi-Fi password", &self.password, 64)?;
        ensure_field_size("Wi-Fi notes", &self.notes, MAX_NOTES_BYTES)?;
        ensure!(
            wifi::password_fits(self.security.shared(), &self.password),
            "Wi-Fi password does not fit the security mode"
        );
        Ok(())
    }
}

/// Builds a ZXing-compatible Wi-Fi provisioning payload for a QR encoder.
pub fn wifi_qr_payload(content: &WifiContent) -> Result<SensitiveText> {
    content.validate()?;
    wifi::qr_payload(
        &content.ssid,
        content.security.shared(),
        &content.password,
        content.hidden,
    )
    .context("WPA3 uses SAE and cannot be represented by the compatible Wi-Fi QR format")
}

impl WifiSecurity {
    fn shared(&self) -> wifi::Security {
        match self {
            Self::Open => wifi::Security::Open,
            Self::Wep => wifi::Security::Wep,
            Self::Wpa => wifi::Security::Wpa,
            Self::Wpa3 => wifi::Security::Wpa3,
        }
    }
}
