//! Qt presentation adapter for the independent session-search service.

use crate::search::{self, VaultSearchSession};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use kryptopass_search::{CatalogRecord, Limits, RecordKey, SearchError};
use serde::Deserialize;
use std::{collections::HashMap, pin::Pin, sync::Arc};
use uuid::Uuid;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        include!("cxx-qt-lib/qstringlist.h");
        type QString = cxx_qt_lib::QString;
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Exposes matching IDs and coverage, never the indexed private fields.
        #[qobject]
        #[qml_element]
        #[qproperty(i32, generation, READ, NOTIFY)]
        #[qproperty(bool, building, READ, NOTIFY)]
        #[qproperty(bool, incomplete, READ, NOTIFY)]
        #[qproperty(bool, query_rejected, cxx_name = "queryRejected", READ, NOTIFY)]
        type SearchController = super::SearchControllerRust;

        #[qinvokable]
        #[cxx_name = "setCatalog"]
        fn set_catalog(
            self: Pin<&mut Self>,
            entries: &QStringList,
            folders: &QStringList,
            kinds: &QStringList,
        );
        #[qinvokable]
        fn poll(self: Pin<&mut Self>);
        #[qinvokable]
        #[cxx_name = "matchingIds"]
        fn matching_ids(self: Pin<&mut Self>, query: &QString) -> QStringList;
    }
}

pub struct SearchControllerRust {
    service: Option<Arc<VaultSearchSession>>,
    fault: bool,
    observed_version: u64,
    generation: i32,
    building: bool,
    incomplete: bool,
    query_rejected: bool,
}

impl Default for SearchControllerRust {
    fn default() -> Self {
        Self {
            service: search::attach(),
            fault: false,
            observed_version: 0,
            generation: 0,
            building: false,
            incomplete: false,
            query_rejected: false,
        }
    }
}

impl qobject::SearchController {
    pub fn set_catalog(
        mut self: Pin<&mut Self>,
        entries: &QStringList,
        folders: &QStringList,
        kinds: &QStringList,
    ) {
        let records = catalog(entries, folders, kinds);
        {
            let mut state = self.as_mut().rust_mut();
            state.fault = match (&state.service, records) {
                (Some(service), Ok(records)) => service.reconcile(records).is_err(),
                (Some(service), Err(_)) => {
                    let _ = service.reconcile(Vec::new());
                    true
                }
                _ => true,
            };
        }
        self.poll();
    }

    pub fn poll(mut self: Pin<&mut Self>) {
        let state = self.rust();
        let version = state
            .service
            .as_ref()
            .map_or(0, |service| service.version());
        if state.service.is_some() && state.observed_version == version {
            return;
        }
        let coverage = state
            .service
            .as_ref()
            .and_then(|service| service.coverage().ok());
        let unlocked = search::is_unlocked();
        let building =
            unlocked && coverage.is_some_and(|coverage| coverage.active && coverage.pending > 0);
        let incomplete = unlocked
            && (state.fault
                || coverage.is_none_or(|coverage| !coverage.active || coverage.unavailable > 0));
        if *self.building() != building {
            self.as_mut().rust_mut().building = building;
            self.as_mut().building_changed();
        }
        if *self.incomplete() != incomplete {
            self.as_mut().rust_mut().incomplete = incomplete;
            self.as_mut().incomplete_changed();
        }
        if self.rust().observed_version != version {
            {
                let mut state = self.as_mut().rust_mut();
                state.observed_version = version;
                state.generation = state.generation.wrapping_add(1);
            }
            self.generation_changed();
        }
    }

    pub fn matching_ids(mut self: Pin<&mut Self>, query: &QString) -> QStringList {
        let result = self
            .rust()
            .service
            .as_ref()
            .ok_or(SearchError::Unavailable)
            .and_then(|service| {
                service.search_characters(
                    char::decode_utf16(query.as_slice().iter().copied())
                        .map(|character| character.map_err(|_| SearchError::InvalidInput)),
                )
            });
        let rejected = result.is_err();
        if *self.query_rejected() != rejected {
            self.as_mut().rust_mut().query_rejected = rejected;
            self.as_mut().query_rejected_changed();
        }
        result
            .unwrap_or_default()
            .into_iter()
            .map(|id| QString::from(Uuid::from_bytes(id).to_string()))
            .collect()
    }
}

#[derive(Deserialize)]
struct EntryRecord {
    id: Uuid,
    revision: u64,
    title: String,
    folder: String,
    kind: String,
    trashed: bool,
}
#[derive(Deserialize)]
struct FolderRecord {
    id: String,
    name: String,
}
#[derive(Deserialize)]
struct KindRecord {
    id: String,
    name: String,
}

fn catalog(
    entries: &QStringList,
    folders: &QStringList,
    kinds: &QStringList,
) -> Result<Vec<CatalogRecord>, SearchError> {
    if entries.len() > Limits::default().max_records as isize {
        return Err(SearchError::LimitExceeded);
    }
    let folders: HashMap<_, _> = folders
        .iter()
        .map(|record| {
            serde_json::from_str::<FolderRecord>(&record.to_string())
                .map(|folder| (folder.id, folder.name))
                .map_err(|_| SearchError::InvalidInput)
        })
        .collect::<Result<_, _>>()?;
    let kinds: HashMap<_, _> = kinds
        .iter()
        .map(|record| {
            serde_json::from_str::<KindRecord>(&record.to_string())
                .map(|kind| (kind.id, kind.name))
                .map_err(|_| SearchError::InvalidInput)
        })
        .collect::<Result<_, _>>()?;
    entries
        .iter()
        .map(|record| {
            let record: EntryRecord =
                serde_json::from_str(&record.to_string()).map_err(|_| SearchError::InvalidInput)?;
            Ok(CatalogRecord {
                key: RecordKey {
                    id: *record.id.as_bytes(),
                    revision: record.revision,
                },
                public_fields: vec![
                    record.title,
                    folders
                        .get(&record.folder)
                        .cloned()
                        .unwrap_or_else(|| crate::i18n::text("Sin carpeta")),
                    kinds
                        .get(&record.kind)
                        .cloned()
                        .unwrap_or_else(|| record.kind.clone()),
                ],
                private_eligible: record.kind == "login" && !record.trashed,
            })
        })
        .collect()
}
