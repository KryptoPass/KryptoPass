//! Typed Proton Pass export reader, scope resolution and credential grouping.

pub mod analysis;
pub mod data;
pub mod decisions;
pub mod documents;
pub mod folders;
pub mod scope;
pub mod username;

pub use analysis::{
    Analysis, AnalysisError, Analyzer, Credential, CredentialRole, Decision, DecisionKind,
    LoginInput, RotationEvidence, ScopeGroup, SourceRef, Unassigned, UnassignedReason, UserGroup,
};
pub use data::{Export, ExportError};
pub use decisions::DecisionPolicy;
pub use documents::{CanonicalUser, DocumentId, DocumentPolicy};
pub use scope::{OriginScope, ScopeKind, ScopeResolver};
pub use username::{UsernameContext, UsernamePolicy};

pub use folders::domain_folder;
