//! Wi-Fi network payloads and the standard provisioning QR text format.

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::ensure_field_size;

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
    pub ssid: String,
    pub security: WifiSecurity,
    pub password: String,
    pub hidden: bool,
    pub notes: String,
}

impl WifiContent {
    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            !self.ssid.is_empty() && self.ssid.len() <= 32,
            "Wi-Fi SSID must contain 1 to 32 bytes"
        );
        ensure_field_size("Wi-Fi password", &self.password, 64)?;
        ensure_field_size("Wi-Fi notes", &self.notes, MAX_NOTES_BYTES)?;
        match &self.security {
            WifiSecurity::Open => ensure!(
                self.password.is_empty(),
                "open Wi-Fi cannot have a password"
            ),
            WifiSecurity::Wep => {
                ensure!(valid_wep_password(&self.password), "invalid WEP password")
            }
            WifiSecurity::Wpa | WifiSecurity::Wpa3 => ensure!(
                valid_wpa_password(&self.password),
                "WPA password must contain 8 to 63 bytes or 64 hexadecimal digits"
            ),
        }
        Ok(())
    }
}

/// Builds a ZXing-compatible Wi-Fi provisioning payload for a QR encoder.
pub fn wifi_qr_payload(content: &WifiContent) -> Result<Zeroizing<String>> {
    content.validate()?;
    ensure!(
        content.security != WifiSecurity::Wpa3,
        "WPA3 uses SAE and cannot be represented by the compatible Wi-Fi QR format"
    );
    let security = match &content.security {
        WifiSecurity::Open => "nopass",
        WifiSecurity::Wep => "WEP",
        WifiSecurity::Wpa => "WPA",
        WifiSecurity::Wpa3 => unreachable!("WPA3 is rejected above"),
    };
    let mut payload = Zeroizing::new(String::with_capacity(
        content.ssid.len() + content.password.len() + 32,
    ));
    payload.push_str("WIFI:T:");
    payload.push_str(security);
    payload.push_str(";S:");
    push_wifi_escaped(&mut payload, &content.ssid);
    if content.security != WifiSecurity::Open {
        payload.push_str(";P:");
        push_wifi_escaped(&mut payload, &content.password);
    }
    if content.hidden {
        payload.push_str(";H:true");
    }
    payload.push_str(";;");
    Ok(payload)
}

fn valid_wep_password(value: &str) -> bool {
    (matches!(value.len(), 5 | 13) && value.is_ascii())
        || matches!(value.len(), 10 | 26) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_wpa_password(value: &str) -> bool {
    (8..=63).contains(&value.len())
        || (value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn push_wifi_escaped(output: &mut String, value: &str) {
    for character in value.chars() {
        match character {
            '\\' | ';' | ',' | ':' | '"' => {
                output.push('\\');
                output.push(character);
            }
            character => output.push(character),
        }
    }
}
