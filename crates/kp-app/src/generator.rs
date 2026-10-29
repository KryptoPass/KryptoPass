//! The generator a field offers, configured by its template.

use kp_template::{Library, Template};
use kryptopass_core::SensitiveText;
use kryptopass_generator::{Settings, generate};

use crate::error::AppError;

/// The settings of `field`'s generator, for the generator dialog to adjust
/// within their bounds.
pub fn settings_for(
    library: &Library,
    template: &Template,
    field: &str,
) -> Result<Settings, AppError> {
    let field = template.field(field).ok_or(AppError::UnknownField)?;
    let reference = field.generator.as_ref().ok_or(AppError::NoGenerator)?;
    let generator = library
        .generator(reference.id.as_str())
        .ok_or(AppError::NoGenerator)?;
    Ok(Settings::from_template(generator, Some(reference))?)
}

/// A value from `field`'s generator with its default settings.
pub fn generate_for(
    library: &Library,
    template: &Template,
    field: &str,
) -> Result<SensitiveText, AppError> {
    Ok(generate(&settings_for(library, template, field)?)?)
}
