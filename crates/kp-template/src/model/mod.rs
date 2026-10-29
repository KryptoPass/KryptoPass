//! The data model: plain types with no parsing, I/O or validation logic.
//!
//! One file per concept. Types here may reference each other but never depend
//! on code outside this module, so the model can be reused by a parser, a
//! validator or a renderer without pulling their dependencies.

mod action;
mod behavior;
mod catalog;
mod check;
mod field;
mod generator;
mod id;
mod message;
mod migration;
mod section;
mod template;
mod text;
pub(crate) mod vocabulary;

pub use action::{Action, ActionKind, QrFormat};
pub use behavior::{Behavior, EventKind};
pub use catalog::{Catalog, LanguageTag};
pub use check::{Check, CheckKind};
pub use field::{
    Autocomplete, DatePrecision, DigitCount, Field, FieldType, Multiplicity, Prop, SearchMode,
    TextFormat,
};
pub use generator::{CharClass, ClassDef, GeneratorDef, GeneratorParam, GeneratorRef, ParamDef};
pub use id::{FieldId, GeneratorId, IconName, InvalidId, MessageId, SectionId, TemplateId};
pub use message::{MessageRef, MessageScope};
pub use migration::{Migration, MigrationStep};
pub use section::{CustomFields, Section};
pub(crate) use template::{ITEM_TITLE, USER_ID_PREFIX};
pub use template::{Meta, Template, TemplateKind};
pub use text::{LocalizedLiteral, Text};
pub(crate) use vocabulary::Named;
