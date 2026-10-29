//! A single field of a template.

mod kind;
mod options;

pub use kind::{DatePrecision, FieldType, TextFormat};
pub use options::{Autocomplete, DigitCount, Multiplicity, Prop, SearchMode};

use super::generator::GeneratorRef;
use super::id::FieldId;
use super::message::MessageRef;
use super::text::Text;

/// One value an item stores and how it is shown and edited.
///
/// The attributes `format`, `precision`, `browser_match` and the digit range
/// only make sense for one field type each; the model keeps them as plain
/// options and the parser rejects them on other types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Stable key of the stored value.
    pub id: FieldId,
    /// Type, which fixes the default behavior of the field.
    pub field_type: FieldType,
    /// Label shown next to the value.
    pub label: Text,
    /// The item cannot be saved without a value.
    pub required: bool,
    /// Offers a copy action. Copying a concealed field clears the clipboard
    /// afterwards.
    pub copy: bool,
    /// Extra concealment requested on a visible type. It cannot reveal a
    /// concealed type, see [`FieldType::is_concealed`].
    pub conceal: bool,
    /// Hint for autofill, using HTML tokens.
    pub autocomplete: Option<Autocomplete>,
    /// Whether the decrypted value enters the in-memory search index.
    pub search: SearchMode,
    /// Previous values are kept as versions, bounded by the template behavior.
    pub history: bool,
    /// One value or a list.
    pub multiplicity: Multiplicity,
    /// `url` only: the field may match the site the browser is on.
    pub browser_match: bool,
    /// `multiline` only: how the text is rendered.
    pub format: Option<TextFormat>,
    /// `date` only: granularity of the value.
    pub precision: Option<DatePrecision>,
    /// Upper bound of the stored value in bytes, not characters.
    pub max_bytes: Option<u32>,
    /// `digits` only: how many digits a value has.
    pub digits: Option<DigitCount>,
    /// `choice` only: the values a person can pick, read from the `options`
    /// property and checked by the parser.
    pub choices: Vec<String>,
    /// Generator offered to fill the field.
    pub generator: Option<GeneratorRef>,
    /// Short help text shown with the field.
    pub hint: Option<MessageRef>,
    /// Properties interpreted only by the implementation of this template.
    pub props: Vec<Prop>,
}
