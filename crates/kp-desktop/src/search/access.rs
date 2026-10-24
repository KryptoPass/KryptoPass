//! Search follows the Rust vault boundary, including when no QML observer runs.

use super::VaultSearchSource;
use kryptopass_core::KeyFingerprint;
use kryptopass_search::{Limits, SearchSession};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

pub(crate) type VaultSearchSession = SearchSession<VaultSearchSource>;

#[derive(Clone)]
struct Grant {
    path: PathBuf,
    primary: KeyFingerprint,
}

#[derive(Default)]
struct Access {
    unlocked: bool,
    grant: Option<Grant>,
    consumers: Vec<Weak<VaultSearchSession>>,
}

// Desktop has one active vault. Only its verified Rust lifecycle issues grants;
// weak registrations do not keep a closed view or its private bytes alive.
static ACCESS: Mutex<Access> = Mutex::new(Access {
    unlocked: false,
    grant: None,
    consumers: Vec::new(),
});

pub(crate) fn attach() -> Option<Arc<VaultSearchSession>> {
    let service = Arc::new(SearchSession::new(Limits::default()).ok()?);
    let mut access = ACCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    access
        .consumers
        .retain(|consumer| consumer.strong_count() > 0);
    if let Some(grant) = &access.grant {
        let _ = service.unlock(VaultSearchSource::new(
            grant.path.clone(),
            grant.primary.clone(),
        ));
    }
    access.consumers.push(Arc::downgrade(&service));
    Some(service)
}

pub(crate) fn is_unlocked() -> bool {
    ACCESS
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .unlocked
}

pub(crate) fn set_locked(locked: bool, primary: Option<&KeyFingerprint>) {
    let mut access = ACCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    access.unlocked = !locked;
    access.grant = if locked {
        None
    } else {
        primary.and_then(|primary| {
            crate::storage::paths::database_path(crate::app_info::APP.desktop_file_id)
                .ok()
                .map(|path| Grant {
                    path,
                    primary: primary.clone(),
                })
        })
    };
    let grant = access.grant.clone();
    access.consumers.retain(|consumer| {
        let Some(service) = consumer.upgrade() else {
            return false;
        };
        service.lock();
        if let Some(grant) = &grant {
            let _ = service.unlock(VaultSearchSource::new(
                grant.path.clone(),
                grant.primary.clone(),
            ));
        }
        true
    });
}
