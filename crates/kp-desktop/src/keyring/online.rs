use cxx_qt_lib::QUrl;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OnlineSettings {
    pub enabled: bool,
    pub services: Vec<String>,
    pub keyservers: Vec<String>,
}

impl Default for OnlineSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            services: vec![
                "https://keyserver.ubuntu.com/pks/lookup?op=vindex&fingerprint=on&search=0x$$FPR$$"
                    .into(),
            ],
            keyservers: vec![
                "hkps://keyserver.ubuntu.com".into(),
                "hkps://keys.openpgp.org".into(),
            ],
        }
    }
}

impl OnlineSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.services.len() > 32 || self.keyservers.len() > 32 {
            return Err("Puedes configurar hasta 32 servicios o servidores.".into());
        }
        for service in &self.services {
            service_url(service, "0123456789ABCDEF0123456789ABCDEF01234567")?;
        }
        for server in &self.keyservers {
            validate_keyserver(server)?;
        }
        Ok(())
    }
}

fn validate_url(value: &str, schemes: &[&str]) -> Result<(), String> {
    let url = QUrl::from(value);
    let scheme = String::from(url.scheme_or_default());
    if value.len() > 4096
        || value.chars().any(char::is_whitespace)
        || !url.is_valid()
        || !schemes.contains(&scheme.as_str())
        || url.host_or_default().is_empty()
        || !url.user_name_or_default().is_empty()
        || !url.password_or_default().is_empty()
    {
        return Err("Indica una URL válida con servidor y sin credenciales ni espacios.".into());
    }
    Ok(())
}

pub fn validate_keyserver(value: &str) -> Result<(), String> {
    validate_url(value, &["hkp", "hkps", "http", "https"])
}

pub fn service_url(template: &str, fingerprint: &str) -> Result<String, String> {
    let fingerprint: String = fingerprint.chars().filter(|c| !c.is_whitespace()).collect();
    if !matches!(fingerprint.len(), 40 | 64) || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Indica la huella OpenPGP completa.".into());
    }
    let upper = fingerprint.to_ascii_uppercase();
    let lower = fingerprint.to_ascii_lowercase();
    let mut url = template.to_owned();
    for (token, value) in [
        ("$$ID8$$", &upper[upper.len() - 8..]),
        ("$$ID16$$", &upper[upper.len() - 16..]),
        ("$$FPR$$", upper.as_str()),
        ("$$id8$$", &lower[lower.len() - 8..]),
        ("$$id16$$", &lower[lower.len() - 16..]),
        ("$$fpr$$", lower.as_str()),
    ] {
        url = url.replace(token, value);
    }
    if url.contains("$$") {
        return Err("La plantilla contiene un marcador desconocido.".into());
    }
    validate_url(&url, &["http", "https"])?;
    Ok(url)
}

#[cfg(test)]
#[path = "../../tests/unit/keyring/online.rs"]
mod tests;
