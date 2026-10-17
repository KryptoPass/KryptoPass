use crate::{Detection, ImportLimits, Importer, ImporterInfo, RegistryError};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedImporter {
    pub info: &'static ImporterInfo,
    pub detection: Detection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredImporter {
    pub info: &'static ImporterInfo,
    pub decryption_available: bool,
}

/// Registration is explicit; detection leaves provider selection to the host.
#[derive(Default)]
pub struct ImporterRegistry {
    importers: BTreeMap<&'static str, Box<dyn Importer>>,
}

impl ImporterRegistry {
    pub fn register(&mut self, importer: Box<dyn Importer>) -> Result<(), RegistryError> {
        let id = importer.info().id;
        if id.is_empty() {
            return Err(RegistryError::EmptyIdentifier);
        }
        match self.importers.entry(id) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(importer);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(RegistryError::DuplicateIdentifier)
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&dyn Importer> {
        self.importers.get(id).map(Box::as_ref)
    }

    pub fn importers(&self) -> impl Iterator<Item = RegisteredImporter> + '_ {
        self.importers.values().map(|importer| RegisteredImporter {
            info: importer.info(),
            decryption_available: importer.decryption_available(),
        })
    }

    pub fn detect(&self, input: &[u8], limits: &ImportLimits) -> Vec<DetectedImporter> {
        let mut matches: Vec<_> = self
            .importers
            .values()
            .filter_map(|importer| {
                let detection = importer.detect(input, limits);
                (detection != Detection::Unsupported).then_some(DetectedImporter {
                    info: importer.info(),
                    detection,
                })
            })
            .collect();
        matches.sort_by_key(|candidate| std::cmp::Reverse(candidate.detection));
        matches
    }
}
