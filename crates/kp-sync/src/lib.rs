//! Sync: signed item and folder versions, deletion marks and blobs exchanged
//! between authorized devices, over a store the caller supplies.
//!
//! This crate has no GUI, socket, OpenPGP runtime or database. Callers supply
//! a [`SyncStore`], identities already authorized by the pairing trust store
//! and the keys each peer syncs. The contract is `docs/sync/contrato.md`.

mod model;
mod session;
mod store;
mod wire;

pub use model::*;
pub use session::*;
pub use store::*;
pub use wire::*;
