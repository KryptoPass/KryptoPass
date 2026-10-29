//! Which icon an item gets after a write.

use kp_template::Template;

use super::error::VaultError;
use super::item::IconChoice;
use crate::domain::entry_icon::{EntryIcon, EntryIconAsset, automatic_icon_for_url};
use crate::domain::item::ItemContent;

/// The icon to store and, for a new picture, its image.
///
/// With no choice an icon the person picked (theme or picture) stays; an
/// automatic one is derived again when the content changed. `automatic` is
/// only called when needed, since without new content it means decrypting.
pub(super) fn resolve(
    choice: Option<IconChoice>,
    current: Option<&EntryIcon>,
    content_changed: bool,
    automatic: impl FnOnce() -> Result<Option<EntryIcon>, VaultError>,
) -> Result<(Option<EntryIcon>, Option<EntryIconAsset>), VaultError> {
    match choice {
        Some(IconChoice::Automatic) => Ok((automatic()?, None)),
        Some(IconChoice::Theme(name)) => {
            let icon = EntryIcon::Theme { name };
            icon.validate().map_err(|_| VaultError::InvalidIcon)?;
            Ok((Some(icon), None))
        }
        Some(IconChoice::Image(asset)) => {
            asset.validate().map_err(|_| VaultError::InvalidIcon)?;
            let icon = EntryIcon::Image { asset_id: asset.id };
            Ok((Some(icon), Some(asset)))
        }
        None => match current {
            Some(icon @ (EntryIcon::Theme { .. } | EntryIcon::Image { .. })) => {
                Ok((Some(icon.clone()), None))
            }
            _ if content_changed => Ok((automatic()?, None)),
            _ => Ok((current.cloned(), None)),
        },
    }
}

/// Icon from the first website of the first field the browser may match.
pub(super) fn automatic(content: &ItemContent, template: &Template) -> Option<EntryIcon> {
    template
        .fields()
        .filter(|field| field.browser_match)
        .find_map(|field| content.value(field.id.as_str())?.first())
        .and_then(|url| automatic_icon_for_url(url))
}
