//! Template-driven item payload.
//!
//! The payload records which template and version produced it and the values
//! keyed by field id. The template, not these types, says which fields exist,
//! so adding an item type never changes this code.
//!
//! Reading is decode, then migrate to the current template version. Writing
//! is normalize, validate, encode; `encode_item` validates on its own, so
//! invalid content is never written.
//!
//! Memory: values live in zeroizing buffers. Writing streams escapes straight
//! into the output buffer, and reading decodes them from the raw input (see
//! `json_text`), so serde_json keeps no plaintext copy. Copies made after
//! decoding, by the UI or a formatter, are outside this module's control.

mod checks;
mod codec;
mod content;
mod format;
mod issue;
mod json_text;
mod migrate;
mod normalize;
mod ssh;
mod validate;
mod value;

pub use checks::ActionError;
pub use codec::{CodecError, decode_item, encode_item, encode_item_with};
pub use content::{CustomValue, ITEM_SCHEMA_VERSION, ItemContent, ItemValue};
pub use format::{FormatCheck, FormatRules};
pub use issue::{IssueCode, IssueTarget, ItemIssue, ItemIssues};
pub use migrate::MigrationError;
pub use value::FieldValue;
