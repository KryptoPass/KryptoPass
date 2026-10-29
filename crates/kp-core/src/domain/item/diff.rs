//! What two diverging versions of an item disagree on, for a person to merge.
//!
//! A difference names what differs, never the values: the merge view reads
//! them from the three versions it already holds, so comparing secrets
//! copies none of them.

use kp_template::{FieldId, Template};
use uuid::Uuid;

use super::content::{CustomValue, ItemContent};

/// One of the versions a merge looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The common ancestor both sides started from.
    Base,
    /// This device's head.
    Ours,
    /// The version that diverged from it.
    Theirs,
}

/// Something two versions can disagree on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DiffTarget {
    Title,
    Folder,
    Favorite,
    Trashed,
    Icon,
    Field(FieldId),
    /// A field the person added, by its id.
    Custom(Uuid),
}

/// How the sides relate to their base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    /// Only this device changed it.
    OnlyOurs,
    /// Only the other side changed it.
    OnlyTheirs,
    /// Both changed it differently, or there is no base to tell.
    Conflict,
}

/// One disagreement between the two sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    pub target: DiffTarget,
    pub kind: DiffKind,
    /// Hidden values, such as passwords and keys.
    pub concealed: bool,
    /// The side a merge view selects by default. Never set for a concealed
    /// conflict: the person must look at both.
    pub suggested: Option<Side>,
}

impl Difference {
    /// Whether the person has to choose before the merge can be saved.
    pub fn needs_review(&self) -> bool {
        self.kind == DiffKind::Conflict
    }
}

/// Compares one target. `base` is `None` when the versions share no
/// history; inner `None`s are absent values. `newer` is the side whose
/// clock is later, the default pick for a visible conflict.
pub fn compare<T: PartialEq>(
    target: DiffTarget,
    base: Option<Option<&T>>,
    ours: Option<&T>,
    theirs: Option<&T>,
    concealed: bool,
    newer: Side,
) -> Option<Difference> {
    if ours == theirs {
        return None;
    }
    let (kind, suggested) = match base {
        Some(base) if base == ours => (DiffKind::OnlyTheirs, Some(Side::Theirs)),
        Some(base) if base == theirs => (DiffKind::OnlyOurs, Some(Side::Ours)),
        _ => (DiffKind::Conflict, (!concealed).then_some(newer)),
    };
    Some(Difference {
        target,
        kind,
        concealed,
        suggested,
    })
}

impl ItemContent {
    /// The fields and added fields on which `ours` and `theirs` differ, in
    /// template order and then added fields. All three must follow the same
    /// template version (open them migrated). Derived fields are left out:
    /// they follow their source.
    pub fn diff(
        base: Option<&Self>,
        ours: &Self,
        theirs: &Self,
        template: &Template,
        newer: Side,
    ) -> Vec<Difference> {
        let mut differences = Vec::new();
        for field in template
            .fields()
            .filter(|field| !field.field_type.is_derived())
        {
            let id = field.id.as_str();
            differences.extend(compare(
                DiffTarget::Field(field.id.clone()),
                base.map(|base| base.value(id)),
                ours.value(id),
                theirs.value(id),
                field.field_type.is_concealed() || field.conceal,
                newer,
            ));
        }
        let mut ids: Vec<Uuid> = Vec::new();
        for custom in ours.custom.iter().chain(&theirs.custom) {
            if !ids.contains(&custom.id) {
                ids.push(custom.id);
            }
        }
        for id in ids {
            let (left, right) = (find_custom(ours, id), find_custom(theirs, id));
            let concealed = left.into_iter().chain(right).any(|custom| custom.concealed);
            differences.extend(compare(
                DiffTarget::Custom(id),
                base.map(|base| find_custom(base, id)),
                left,
                right,
                concealed,
                newer,
            ));
        }
        differences
    }
}

fn find_custom(content: &ItemContent, id: Uuid) -> Option<&CustomValue> {
    content.custom.iter().find(|custom| custom.id == id)
}
