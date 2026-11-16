//! Conservative domain names for organizing imported entries.

use url::{Host, Url};

/// Returns one domain folder only when every address has the same usable domain.
/// Invalid, unsupported, absent, or cross-domain addresses require no move.
/// This classifies folders only; it never grants autofill or merges accounts.
pub fn domain_folder<'a>(urls: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut domain: Option<String> = None;
    for raw in urls {
        let url = Url::parse(raw.trim()).ok()?;
        if !matches!(url.scheme(), "http" | "https") {
            return None;
        }
        let name = match url.host()? {
            Host::Domain(host) => psl::domain_str(host).unwrap_or(host).to_owned(),
            Host::Ipv4(host) => host.to_string(),
            Host::Ipv6(host) => format!("[{host}]"),
        };
        if name.len() > 256 || domain.as_ref().is_some_and(|other| other != &name) {
            return None;
        }
        domain = Some(name);
    }
    domain
}
