//! Reads the application id from the workspace metadata, so the pinentry finds
//! the same socket the app opens without duplicating the name.

fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.toml");
    let manifest = std::fs::read_to_string("../../Cargo.toml").unwrap_or_default();
    let app_id = manifest
        .lines()
        .find_map(|line| {
            let value = line
                .trim()
                .strip_prefix("app-id")?
                .trim()
                .strip_prefix('=')?;
            Some(value.trim().trim_matches('"').to_owned())
        })
        .filter(|id| id.starts_with("org.kryptopass."));
    match app_id {
        Some(id) => println!("cargo:rustc-env=KP_APPROVAL_APP_ID={id}"),
        None => panic!("app-id is missing from [workspace.metadata.kryptopass]"),
    }
}
