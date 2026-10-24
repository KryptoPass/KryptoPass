use crate::{
    Projection, SearchError,
    arena::{Arena, Span},
    normalize::public_field,
};
use std::collections::VecDeque;

pub type RecordId = [u8; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordKey {
    pub id: RecordId,
    pub revision: u64,
}

/// Public list metadata only. Private values enter exclusively through Projection.
#[derive(Clone)]
pub struct CatalogRecord {
    pub key: RecordKey,
    pub public_fields: Vec<String>,
    pub private_eligible: bool,
}

#[derive(Clone, Copy)]
pub struct Limits {
    pub max_records: usize,
    pub max_private_bytes: usize,
    pub block_bytes: usize,
    pub max_field_bytes: usize,
    pub max_query_bytes: usize,
    pub max_query_terms: usize,
    pub max_public_bytes_per_record: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_records: 100_000,
            max_private_bytes: 2 * 1024 * 1024,
            block_bytes: 64 * 1024,
            max_field_bytes: 64 * 1024,
            max_query_bytes: 1024,
            max_query_terms: 16,
            max_public_bytes_per_record: 16 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub active: bool,
    pub eligible: usize,
    pub indexed: usize,
    pub pending: usize,
    pub unavailable: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryUsage {
    pub private_live_bytes: usize,
    pub private_reserved_bytes: usize,
    pub protected_regions: usize,
    pub metadata_capacity_bytes: usize,
}

enum PrivateState {
    Pending,
    Loading,
    Ready(Span),
    Unavailable,
}

struct Entry {
    record: CatalogRecord,
    private: PrivateState,
}

pub struct Index {
    entries: Vec<Entry>,
    pending: VecDeque<RecordKey>,
    arena: Arena,
    limits: Limits,
    active: bool,
}

impl Index {
    pub fn new(limits: Limits) -> Self {
        Self {
            entries: Vec::new(),
            pending: VecDeque::new(),
            arena: Arena::new(limits.max_private_bytes, limits.block_bytes),
            limits,
            active: false,
        }
    }

    /// Invalidating the session wipes private bytes while retaining public metadata.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
        self.arena.clear();
        for entry in &mut self.entries {
            entry.private = PrivateState::Pending;
        }
        self.rebuild_pending();
    }

    /// Replaces an authoritative catalog atomically. Unchanged revisions retain
    /// their projections; removed, trashed or changed revisions are wiped now.
    pub fn reconcile(&mut self, mut records: Vec<CatalogRecord>) -> Result<(), SearchError> {
        if records.len() > self.limits.max_records {
            return Err(SearchError::LimitExceeded);
        }
        records.sort_unstable_by_key(|record| record.key.id);
        if records
            .windows(2)
            .any(|pair| pair[0].key.id == pair[1].key.id)
        {
            return Err(SearchError::InvalidInput);
        }
        for record in &mut records {
            if record.public_fields.len() > 16 {
                return Err(SearchError::LimitExceeded);
            }
            let mut size = 0usize;
            let mut normalized_size = 0usize;
            for field in &mut record.public_fields {
                size = size
                    .checked_add(field.len())
                    .ok_or(SearchError::LimitExceeded)?;
                if size > self.limits.max_public_bytes_per_record {
                    return Err(SearchError::LimitExceeded);
                }
                *field = public_field(field, self.limits.max_public_bytes_per_record)?;
                normalized_size = normalized_size
                    .checked_add(field.len())
                    .ok_or(SearchError::LimitExceeded)?;
                if normalized_size > self.limits.max_public_bytes_per_record {
                    return Err(SearchError::LimitExceeded);
                }
            }
        }
        let mut previous = std::mem::take(&mut self.entries).into_iter().peekable();
        let mut entries = Vec::with_capacity(records.len());
        for record in records {
            while previous
                .peek()
                .is_some_and(|old| old.record.key.id < record.key.id)
            {
                if let Some(old) = previous.next() {
                    self.retire(old.private);
                }
            }
            let private = if previous
                .peek()
                .is_some_and(|old| old.record.key.id == record.key.id)
            {
                let old = previous.next().ok_or(SearchError::InvalidInput)?;
                if old.record.key == record.key
                    && record.private_eligible == old.record.private_eligible
                {
                    old.private
                } else {
                    self.retire(old.private);
                    PrivateState::Pending
                }
            } else {
                PrivateState::Pending
            };
            entries.push(Entry { record, private });
        }
        for old in previous {
            self.retire(old.private);
        }
        self.entries = entries;
        self.rebuild_pending();
        Ok(())
    }

    fn retire(&mut self, state: PrivateState) {
        if let PrivateState::Ready(span) = state {
            self.arena.remove(span);
        }
    }

    fn rebuild_pending(&mut self) {
        self.pending.clear();
        if self.active {
            self.pending.extend(self.entries.iter().filter_map(|entry| {
                (entry.record.private_eligible && matches!(entry.private, PrivateState::Pending))
                    .then_some(entry.record.key)
            }));
        }
    }

    pub fn next_pending(&mut self) -> Option<RecordKey> {
        if !self.active {
            return None;
        }
        while let Some(key) = self.pending.pop_front() {
            let Ok(index) = self
                .entries
                .binary_search_by_key(&key.id, |entry| entry.record.key.id)
            else {
                continue;
            };
            let entry = &mut self.entries[index];
            if entry.record.key == key && matches!(entry.private, PrivateState::Pending) {
                entry.private = PrivateState::Loading;
                return Some(key);
            }
        }
        None
    }

    pub fn complete(
        &mut self,
        key: RecordKey,
        projection: Result<Projection, SearchError>,
    ) -> Result<(), SearchError> {
        if !self.active {
            return Err(SearchError::Closed);
        }
        let index = self
            .entries
            .binary_search_by_key(&key.id, |entry| entry.record.key.id)
            .map_err(|_| SearchError::StaleRevision)?;
        let entry = &self.entries[index];
        if entry.record.key != key
            || !entry.record.private_eligible
            || !matches!(entry.private, PrivateState::Loading)
        {
            return Err(SearchError::StaleRevision);
        }
        let span = projection.and_then(|projection| {
            if projection.bytes().len() > self.limits.max_field_bytes {
                return Err(SearchError::LimitExceeded);
            }
            self.arena.insert(projection.bytes())
        });
        match span {
            Ok(span) => {
                self.entries[index].private = PrivateState::Ready(span);
                Ok(())
            }
            Err(error) => {
                self.entries[index].private = PrivateState::Unavailable;
                Err(error)
            }
        }
    }

    pub fn search(&self, query: &str) -> Result<Vec<RecordId>, SearchError> {
        self.search_characters(query.chars().map(Ok), || false)
    }

    pub(crate) fn search_characters<I, C>(
        &self,
        characters: I,
        cancelled: C,
    ) -> Result<Vec<RecordId>, SearchError>
    where
        I: Iterator<Item = Result<char, SearchError>> + Clone,
        C: Fn() -> bool,
    {
        let mut original_bytes = 0usize;
        let mut empty = true;
        for character in characters.clone() {
            let character = character?;
            original_bytes = original_bytes
                .checked_add(character.len_utf8())
                .ok_or(SearchError::LimitExceeded)?;
            if original_bytes > self.limits.max_query_bytes {
                return Err(SearchError::LimitExceeded);
            }
            empty &= character.is_whitespace();
        }
        if empty {
            return if cancelled() {
                Err(SearchError::Closed)
            } else {
                Ok(self
                    .entries
                    .iter()
                    .map(|entry| entry.record.key.id)
                    .collect())
            };
        }
        let query = Projection::from_characters(characters, self.limits.max_query_bytes)?;
        if query.bytes().is_empty() {
            return Err(SearchError::InvalidInput);
        }
        let text = std::str::from_utf8(query.bytes()).map_err(|_| SearchError::InvalidInput)?;
        let terms: Vec<&str> = text.split_whitespace().collect();
        if terms.len() > self.limits.max_query_terms {
            return Err(SearchError::LimitExceeded);
        }
        let mut matches = Vec::new();
        for entry in &self.entries {
            if cancelled() {
                return Err(SearchError::Closed);
            }
            let private = match entry.private {
                PrivateState::Ready(span) if self.active => self
                    .arena
                    .get(span)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok()),
                _ => None,
            };
            if terms.iter().all(|term| {
                entry
                    .record
                    .public_fields
                    .iter()
                    .any(|field| field.contains(term))
                    || private.is_some_and(|field| field.contains(term))
            }) {
                matches.push(entry.record.key.id);
            }
        }
        Ok(matches)
    }

    pub fn coverage(&self) -> Coverage {
        let mut coverage = Coverage {
            active: self.active,
            ..Coverage::default()
        };
        for entry in &self.entries {
            if !entry.record.private_eligible {
                continue;
            }
            coverage.eligible += 1;
            match entry.private {
                PrivateState::Ready(_) => coverage.indexed += 1,
                PrivateState::Pending | PrivateState::Loading => coverage.pending += 1,
                PrivateState::Unavailable => coverage.unavailable += 1,
            }
        }
        coverage
    }

    pub fn memory_usage(&self) -> MemoryUsage {
        MemoryUsage {
            private_live_bytes: self.arena.live(),
            private_reserved_bytes: self.arena.reserved(),
            protected_regions: self.arena.regions(),
            metadata_capacity_bytes: self.entries.capacity() * size_of::<Entry>()
                + self.pending.capacity() * size_of::<RecordKey>()
                + self.arena.metadata_bytes()
                + self
                    .entries
                    .iter()
                    .map(|entry| {
                        entry.record.public_fields.capacity() * size_of::<String>()
                            + entry
                                .record
                                .public_fields
                                .iter()
                                .map(String::capacity)
                                .sum::<usize>()
                    })
                    .sum::<usize>(),
        }
    }
}
