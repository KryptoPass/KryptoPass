//! Network addresses, Internet routes and the VPN privacy policy.

use super::*;

fn persist_network_settings(lan_bind_ip: Option<IpAddr>) -> Result<(), ()> {
    let directory = peers_directory()?;
    // Selecting a local address does not authorize changing the saved privacy
    // policy. An unreadable file must not be replaced with stale UI defaults.
    let policy = load_network_privacy_settings(&directory)
        .map_err(|_| ())?
        .policy;
    save_network_privacy_settings(
        &directory,
        NetworkPrivacySettings {
            policy,
            lan_bind_ip,
        },
    )
    .map_err(|_| ())
}

/// VPN state for QML: 0 not required, 1 unverifiable, 2 active, 3 inactive.
fn vpn_status_code(policy: NetworkPrivacyPolicy) -> i32 {
    match NetworkPolicyGuard::new(policy).vpn_status() {
        VpnStatus::NotRequired => 0,
        VpnStatus::Unverifiable => 1,
        VpnStatus::Active => 2,
        VpnStatus::Inactive => 3,
    }
}

/// Whether any LAN path is allowed by the privacy choices.
pub(super) const fn lan_permitted(
    require_vpn: bool,
    allow_lan_exception: bool,
    relay_only: bool,
) -> bool {
    !relay_only && (!require_vpn || allow_lan_exception)
}

impl qobject::PeersController {
    pub fn load_network_addresses(mut self: Pin<&mut Self>) {
        if *self.busy() {
            return;
        }
        let settings = match peers_directory()
            .map_err(|_| ())
            .and_then(|directory| load_network_privacy_settings(&directory).map_err(|_| ()))
        {
            Ok(settings) => settings,
            Err(()) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    crate::i18n::tr_noop("The network privacy policy could not be read. Recovery connections are blocked until the settings are repaired."),
                ));
                return;
            }
        };
        let loaded_policy_changed = settings.policy
            != (NetworkPrivacyPolicy {
                vpn_mode: if self.rust().require_vpn {
                    VpnMode::RequireVpn
                } else {
                    VpnMode::FollowSystem
                },
                allow_lan_exception: self.rust().allow_lan_exception,
                relay_only: self.rust().relay_only,
            });
        if loaded_policy_changed {
            if let Some(guard) = self.as_mut().rust_mut().active_network_guard.as_ref() {
                guard.invalidate();
            }
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                owner.refresh_network_policy();
            }
        }
        self.as_mut()
            .set_require_vpn(settings.policy.vpn_mode == VpnMode::RequireVpn);
        self.as_mut()
            .set_allow_lan_exception(settings.policy.allow_lan_exception);
        self.as_mut().set_relay_only(settings.policy.relay_only);
        self.as_mut()
            .set_vpn_status(vpn_status_code(settings.policy));
        self.as_mut().align_pairing_method_with_policy();
        let previous = usize::try_from(self.rust().network_index)
            .ok()
            .and_then(|index| self.rust().network_ips.get(index))
            .copied();
        let interfaces = kryptopass_lan::local_interfaces().unwrap_or_default();
        let addresses: Vec<_> = interfaces
            .iter()
            .map(|interface| interface.address)
            .collect();
        let selected = kryptopass_lan::select_local_bind_address(
            settings.lan_bind_ip,
            &addresses,
            kryptopass_lan::os_selected_local_address(),
        )
        .or_else(|| previous.filter(|address| addresses.contains(address)));
        let index = selected.and_then(|selected| addresses.iter().position(|ip| *ip == selected));
        let display = interfaces
            .iter()
            .map(|interface| {
                use kryptopass_lan::InterfaceKind;
                let label = match interface.kind {
                    InterfaceKind::Wifi => "Wi-Fi",
                    InterfaceKind::Ethernet => "Ethernet",
                    InterfaceKind::Virtual => crate::i18n::tr_noop("VPN / virtual"),
                    InterfaceKind::Loopback => "Local",
                    InterfaceKind::Other => crate::i18n::tr_noop("Network"),
                };
                QString::from(format!(
                    "{label} · {} — {}",
                    interface.name, interface.address
                ))
            })
            .collect();
        self.as_mut().rust_mut().network_ips = addresses;
        self.as_mut().set_network_addresses(display);
        self.as_mut().set_network_index(
            index
                .and_then(|index| i32::try_from(index).ok())
                .unwrap_or(-1),
        );
    }

    /// Internet is an explicit carrier choice; it never changes device authorization.
    pub fn configure_internet(mut self: Pin<&mut Self>, enabled: bool, relay_origin: QString) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if *self.busy() || self.rust().maintenance_busy {
            return;
        }
        if enabled {
            let origin = relay_origin.to_string();
            if kryptopass_lan::RelayRoute::new(origin.trim(), [1; 32]).is_err() {
                self.as_mut()
                    .set_status_code(QString::from("network_error"));
                self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
                    "Enter a valid WSS server, without credentials or parameters.",
                )));
                return;
            }
            self.as_mut().set_relay_origin(QString::from(origin.trim()));
        }
        self.as_mut().set_internet_enabled(enabled);
    }

    pub fn configure_network(mut self: Pin<&mut Self>, enabled: bool, index: i32) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if *self.busy() {
            return;
        }
        if !enabled {
            self.as_mut().set_lan_enabled(false);
            return;
        }
        // The UI disables this choice too; the backend still refuses it so a
        // stale binding can never enable LAN against the saved privacy policy.
        if !lan_permitted(
            self.rust().require_vpn,
            self.rust().allow_lan_exception,
            self.rust().relay_only,
        ) {
            self.as_mut()
                .set_status_code(QString::from("network_policy_blocked"));
            self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
                "The local network is not available with the chosen privacy.",
            )));
            return;
        }
        if usize::try_from(index)
            .ok()
            .is_some_and(|index| index < self.rust().network_ips.len())
        {
            let Some(bind_ip) = usize::try_from(index)
                .ok()
                .and_then(|index| self.rust().network_ips.get(index))
                .copied()
            else {
                return;
            };
            if let Err(()) = persist_network_settings(Some(bind_ip)) {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(
                    crate::i18n::tr_noop("The selected LAN address could not be saved. A connection will not start with an unsaved policy."),
                ));
                return;
            }
            self.as_mut().set_network_index(index);
            self.as_mut().set_lan_enabled(true);
            if let Some(owner) = self.rust().resume_owner.as_ref() {
                owner.refresh_network_policy();
            }
        } else {
            self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
                "No LAN address is selected. Refresh the interfaces or use emulator mode.",
            )));
        }
    }

    pub fn configure_privacy(
        mut self: Pin<&mut Self>,
        require_vpn: bool,
        allow_lan_exception: bool,
        relay_only: bool,
    ) {
        self.as_mut().cancel_synthetic_key_transfer_operation();
        if self.rust().maintenance_busy {
            return;
        }
        // Changing privacy settings must not pin an automatically displayed
        // route address. Only the explicit LAN selection saves such a preference.
        let bind_ip = peers_directory()
            .ok()
            .and_then(|directory| load_network_privacy_settings(&directory).ok())
            .and_then(|settings| settings.lan_bind_ip);
        // Save only meaningful combinations: the LAN exception exists only
        // while VPN is required, and relay-only excludes every LAN path.
        let allow_lan_exception = require_vpn && allow_lan_exception && !relay_only;
        let policy = NetworkPrivacyPolicy {
            vpn_mode: if require_vpn {
                VpnMode::RequireVpn
            } else {
                VpnMode::FollowSystem
            },
            allow_lan_exception,
            relay_only,
        };
        let directory = match peers_directory() {
            Ok(directory) => directory,
            Err(()) => {
                self.as_mut()
                    .set_status_code(QString::from("network_policy_blocked"));
                self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
                    "The network policy file could not be located. No changes were applied.",
                )));
                return;
            }
        };
        if save_network_privacy_settings(
            &directory,
            NetworkPrivacySettings {
                policy,
                lan_bind_ip: bind_ip,
            },
        )
        .is_err()
        {
            self.as_mut()
                .set_status_code(QString::from("network_policy_blocked"));
            self.as_mut().set_status(QString::from(crate::i18n::tr_noop(
                "The network policy could not be saved. No changes were applied.",
            )));
            return;
        }

        // Invalidate the guard shared by the pending pairing and its active
        // WebRTC transport. Its monitor closes locally without a route fallback.
        if let Some(guard) = self.as_mut().rust_mut().active_network_guard.take() {
            guard.invalidate();
        }
        self.as_mut().set_require_vpn(require_vpn);
        self.as_mut().set_allow_lan_exception(allow_lan_exception);
        self.as_mut().set_relay_only(relay_only);
        self.as_mut().set_vpn_status(vpn_status_code(policy));
        self.as_mut().align_pairing_method_with_policy();
        if let Some(owner) = self.rust().resume_owner.as_ref() {
            owner.refresh_network_policy();
        }
        if self.rust().busy || self.rust().session_active {
            self.as_mut()
                .set_status_code(QString::from("network_policy_changed"));
            self.as_mut().set_status(QString::from(
                crate::i18n::tr_noop("The network policy changed. The operation is being closed and its previous route will not be reused."),
            ));
        }
    }

    /// Keep the pairing method on a route the saved privacy choices allow, so
    /// the window never opens with LAN selected while LAN is disabled.
    fn align_pairing_method_with_policy(mut self: Pin<&mut Self>) {
        let allowed = lan_permitted(
            self.rust().require_vpn,
            self.rust().allow_lan_exception,
            self.rust().relay_only,
        );
        if allowed || self.rust().internet_enabled {
            return;
        }
        self.as_mut().set_lan_enabled(false);
        if !self.rust().relay_origin.is_empty() {
            self.as_mut().set_internet_enabled(true);
        }
    }
}
