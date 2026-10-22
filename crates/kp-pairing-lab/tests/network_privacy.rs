use std::{
    fs,
    net::IpAddr,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use kryptopass_lan::{NetworkPrivacyPolicy, VpnMode};
use kryptopass_pairing_lab::{
    NetworkPrivacySettings, load_network_privacy_settings, save_network_privacy_settings,
};

#[test]
fn network_privacy_policy_and_explicit_lan_address_persist_across_profile_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let parent = IsolatedParent::new()?;
    let profile_directory = parent.path.join("profile");
    assert_eq!(
        load_network_privacy_settings(&profile_directory)?,
        NetworkPrivacySettings::default(),
    );

    let follow_system = NetworkPrivacySettings {
        policy: NetworkPrivacyPolicy::default(),
        lan_bind_ip: None,
    };
    save_network_privacy_settings(&profile_directory, follow_system)?;
    assert_eq!(
        load_network_privacy_settings(&profile_directory)?,
        follow_system,
    );

    let strict = NetworkPrivacySettings {
        policy: NetworkPrivacyPolicy {
            vpn_mode: VpnMode::RequireVpn,
            allow_lan_exception: true,
            relay_only: true,
        },
        lan_bind_ip: Some("192.168.44.8".parse::<IpAddr>()?),
    };
    save_network_privacy_settings(&profile_directory, strict)?;
    assert_eq!(load_network_privacy_settings(&profile_directory)?, strict,);
    Ok(())
}

struct IsolatedParent {
    path: PathBuf,
}

impl IsolatedParent {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kp-network-privacy-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }
}

impl Drop for IsolatedParent {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
