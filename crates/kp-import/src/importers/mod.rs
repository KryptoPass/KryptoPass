pub mod proton;

use crate::ImporterInfo;

static AVAILABLE_IMPORTERS: &[&ImporterInfo] = &[&proton::PROTON_PASS_INFO];

/// Descriptors for providers included in this library; custom providers register separately.
pub fn available_importers() -> &'static [&'static ImporterInfo] {
    AVAILABLE_IMPORTERS
}
