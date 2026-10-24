use crate::{
    CatalogRecord, Coverage, Index, Limits, MemoryUsage, Projection, RecordId, RecordKey,
    SearchError,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Implementations provide only the allowed field and must verify the requested
/// revision and access. Concrete source types are monomorphized in the loader.
pub trait SearchSource: Send + 'static {
    fn load(&mut self, record: RecordKey) -> Result<Projection, SearchError>;
}

struct State<S> {
    index: Index,
    next_source: Option<S>,
}

struct Shared<S> {
    lifecycle: Mutex<()>,
    state: Mutex<State<S>>,
    wake: Condvar,
    epoch: AtomicU64,
    version: AtomicU64,
    shutdown: AtomicBool,
}

/// One loader for the controller's lifetime. Session changes invalidate its
/// generation; reopening never creates another concurrently decrypting worker.
pub struct SearchSession<S: SearchSource> {
    shared: Arc<Shared<S>>,
}

impl<S: SearchSource> SearchSession<S> {
    pub fn new(limits: Limits) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            lifecycle: Mutex::new(()),
            state: Mutex::new(State {
                index: Index::new(limits),
                next_source: None,
            }),
            wake: Condvar::new(),
            epoch: AtomicU64::new(0),
            version: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("private-search".into())
            .spawn(move || load_loop(worker))?;
        Ok(Self { shared })
    }

    pub fn unlock(&self, source: S) -> Result<(), SearchError> {
        let _transition = self
            .shared
            .lifecycle
            .lock()
            .map_err(|_| SearchError::Unavailable)?;
        self.advance_epoch()?;
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| SearchError::Unavailable)?;
        state.index.set_active(true);
        state.next_source = Some(source);
        self.changed();
        Ok(())
    }

    pub fn lock(&self) {
        let _transition = self
            .shared
            .lifecycle
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        // Queries observe cancellation before this method waits for their lock.
        let _ = self.advance_epoch();
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.index.set_active(false);
        state.next_source = None;
        self.changed();
    }

    pub fn reconcile(&self, records: Vec<CatalogRecord>) -> Result<(), SearchError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| SearchError::Unavailable)?;
        if let Err(error) = state.index.reconcile(records) {
            // Untrusted or over-budget catalogs cannot leave stale private matches.
            state.index.set_active(false);
            self.changed();
            return Err(error);
        }
        self.changed();
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<RecordId>, SearchError> {
        self.search_characters(query.chars().map(Ok))
    }

    pub fn search_characters<I>(&self, characters: I) -> Result<Vec<RecordId>, SearchError>
    where
        I: Iterator<Item = Result<char, SearchError>> + Clone,
    {
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        self.shared
            .state
            .lock()
            .map_err(|_| SearchError::Unavailable)?
            .index
            .search_characters(characters, || {
                self.shared.epoch.load(Ordering::Acquire) != epoch
            })
    }

    pub fn coverage(&self) -> Result<Coverage, SearchError> {
        Ok(self
            .shared
            .state
            .lock()
            .map_err(|_| SearchError::Unavailable)?
            .index
            .coverage())
    }

    pub fn memory_usage(&self) -> Result<MemoryUsage, SearchError> {
        Ok(self
            .shared
            .state
            .lock()
            .map_err(|_| SearchError::Unavailable)?
            .index
            .memory_usage())
    }

    pub fn version(&self) -> u64 {
        self.shared.version.load(Ordering::Acquire)
    }

    fn advance_epoch(&self) -> Result<(), SearchError> {
        self.shared
            .epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |epoch| {
                epoch.checked_add(1)
            })
            .map(|_| ())
            .map_err(|_| SearchError::Closed)
    }

    fn changed(&self) {
        self.shared.version.fetch_add(1, Ordering::Release);
        self.shared.wake.notify_one();
    }
}

impl<S: SearchSource> Drop for SearchSession<S> {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.lock();
    }
}

fn load_loop<S: SearchSource>(shared: Arc<Shared<S>>) {
    let mut source = None;
    let mut source_epoch = 0;
    loop {
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        if shared.shutdown.load(Ordering::Acquire) {
            return;
        }
        let epoch = shared.epoch.load(Ordering::Acquire);
        if epoch != source_epoch {
            source = None;
            source_epoch = epoch;
        }
        if let Some(next) = state.next_source.take() {
            source = Some(next);
        }
        let pending = if source.is_some() {
            state.index.next_pending()
        } else {
            None
        };
        let Some(record) = pending else {
            // Condvar releases the same mutex used by reconcile/unlock/lock.
            match shared.wake.wait(state) {
                Ok(_) => continue,
                Err(_) => return,
            }
        };
        drop(state);
        let Some(loader) = source.as_mut() else {
            continue;
        };
        let projection = loader.load(record);
        let Ok(mut state) = shared.state.lock() else {
            return;
        };
        if shared.epoch.load(Ordering::Acquire) == epoch && !shared.shutdown.load(Ordering::Acquire)
        {
            let _ = state.index.complete(record, projection);
            shared.version.fetch_add(1, Ordering::Release);
        }
        // Otherwise the stale projection is dropped without entering the index.
    }
}
