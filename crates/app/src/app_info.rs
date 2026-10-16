//! Application identity generated from the workspace Cargo manifest.

pub struct AppInfo {
    pub display_name: &'static str,
    pub version: &'static str,
    pub organization: &'static str,
    pub organization_domain: &'static str,
    pub desktop_file_id: &'static str,
    pub description: &'static str,
    pub license: &'static str,
    pub authors: &'static str,
    pub copyright: &'static str,
    pub homepage: &'static str,
    pub repository: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/app_identity.rs"));
