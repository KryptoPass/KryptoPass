//! An item being created or edited, until it is saved.

use kp_template::{Field, Multiplicity, Template};
use kryptopass_core::vault::{IconChoice, Item, ItemEdit, NewItem, Recipients};
use kryptopass_core::{
    FieldValue, FolderId, ItemContent, ItemIssue, ItemIssues, SecretId, SensitiveText,
};

use crate::error::AppError;

/// Where a draft came from, which decides how it is saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    New,
    /// An existing item as opened at revision `base`.
    Existing {
        id: SecretId,
        base: u64,
    },
}

/// The editor's working copy: title and content, checked as the person
/// types and turned into a vault write when saved.
pub struct Draft {
    origin: Origin,
    title: String,
    content: ItemContent,
    migrated_from: Option<u32>,
}

impl Draft {
    /// An empty item of `template`.
    pub fn new(template: &Template) -> Self {
        Self {
            origin: Origin::New,
            title: String::new(),
            content: ItemContent::new(template),
            migrated_from: None,
        }
    }

    /// A copy of an opened item, saved back as an edit of its revision.
    pub fn edit(item: &Item) -> Self {
        Self {
            origin: Origin::Existing {
                id: item.meta.id,
                base: item.meta.revision,
            },
            title: item.meta.state.title.clone(),
            content: item.content.clone(),
            migrated_from: item.migrated_from,
        }
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn content(&self) -> &ItemContent {
        &self.content
    }

    /// Whether saving also writes an upgrade done when the item was opened.
    pub fn migrated_from(&self) -> Option<u32> {
        self.migrated_from
    }

    /// Sets a single-value field. An empty text clears it. Derived fields
    /// are refused: the app computes them on save.
    pub fn set_text(
        &mut self,
        template: &Template,
        field: &str,
        value: SensitiveText,
    ) -> Result<(), AppError> {
        let field = editable(template, field)?;
        if field.multiplicity != Multiplicity::Single {
            return Err(AppError::WrongShape);
        }
        if value.is_empty() {
            self.content.remove(field.id.as_str());
        } else {
            self.content.set(field.id.clone(), FieldValue::One(value));
        }
        Ok(())
    }

    /// Sets every entry of a list field, in order.
    pub fn set_list(
        &mut self,
        template: &Template,
        field: &str,
        values: Vec<SensitiveText>,
    ) -> Result<(), AppError> {
        let field = editable(template, field)?;
        if field.multiplicity == Multiplicity::Single {
            return Err(AppError::WrongShape);
        }
        if values.is_empty() {
            self.content.remove(field.id.as_str());
        } else {
            self.content.set(field.id.clone(), FieldValue::Many(values));
        }
        Ok(())
    }

    /// Problems the draft would be saved with, to mark fields as the person
    /// types. It checks a normalized copy, as saving will.
    pub fn issues(&self, template: &Template) -> Vec<ItemIssue> {
        let mut normalized = self.content.clone();
        normalized.normalize(template);
        match normalized.validate(template) {
            Ok(()) => Vec::new(),
            Err(ItemIssues(issues)) => issues,
        }
    }

    /// The write that creates the item. `None` for a draft of an existing item.
    pub fn into_new(
        self,
        folder: Option<FolderId>,
        recipients: Recipients,
        icon: IconChoice,
    ) -> Option<NewItem> {
        (self.origin == Origin::New).then_some(NewItem {
            folder,
            title: self.title,
            content: self.content,
            recipients,
            icon,
            link: None,
        })
    }

    /// The write that saves an existing item's title and content, on top of
    /// which the caller may add folder, icon or flag changes.
    pub fn into_edit(self) -> Option<ItemEdit> {
        match self.origin {
            Origin::New => None,
            Origin::Existing { id, base } => Some(
                ItemEdit::new(id, base)
                    .title(self.title)
                    .content(self.content),
            ),
        }
    }
}

fn editable<'a>(template: &'a Template, field: &str) -> Result<&'a Field, AppError> {
    let field = template.field(field).ok_or(AppError::UnknownField)?;
    if field.field_type.is_derived() {
        return Err(AppError::WrongShape);
    }
    Ok(field)
}
