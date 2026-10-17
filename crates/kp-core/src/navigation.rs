//! Read-only Composite projection used by navigation models.

use crate::domain::{FolderId, SecretId, SecretKind};

/// Navigation data is deliberately separate from persisted folder relations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationNode {
    Folder {
        id: FolderId,
        name: String,
        children: Vec<NavigationNode>,
    },
    Secret {
        id: SecretId,
        title: String,
        kind: SecretKind,
    },
}

impl NavigationNode {
    pub fn secret_count(&self) -> usize {
        match self {
            Self::Secret { .. } => 1,
            Self::Folder { children, .. } => children.iter().map(Self::secret_count).sum(),
        }
    }
}
