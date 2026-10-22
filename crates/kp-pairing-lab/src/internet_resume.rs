//! Outbound relay listeners reuse the process owner's pinned Noise/session path.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use kryptopass_devices::ConnectionState;
use kryptopass_lan::{NetworkUse, RelayHost, RelayRoute};
use tokio::sync::watch;

use super::{AcceptPolicy, OwnerCommand, OwnerRuntime, clear_negotiating, record_failure};

pub(super) struct RelayWorker {
    route: RelayRoute,
    cancel: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl OwnerRuntime {
    pub(super) fn sync_relay_workers(&mut self, policy: &Arc<Mutex<AcceptPolicy>>) {
        let network_guard = self.network_guard.clone();
        let internet_allowed = !self.network_settings_error
            && network_guard
                .as_ref()
                .is_some_and(|guard| guard.preflight(NetworkUse::Internet).is_ok());
        let routes = if internet_allowed {
            self.host
                .as_ref()
                .and_then(|host| host.internet_routes().ok())
                .unwrap_or_default()
        } else {
            std::collections::BTreeMap::new()
        };
        let allowed: HashMap<_, _> = routes
            .into_iter()
            .filter(|(peer, _)| policy.lock().is_ok_and(|p| !p.suppressed.contains(peer)))
            .take(super::MAX_OWNED_SESSIONS)
            .collect();
        self.relay_workers.retain(|peer, worker| {
            let keep = allowed.get(peer).is_some_and(|route| {
                route.relay_origin() == worker.route.relay_origin()
                    && route.room() == worker.route.room()
            }) && !worker.task.is_finished();
            if !keep {
                worker.cancel.send_replace(true);
                worker.task.abort();
            }
            keep
        });
        let (Some(host), Some(devices)) = (self.host.as_ref(), self.devices.as_ref()) else {
            return;
        };
        let Some(network_guard) = network_guard.filter(|_| internet_allowed) else {
            return;
        };
        for (peer, route) in allowed {
            if self.relay_workers.contains_key(&peer) {
                continue;
            }
            let host = Arc::clone(host);
            let devices = devices.clone();
            let policy = Arc::clone(policy);
            let commands = self.command_sender.clone();
            let (cancel, mut cancelled) = watch::channel(false);
            let task_route = route.clone();
            let task_network_guard = network_guard.clone();
            let task = tokio::spawn(async move {
                let mut failures = 0_u32;
                loop {
                    if *cancelled.borrow() {
                        break;
                    }
                    let idle = devices.snapshot().get(&peer).is_some_and(|d| {
                        matches!(
                            d.connection,
                            ConnectionState::Disconnected | ConnectionState::Failed
                        )
                    }) && policy.lock().is_ok_and(|p| {
                        !p.suppressed.contains(&peer)
                            && !p.active.contains(&peer)
                            && !p.negotiating.contains(&peer)
                    });
                    if !idle {
                        tokio::select! {
                            _ = cancelled.changed() => break,
                            _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                        }
                    }
                    let expected = match host.lookup(peer) {
                        Ok(p) => p,
                        Err(_) => break,
                    };
                    let mut session_id = [0_u8; 16];
                    if getrandom::fill(&mut session_id).is_err()
                        || session_id.iter().all(|v| *v == 0)
                    {
                        break;
                    }
                    let authorize_host = Arc::clone(&host);
                    let authorize_devices = devices.clone();
                    let authorize_policy = Arc::clone(&policy);
                    let operation_guard = task_network_guard.clone();
                    let operation = async {
                        operation_guard
                            .preflight(NetworkUse::Internet)
                            .map_err(|_| kryptopass_lan::LanError::NetworkPolicy)?;
                        let (ice_config, ice_policy) =
                            host.internet_ice_configuration(&operation_guard).await?;
                        let relay = RelayHost::connect_with_privacy(
                            task_route.clone(),
                            ice_config,
                            ice_policy,
                            operation_guard.clone(),
                        )
                        .await?;
                        relay
                            .accept(session_id, move |selector| {
                                if selector != *peer.as_bytes()
                                    || authorize_host.lookup(peer).ok() != Some(expected)
                                {
                                    return false;
                                }
                                if !authorize_devices.snapshot().get(&peer).is_some_and(|d| {
                                    d.identity == expected
                                        && matches!(
                                            d.connection,
                                            ConnectionState::Disconnected | ConnectionState::Failed
                                        )
                                }) {
                                    return false;
                                }
                                authorize_policy.lock().is_ok_and(|mut p| {
                                    p.prune_failures(peer);
                                    if p.suppressed.contains(&peer)
                                        || p.active.contains(&peer)
                                        || p.negotiating.contains(&peer)
                                        || p.cooling_until.contains_key(&peer)
                                        || p.active.len() + p.negotiating.len()
                                            >= super::MAX_OWNED_SESSIONS
                                    {
                                        return false;
                                    }
                                    p.negotiating.insert(peer);
                                    true
                                })
                            })
                            .await
                    };
                    let result = tokio::select! {
                        _ = cancelled.changed() => { clear_negotiating(&policy, peer); break; }
                        result = operation => result,
                    };
                    match result {
                        Ok(attempt) => {
                            if commands
                                .send(OwnerCommand::RelayIncoming(Box::new(attempt)))
                                .await
                                .is_err()
                            {
                                break;
                            }
                            failures = 0;
                        }
                        Err(kryptopass_lan::LanError::NetworkPolicy) => {
                            clear_negotiating(&policy, peer);
                            failures = 0;
                        }
                        Err(_) => {
                            clear_negotiating(&policy, peer);
                            record_failure(&policy, peer);
                            failures = failures.saturating_add(1);
                        }
                    }
                    // A bounded group of attempts is followed by a cool-down. Waiting for
                    // a relay never blocks LAN acceptance or causes trust to be rewritten.
                    let pause = match failures {
                        0 => 1,
                        1 => 2,
                        2 => 4,
                        3 => 8,
                        4 => 16,
                        _ => {
                            failures = 0;
                            300
                        }
                    };
                    tokio::select! {
                        _ = cancelled.changed() => break,
                        _ = tokio::time::sleep(Duration::from_secs(pause)) => {},
                    }
                }
                clear_negotiating(&policy, peer);
            });
            self.relay_workers.insert(
                peer,
                RelayWorker {
                    route,
                    cancel,
                    task,
                },
            );
        }
    }

    pub(super) async fn drain_relay_workers(&mut self) {
        for worker in self.relay_workers.values() {
            worker.cancel.send_replace(true);
        }
        for (_, worker) in self.relay_workers.drain() {
            let mut task = worker.task;
            if tokio::time::timeout(Duration::from_secs(1), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}
