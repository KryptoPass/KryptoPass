//! Search follows the Rust vault boundary, including when no QML observer runs.

use kryptopass_app::VaultSearchSource;
use kryptopass_search::{Limits, SearchSession};
use std::sync::{Arc, Mutex, Weak};

pub(crate) type VaultSearchSession = SearchSession<VaultSearchSource>;

#[derive(Default)]
struct Access {
    unlocked: bool,
    consumers: Vec<Weak<VaultSearchSession>>,
}

// Desktop has one active vault. Only its verified Rust lifecycle issues grants;
// weak registrations do not keep a closed view or its private bytes alive.
static ACCESS: Mutex<Access> = Mutex::new(Access {
    unlocked: false,
    consumers: Vec::new(),
});

pub(crate) fn attach() -> Option<Arc<VaultSearchSession>> {
    let service = Arc::new(SearchSession::new(Limits::default()).ok()?);
    let mut access = ACCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    access
        .consumers
        .retain(|consumer| consumer.strong_count() > 0);
    if access.unlocked {
        if let Some(source) = search_source() {
            let _ = service.unlock(source);
        }
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

pub(crate) fn set_locked(locked: bool) {
    let mut access = ACCESS.lock().unwrap_or_else(|poison| poison.into_inner());
    access.unlocked = !locked;
    access.consumers.retain(|consumer| {
        let Some(service) = consumer.upgrade() else {
            return false;
        };
        service.lock();
        if !locked {
            if let Some(source) = search_source() {
                let _ = service.unlock(source);
            }
        }
        true
    });
}

/// Private values of the active vault, read only with cached passphrases.
fn search_source() -> Option<VaultSearchSource> {
    match crate::vault_access::open_session() {
        Ok(session) => Some(VaultSearchSource::new(&session)),
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "private search is unavailable");
            None
        }
    }
}
