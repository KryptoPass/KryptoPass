//! Wi-Fi rules shared by every representation of a network: which password
//! a security mode accepts, and the provisioning QR text phones read.

use crate::SensitiveText;

/// Authentication of a network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Security {
    Open,
    Wep,
    Wpa,
    Wpa3,
}

impl Security {
    /// The mode named by a `choice` option id of the Wi-Fi template.
    pub(crate) fn from_option(id: &str) -> Option<Self> {
        match id {
            "none" => Some(Self::Open),
            "wep" => Some(Self::Wep),
            "wpa" => Some(Self::Wpa),
            "wpa3" => Some(Self::Wpa3),
            _ => None,
        }
    }
}

/// Whether `password` is one the mode can use: none for an open network,
/// a 40 or 104-bit key for WEP, a passphrase or raw key for WPA.
pub(crate) fn password_fits(security: Security, password: &str) -> bool {
    match security {
        Security::Open => password.is_empty(),
        Security::Wep => {
            (matches!(password.len(), 5 | 13) && password.is_ascii())
                || (matches!(password.len(), 10 | 26) && is_hex(password))
        }
        Security::Wpa | Security::Wpa3 => {
            (8..=63).contains(&password.len()) || (password.len() == 64 && is_hex(password))
        }
    }
}

fn is_hex(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The ZXing-compatible `WIFI:` text for a QR encoder. `None` for WPA3,
/// whose SAE handshake the format cannot express.
pub(crate) fn qr_payload(
    ssid: &str,
    security: Security,
    password: &str,
    hidden: bool,
) -> Option<SensitiveText> {
    let kind = match security {
        Security::Open => "nopass",
        Security::Wep => "WEP",
        Security::Wpa => "WPA",
        Security::Wpa3 => return None,
    };
    let mut payload = SensitiveText::with_capacity(ssid.len() + password.len() + 32);
    payload.push_str("WIFI:T:");
    payload.push_str(kind);
    payload.push_str(";S:");
    push_escaped(&mut payload, ssid);
    if security != Security::Open {
        payload.push_str(";P:");
        push_escaped(&mut payload, password);
    }
    if hidden {
        payload.push_str(";H:true");
    }
    payload.push_str(";;");
    Some(payload)
}

fn push_escaped(output: &mut SensitiveText, value: &str) {
    for character in value.chars() {
        if matches!(character, '\\' | ';' | ',' | ':' | '"') {
            output.push('\\');
        }
        output.push(character);
    }
}
