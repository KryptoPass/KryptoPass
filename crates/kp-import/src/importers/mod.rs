pub mod proton;

use crate::ImporterInfo;

/// Descriptors for providers included in this library; custom providers register separately.
pub fn available_importers() -> &'static [&'static ImporterInfo] {
    &[&proton::PROTON_PASS_INFO]
}
