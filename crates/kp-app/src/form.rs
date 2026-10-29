//! What an editor shows for a template, in the person's language.
//!
//! A form describes fields, never values: values stay in the item's
//! protected content and the UI reads them field by field, so a form can be
//! kept, logged or passed around freely.

use kp_template::{
    Autocomplete, DigitCount, FieldId, FieldType, Library, Multiplicity, SectionId, Template,
    TemplateId, TextFormat,
};

/// The editor of one template.
#[derive(Clone, Debug)]
pub struct Form {
    pub template: TemplateId,
    pub name: String,
    pub icon: String,
    pub sections: Vec<FormSection>,
}

#[derive(Clone, Debug)]
pub struct FormSection {
    pub id: SectionId,
    pub title: String,
    pub fields: Vec<FormField>,
    /// How many fields the person may add here; `None` when not allowed.
    pub custom_fields: Option<u16>,
}

/// One field and how to edit it.
#[derive(Clone, Debug)]
pub struct FormField {
    pub id: FieldId,
    pub label: String,
    pub field_type: FieldType,
    /// Shown hidden; copying clears the clipboard afterwards.
    pub concealed: bool,
    pub required: bool,
    pub copy: bool,
    /// Computed by the app; shown read-only.
    pub derived: bool,
    /// Most entries when the field is a list.
    pub multiple: Option<u16>,
    pub max_bytes: Option<u32>,
    pub markdown: bool,
    pub digits: Option<DigitCount>,
    pub choices: Vec<FormChoice>,
    /// Offers a generate button; see [`crate::generate_for`].
    pub generator: bool,
    pub autocomplete: Option<Autocomplete>,
}

#[derive(Clone, Debug)]
pub struct FormChoice {
    pub id: String,
    pub label: String,
}

impl Form {
    /// The form of `template` with its text in `language`.
    pub fn build(library: &Library, template: &Template, language: &str) -> Self {
        let text = |text| library.text(text, language).unwrap_or_default().to_owned();
        let sections = template
            .sections
            .iter()
            .map(|section| FormSection {
                id: section.id.clone(),
                title: library
                    .section_title(section.id.as_str(), language)
                    .to_owned(),
                fields: section
                    .fields
                    .iter()
                    .map(|field| FormField {
                        id: field.id.clone(),
                        label: text(&field.label),
                        field_type: field.field_type,
                        concealed: field.field_type.is_concealed() || field.conceal,
                        required: field.required,
                        copy: field.copy,
                        derived: field.field_type.is_derived(),
                        multiple: match field.multiplicity {
                            Multiplicity::Single => None,
                            Multiplicity::Many { max } => Some(max),
                        },
                        max_bytes: field.max_bytes,
                        markdown: field.format == Some(TextFormat::Markdown),
                        digits: field.digits,
                        choices: field
                            .choices
                            .iter()
                            .map(|choice| FormChoice {
                                id: choice.clone(),
                                label: library.option_label(choice, language).to_owned(),
                            })
                            .collect(),
                        generator: field.generator.is_some(),
                        autocomplete: field.autocomplete,
                    })
                    .collect(),
                custom_fields: section.custom_fields.map(|custom| custom.max),
            })
            .collect();
        Self {
            template: template.id.clone(),
            name: text(&template.meta.name),
            icon: template.meta.icon.as_str().to_owned(),
            sections,
        }
    }
}
