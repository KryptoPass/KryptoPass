use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use thiserror::Error;

use super::{Export, Item, LoginEntry, Vault};

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("cannot read export: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid export JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl Export {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ExportError> {
        let file = File::open(path)?;
        Self::from_reader(BufReader::new(file))
    }

    pub fn from_reader(reader: impl Read) -> Result<Self, ExportError> {
        Ok(serde_json::from_reader(reader)?)
    }

    pub fn iter_vaults(&self) -> impl Iterator<Item = (&str, &Vault)> + '_ {
        self.vaults.iter().map(|(id, vault)| (id.as_str(), vault))
    }

    pub fn iter_items(&self) -> impl Iterator<Item = &Item> + '_ {
        self.iter_vaults().flat_map(|(_, vault)| vault.items.iter())
    }

    pub fn iter_logins(&self) -> impl Iterator<Item = LoginEntry> + '_ {
        self.iter_vaults().flat_map(|(_, vault)| {
            vault
                .items
                .iter()
                .filter_map(|item| LoginEntry::from_item(&vault.name, item))
        })
    }
}

impl std::str::FromStr for Export {
    type Err = ExportError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(source)?)
    }
}
