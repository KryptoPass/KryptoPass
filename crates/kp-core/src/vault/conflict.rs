//! Merging diverging versions of an item, with the person deciding.
//!
//! When Sync or another device writes on top of an older version, the item
//! ends up with two heads. Nothing is merged on its own: the merge view
//! shows the [`Difference`]s between this device's head and the other
//! version, the person picks a side for each (one-sided changes and the more
//! recent side come preselected; concealed conflicts never do), and
//! [`Vault::resolve_conflict`] saves the result as a version with both as
//! parents. With three or more heads, each merge joins two and the next one
//! is opened after it.

use std::collections::HashMap;

use super::error::VaultError;
use super::item::{IconChoice, Item, ItemEdit};
use super::record::{ItemMeta, ItemState};
use super::{Cipher, Store, Vault, logged};
use crate::domain::SecretId;
use crate::domain::entry_icon::EntryIcon;
use crate::domain::item::{DiffTarget, Difference, ItemContent, Side, compare};

/// The two sides of a conflict, their common ancestor and what differs.
pub struct Conflict {
    /// The common ancestor; `None` when the versions share no history.
    pub base: Option<Item>,
    /// This device's head.
    pub ours: Item,
    /// The most recent other head.
    pub theirs: Item,
    pub differences: Vec<Difference>,
}

impl Conflict {
    /// The version of `side`, if there is one.
    pub fn side(&self, side: Side) -> Option<&Item> {
        match side {
            Side::Base => self.base.as_ref(),
            Side::Ours => Some(&self.ours),
            Side::Theirs => Some(&self.theirs),
        }
    }
}

impl<S: Store, C: Cipher> Vault<S, C> {
    /// The item's conflict, or `None` when it has a single head.
    pub fn open_conflict(&self, id: SecretId) -> Result<Option<Conflict>, VaultError> {
        logged("open_conflict", self.try_open_conflict(id))
    }

    /// Saves the merge. `choices` picks a side per difference; differences
    /// left out take their suggested side, and a conflict without a choice
    /// or suggestion refuses the save. The base revision is the one the
    /// conflict was opened at, so a change since then makes it stale.
    pub fn resolve_conflict(
        &mut self,
        conflict: &Conflict,
        choices: &HashMap<DiffTarget, Side>,
    ) -> Result<ItemMeta, VaultError> {
        logged(
            "resolve_conflict",
            self.try_resolve_conflict(conflict, choices),
        )
    }

    fn try_open_conflict(&self, id: SecretId) -> Result<Option<Conflict>, VaultError> {
        let meta = self.store.item_meta(id)?.ok_or(VaultError::ItemNotFound)?;
        let heads = self.store.heads(id)?;
        let Some(ours_clock) = heads
            .iter()
            .find(|head| head.id == meta.head)
            .map(|head| head.clock)
        else {
            return Err(VaultError::CorruptContent);
        };
        let Some(theirs) = heads
            .iter()
            .filter(|head| head.id != meta.head)
            .max_by_key(|head| (head.clock, head.id))
        else {
            return Ok(None);
        };
        let newer = if theirs.clock > ours_clock {
            Side::Theirs
        } else {
            Side::Ours
        };
        let base = self
            .store
            .merge_base(id, meta.head, theirs.id)?
            .map(|base| self.open_version(id, base))
            .transpose()?;
        let ours = self.open_version(id, meta.head)?;
        let theirs = self.open_version(id, theirs.id)?;
        let template = self.templates.template(ours.meta.state.template.as_str())?;

        let mut differences = state_differences(
            base.as_ref().map(|item| &item.meta.state),
            &ours.meta.state,
            &theirs.meta.state,
            newer,
        );
        differences.extend(ItemContent::diff(
            base.as_ref().map(|item| &item.content),
            &ours.content,
            &theirs.content,
            template,
            newer,
        ));
        tracing::debug!(
            item = %id.0,
            differences = differences.len(),
            review = differences.iter().filter(|d| d.needs_review()).count(),
            "item conflict opened"
        );
        Ok(Some(Conflict {
            base,
            ours,
            theirs,
            differences,
        }))
    }

    fn try_resolve_conflict(
        &mut self,
        conflict: &Conflict,
        choices: &HashMap<DiffTarget, Side>,
    ) -> Result<ItemMeta, VaultError> {
        let ours = &conflict.ours;
        let mut content = ours.content.clone();
        let mut edit =
            ItemEdit::new(ours.meta.id, ours.meta.revision).merge_with(conflict.theirs.meta.head);
        for difference in &conflict.differences {
            let side = choices
                .get(&difference.target)
                .copied()
                .or(difference.suggested)
                .ok_or(VaultError::ConflictUnresolved)?;
            if side == Side::Ours {
                continue;
            }
            // A base that does not exist means the value was absent there.
            let chosen = conflict.side(side);
            let state = chosen.map(|item| &item.meta.state);
            edit = match &difference.target {
                DiffTarget::Title => match state {
                    Some(state) => edit.title(state.title.clone()),
                    None => edit,
                },
                DiffTarget::Folder => edit.folder(state.and_then(|state| state.folder)),
                DiffTarget::Favorite => edit.favorite(state.is_some_and(|state| state.favorite)),
                DiffTarget::Trashed => edit.trashed(state.is_some_and(|state| state.trashed)),
                DiffTarget::Icon => {
                    edit.icon(self.icon_choice(state.and_then(|state| state.icon.as_ref()))?)
                }
                DiffTarget::Field(field) => {
                    match chosen.and_then(|item| item.content.value(field.as_str())) {
                        Some(value) => content.set(field.clone(), value.clone()),
                        None => {
                            content.remove(field.as_str());
                        }
                    }
                    edit
                }
                DiffTarget::Custom(id) => {
                    let value = chosen.and_then(|item| {
                        item.content
                            .custom()
                            .iter()
                            .find(|custom| custom.id() == *id)
                    });
                    match (content.custom_mut(*id), value) {
                        (Some(slot), Some(value)) => *slot = value.clone(),
                        (None, Some(value)) => content.add_custom(value.clone()),
                        (_, None) => {
                            content.remove_custom(*id);
                        }
                    }
                    edit
                }
            };
        }
        let meta = self.update_item(edit.content(content))?;
        tracing::info!(item = %meta.id.0, revision = meta.revision, "item conflict resolved");
        Ok(meta)
    }

    /// The icon choice that keeps `icon` as it is.
    fn icon_choice(&self, icon: Option<&EntryIcon>) -> Result<IconChoice, VaultError> {
        Ok(match icon {
            Some(EntryIcon::Theme { name }) => IconChoice::Theme(name.clone()),
            Some(EntryIcon::Image { asset_id }) => match self.store.icon_asset(*asset_id)? {
                Some(asset) => IconChoice::Image(asset),
                None => IconChoice::Automatic,
            },
            Some(EntryIcon::Favicon { .. }) | None => IconChoice::Automatic,
        })
    }
}

/// Differences in title, folder, flags and icon.
fn state_differences(
    base: Option<&ItemState>,
    ours: &ItemState,
    theirs: &ItemState,
    newer: Side,
) -> Vec<Difference> {
    let mut differences = Vec::new();
    differences.extend(compare(
        DiffTarget::Title,
        base.map(|state| Some(&state.title)),
        Some(&ours.title),
        Some(&theirs.title),
        false,
        newer,
    ));
    differences.extend(compare(
        DiffTarget::Folder,
        base.map(|state| state.folder.as_ref()),
        ours.folder.as_ref(),
        theirs.folder.as_ref(),
        false,
        newer,
    ));
    differences.extend(compare(
        DiffTarget::Favorite,
        base.map(|state| Some(&state.favorite)),
        Some(&ours.favorite),
        Some(&theirs.favorite),
        false,
        newer,
    ));
    differences.extend(compare(
        DiffTarget::Trashed,
        base.map(|state| Some(&state.trashed)),
        Some(&ours.trashed),
        Some(&theirs.trashed),
        false,
        newer,
    ));
    // An automatic icon follows the content, so only icons the person chose
    // (theme or picture) can conflict.
    let chosen = |state: &ItemState| {
        state
            .icon
            .clone()
            .filter(|icon| !matches!(icon, EntryIcon::Favicon { .. }))
    };
    let (base_icon, ours_icon, theirs_icon) = (base.map(chosen), chosen(ours), chosen(theirs));
    differences.extend(compare(
        DiffTarget::Icon,
        base_icon.as_ref().map(Option::as_ref),
        ours_icon.as_ref(),
        theirs_icon.as_ref(),
        false,
        newer,
    ));
    differences
}
