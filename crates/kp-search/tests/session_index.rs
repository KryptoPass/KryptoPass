use kp_search::{
    CatalogRecord, Coverage, Index, Limits, Projection, RecordId, RecordKey, SearchError,
    SearchSession, SearchSource,
};
use kryptopass_secure_memory::ProtectedBytes;
use std::{
    error::Error,
    io,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn Error>>;

const DEADLINE: Duration = Duration::from_secs(5);
static PROTECTED_TESTS: Mutex<()> = Mutex::new(());

fn protected_test_guard() -> MutexGuard<'static, ()> {
    PROTECTED_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn protected_page_size() -> Result<Option<usize>, Box<dyn Error>> {
    let mut reservations = Vec::new();
    for _ in 0..4 {
        match ProtectedBytes::try_with_capacity(1) {
            Ok(bytes) => reservations.push(bytes),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                eprintln!(
                    "NOT EXERCISED: private search assertions require four protected memory pages; \
                     this environment denied the required protection"
                );
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(reservations.first().map(ProtectedBytes::capacity))
}

fn limits(page: usize) -> Limits {
    Limits {
        block_bytes: page,
        max_private_bytes: 2 * page,
        max_field_bytes: page,
        ..Limits::default()
    }
}

fn id(value: u8) -> RecordId {
    [value; 16]
}

fn key(value: u8, revision: u64) -> RecordKey {
    RecordKey {
        id: id(value),
        revision,
    }
}

fn record(value: u8, revision: u64, fields: &[&str], eligible: bool) -> CatalogRecord {
    CatalogRecord {
        key: key(value, revision),
        public_fields: fields.iter().map(|field| (*field).to_owned()).collect(),
        private_eligible: eligible,
    }
}

fn complete_text(index: &mut Index, expected: RecordKey, text: &str) -> TestResult {
    assert_eq!(index.next_pending(), Some(expected));
    index.complete(expected, Projection::from_text(text, 64 * 1024))?;
    Ok(())
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + DEADLINE;
    while !condition() {
        assert!(Instant::now() < deadline, "search worker did not settle");
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn catalog_changes_are_authoritative_even_before_unlock() -> TestResult {
    let mut index = Index::new(Limits {
        max_records: 2,
        ..Limits::default()
    });
    index.reconcile(vec![
        record(2, 1, &["Papelera"], false),
        record(1, 1, &["Trabajo"], true),
    ])?;
    assert_eq!(index.search(" ")?, vec![id(1), id(2)]);
    assert_eq!(index.next_pending(), None);
    assert_eq!(
        index.coverage(),
        Coverage {
            active: false,
            eligible: 1,
            pending: 1,
            ..Coverage::default()
        }
    );

    assert_eq!(
        index.reconcile(vec![record(3, 1, &[], true), record(3, 2, &[], true)]),
        Err(SearchError::InvalidInput)
    );
    assert_eq!(index.search("")?, vec![id(1), id(2)]);
    assert_eq!(
        index.reconcile(vec![
            record(1, 1, &[], true),
            record(2, 1, &[], true),
            record(3, 1, &[], true),
        ]),
        Err(SearchError::LimitExceeded)
    );
    assert_eq!(index.search("")?, vec![id(1), id(2)]);

    index.set_active(true);
    assert_eq!(index.next_pending(), Some(key(1, 1)));
    assert_eq!(index.next_pending(), None);
    index.reconcile(vec![record(2, 2, &["Restaurado"], true)])?;
    assert_eq!(index.search("")?, vec![id(2)]);
    assert_eq!(
        index.complete(key(1, 1), Err(SearchError::Unavailable)),
        Err(SearchError::StaleRevision)
    );
    assert_eq!(index.next_pending(), Some(key(2, 2)));
    index.set_active(false);
    assert_eq!(index.next_pending(), None);
    assert_eq!(
        index.complete(key(2, 2), Err(SearchError::Unavailable)),
        Err(SearchError::Closed)
    );
    Ok(())
}

#[test]
fn terms_match_across_public_and_private_fields_without_query_syntax() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let mut index = Index::new(limits(page));
    index.reconcile(vec![
        record(1, 1, &["Trabajo", "Correo principal"], true),
        record(2, 1, &["Personal", "Referencia .* [x]"], true),
    ])?;
    index.set_active(true);
    complete_text(&mut index, key(1, 1), "Ána+QA@Ejemplo.org")?;
    complete_text(&mut index, key(2, 1), "ana@otra.org")?;

    assert_eq!(index.search("TRÁBAJO ana+qa@ejemplo.org")?, vec![id(1)]);
    assert_eq!(index.search("ejemplo.org correo trabajo")?, vec![id(1)]);
    assert!(index.search("personal ejemplo.org")?.is_empty());
    assert_eq!(index.search(".* [x]")?, vec![id(2)]);
    assert!(index.search("trabajo .*")?.is_empty());
    assert!(index.search("usuario:ana")?.is_empty());
    assert_eq!(index.search(" \t\n")?, vec![id(1), id(2)]);

    index.set_active(false);
    assert!(index.search("ejemplo.org")?.is_empty());
    assert_eq!(index.search("trabajo")?, vec![id(1)]);
    assert_eq!(index.memory_usage().private_live_bytes, 0);
    assert_eq!(index.memory_usage().private_reserved_bytes, 0);
    assert_eq!(index.memory_usage().protected_regions, 0);
    Ok(())
}

#[test]
fn reconciliation_retains_current_users_and_retires_edited_removed_or_trashed_users() -> TestResult
{
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let mut index = Index::new(limits(page));
    index.reconcile(vec![
        record(1, 1, &["Trabajo"], true),
        record(2, 1, &["Personal"], true),
    ])?;
    index.set_active(true);
    complete_text(&mut index, key(1, 1), "ana-original")?;
    complete_text(&mut index, key(2, 1), "beto-original")?;

    index.reconcile(vec![
        record(1, 1, &["Renombrado"], true),
        record(2, 1, &["Personal"], true),
        record(3, 1, &["Nuevo"], true),
    ])?;
    assert_eq!(index.search("renombrado ana-original")?, vec![id(1)]);
    assert!(index.search("trabajo")?.is_empty());
    assert_eq!(index.coverage().indexed, 2);
    assert_eq!(index.coverage().pending, 1);
    complete_text(&mut index, key(3, 1), "carla-original")?;

    index.reconcile(vec![
        record(1, 2, &["Renombrado"], true),
        record(3, 1, &["Papelera"], false),
    ])?;
    assert!(index.search("ana-original")?.is_empty());
    assert!(index.search("beto-original")?.is_empty());
    assert!(index.search("carla-original")?.is_empty());
    assert_eq!(index.search("papelera")?, vec![id(3)]);
    assert_eq!(index.memory_usage().private_live_bytes, 0);
    assert_eq!(index.memory_usage().private_reserved_bytes, 0);
    assert_eq!(index.coverage().eligible, 1);
    assert_eq!(index.coverage().pending, 1);
    assert_eq!(index.coverage().indexed, 0);

    assert_eq!(index.next_pending(), Some(key(1, 2)));
    assert_eq!(
        index.complete(key(1, 1), Projection::from_text("stale-user", page)),
        Err(SearchError::StaleRevision)
    );
    index.complete(key(1, 2), Projection::from_text("ana-revisada", page))?;
    assert_eq!(index.search("renombrado ana-revisada")?, vec![id(1)]);
    assert!(index.search("stale-user")?.is_empty());

    index.reconcile(vec![
        record(1, 2, &["Renombrado"], true),
        record(3, 1, &["Restaurado"], true),
    ])?;
    complete_text(&mut index, key(3, 1), "carla-restaurada")?;
    assert_eq!(index.search("restaurado carla-restaurada")?, vec![id(3)]);
    assert!(index.search("carla-original")?.is_empty());
    Ok(())
}

#[test]
fn private_allocation_budget_never_changes_catalog_success_or_reuses_removed_values() -> TestResult
{
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let mut index = Index::new(Limits {
        max_private_bytes: page,
        ..limits(page)
    });
    index.reconcile(vec![
        record(1, 1, &["Primero"], true),
        record(2, 1, &["Segundo"], true),
    ])?;
    index.set_active(true);
    complete_text(&mut index, key(1, 1), &"a".repeat(page - 16))?;
    assert_eq!(index.next_pending(), Some(key(2, 1)));
    assert_eq!(
        index.complete(key(2, 1), Projection::from_text(&"b".repeat(32), page)),
        Err(SearchError::LimitExceeded)
    );
    assert_eq!(index.memory_usage().private_live_bytes, page - 16);
    assert_eq!(index.memory_usage().private_reserved_bytes, page);
    assert_eq!(index.memory_usage().protected_regions, 1);
    assert_eq!(index.coverage().indexed, 1);
    assert_eq!(index.coverage().unavailable, 1);
    assert_eq!(index.coverage().pending, 0);
    assert_eq!(index.search("")?, vec![id(1), id(2)]);
    assert_eq!(index.search("segundo")?, vec![id(2)]);
    assert!(index.search("bbbb")?.is_empty());

    index.reconcile(vec![record(3, 1, &["Reemplazo"], true)])?;
    assert_eq!(index.memory_usage().private_live_bytes, 0);
    assert_eq!(index.memory_usage().private_reserved_bytes, 0);
    complete_text(&mut index, key(3, 1), "replacement-user")?;
    assert_eq!(index.search("replacement-user")?, vec![id(3)]);
    assert!(index.search("aaaa")?.is_empty());
    assert_eq!(index.memory_usage().private_reserved_bytes, page);
    Ok(())
}

#[test]
fn adjacent_retired_fields_can_fund_a_larger_user_without_extra_reservation() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let mut index = Index::new(Limits {
        max_private_bytes: page,
        ..limits(page)
    });
    index.reconcile(vec![
        record(1, 1, &[], true),
        record(2, 1, &[], true),
        record(3, 1, &[], true),
    ])?;
    index.set_active(true);
    let portion = page / 4;
    complete_text(&mut index, key(1, 1), &"a".repeat(portion))?;
    complete_text(&mut index, key(2, 1), &"b".repeat(portion))?;
    complete_text(&mut index, key(3, 1), &"c".repeat(portion))?;
    index.reconcile(vec![record(1, 1, &[], true), record(4, 1, &[], true)])?;
    complete_text(&mut index, key(4, 1), &"d".repeat(2 * portion))?;

    assert_eq!(index.memory_usage().private_reserved_bytes, page);
    assert_eq!(index.memory_usage().protected_regions, 1);
    assert_eq!(index.memory_usage().private_live_bytes, 3 * portion);
    assert_eq!(index.search("aaaa")?, vec![id(1)]);
    assert_eq!(index.search("dddd")?, vec![id(4)]);
    assert!(index.search("bbbb")?.is_empty());
    assert!(index.search("cccc")?.is_empty());
    Ok(())
}

#[derive(Default)]
struct ConcurrentLoads {
    active: AtomicUsize,
    maximum: AtomicUsize,
}

struct LoadGuard(Arc<ConcurrentLoads>);

impl Drop for LoadGuard {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

struct ControlledSource {
    name: &'static str,
    started: Sender<(&'static str, RecordKey)>,
    response: Receiver<&'static str>,
    loads: Arc<ConcurrentLoads>,
}

impl SearchSource for ControlledSource {
    fn load(&mut self, record: RecordKey) -> Result<Projection, SearchError> {
        let active = self.loads.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.loads.maximum.fetch_max(active, Ordering::SeqCst);
        let _guard = LoadGuard(self.loads.clone());
        self.started
            .send((self.name, record))
            .map_err(|_| SearchError::Unavailable)?;
        let text = self
            .response
            .recv_timeout(DEADLINE)
            .map_err(|_| SearchError::Unavailable)?;
        Projection::from_text(text, 1024)
    }
}

#[test]
fn an_inflight_revision_cannot_publish_after_the_catalog_is_edited() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let loads = Arc::new(ConcurrentLoads::default());
    let session = SearchSession::new(limits(page))?;
    session.reconcile(vec![record(1, 1, &["Trabajo"], true)])?;
    session.unlock(ControlledSource {
        name: "same-session",
        started: started_tx,
        response: response_rx,
        loads: loads.clone(),
    })?;
    assert_eq!(
        started_rx.recv_timeout(DEADLINE)?,
        ("same-session", key(1, 1))
    );

    session.reconcile(vec![record(1, 2, &["Trabajo"], true)])?;
    response_tx.send("obsolete-user")?;
    assert_eq!(
        started_rx.recv_timeout(DEADLINE)?,
        ("same-session", key(1, 2))
    );
    assert!(session.search("obsolete-user")?.is_empty());
    assert_eq!(session.memory_usage()?.private_live_bytes, 0);
    response_tx.send("current-user")?;
    wait_until(|| session.coverage().is_ok_and(|value| value.indexed == 1));
    assert_eq!(session.search("trabajo current-user")?, vec![id(1)]);
    assert!(session.search("obsolete-user")?.is_empty());
    assert_eq!(loads.maximum.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn locking_and_reopening_rejects_the_old_generation_without_a_second_loader() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (old_response_tx, old_response_rx) = mpsc::channel();
    let (new_response_tx, new_response_rx) = mpsc::channel();
    let loads = Arc::new(ConcurrentLoads::default());
    let session = SearchSession::new(limits(page))?;
    session.reconcile(vec![record(1, 1, &["Trabajo"], true)])?;
    session.unlock(ControlledSource {
        name: "old-session",
        started: started_tx.clone(),
        response: old_response_rx,
        loads: loads.clone(),
    })?;
    assert_eq!(
        started_rx.recv_timeout(DEADLINE)?,
        ("old-session", key(1, 1))
    );

    session.lock();
    assert!(!session.coverage()?.active);
    assert_eq!(session.memory_usage()?.private_reserved_bytes, 0);
    session.unlock(ControlledSource {
        name: "new-session",
        started: started_tx,
        response: new_response_rx,
        loads: loads.clone(),
    })?;
    assert_eq!(loads.active.load(Ordering::SeqCst), 1);
    assert!(matches!(
        started_rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    ));

    old_response_tx.send("old-session-user")?;
    assert_eq!(
        started_rx.recv_timeout(DEADLINE)?,
        ("new-session", key(1, 1))
    );
    assert!(session.search("old-session-user")?.is_empty());
    assert_eq!(session.memory_usage()?.private_live_bytes, 0);
    new_response_tx.send("new-session-user")?;
    wait_until(|| session.coverage().is_ok_and(|value| value.indexed == 1));
    assert_eq!(session.search("new-session-user")?, vec![id(1)]);
    assert!(session.search("old-session-user")?.is_empty());
    assert_eq!(loads.maximum.load(Ordering::SeqCst), 1);

    session.lock();
    assert_eq!(session.memory_usage()?.private_live_bytes, 0);
    assert_eq!(session.memory_usage()?.private_reserved_bytes, 0);
    assert!(session.search("new-session-user")?.is_empty());
    Ok(())
}

#[test]
fn a_removed_inflight_record_never_returns_to_the_index() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let session = SearchSession::new(limits(page))?;
    session.reconcile(vec![record(1, 1, &["Trabajo"], true)])?;
    session.unlock(ControlledSource {
        name: "removed",
        started: started_tx,
        response: response_rx,
        loads: Arc::new(ConcurrentLoads::default()),
    })?;
    assert_eq!(started_rx.recv_timeout(DEADLINE)?, ("removed", key(1, 1)));
    session.reconcile(Vec::new())?;
    let version = session.version();
    response_tx.send("removed-user")?;
    wait_until(|| session.version() != version);

    assert!(session.search("")?.is_empty());
    assert!(session.search("removed-user")?.is_empty());
    assert_eq!(session.coverage()?.eligible, 0);
    assert_eq!(session.memory_usage()?.private_live_bytes, 0);
    assert_eq!(session.memory_usage()?.private_reserved_bytes, 0);
    Ok(())
}

#[test]
fn an_invalid_catalog_closes_private_coverage_instead_of_preserving_old_matches() -> TestResult {
    let _guard = protected_test_guard();
    let Some(page) = protected_page_size()? else {
        return Ok(());
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let session = SearchSession::new(limits(page))?;
    session.reconcile(vec![record(1, 1, &["Trabajo"], true)])?;
    session.unlock(ControlledSource {
        name: "invalid-catalog",
        started: started_tx,
        response: response_rx,
        loads: Arc::new(ConcurrentLoads::default()),
    })?;
    assert_eq!(
        started_rx.recv_timeout(DEADLINE)?,
        ("invalid-catalog", key(1, 1))
    );
    response_tx.send("previous-user")?;
    wait_until(|| session.coverage().is_ok_and(|value| value.indexed == 1));
    assert_eq!(session.search("previous-user")?, vec![id(1)]);

    assert_eq!(
        session.reconcile(vec![record(1, 1, &[], true), record(1, 2, &[], true)]),
        Err(SearchError::InvalidInput)
    );
    assert!(!session.coverage()?.active);
    assert_eq!(session.coverage()?.indexed, 0);
    assert_eq!(session.memory_usage()?.private_live_bytes, 0);
    assert_eq!(session.memory_usage()?.private_reserved_bytes, 0);
    assert!(session.search("previous-user")?.is_empty());
    assert_eq!(session.search("trabajo")?, vec![id(1)]);
    Ok(())
}
