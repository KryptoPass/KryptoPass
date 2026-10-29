//! Bringing content written by an older template version up to date.

use std::mem;

use thiserror::Error;

use super::content::{CustomValue, ItemContent};
use super::issue::{IssueCode, ItemIssues};
use super::value::FieldValue;
use crate::SensitiveText;

use kp_template::{FieldId, MigrationStep, Template};
use uuid::Uuid;

/// Why content could not be brought to the template's version.
#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("content belongs to another template")]
    WrongTemplate,
    /// Written by a newer version of the template, for example by a more
    /// recent app through Sync. It must not be read or overwritten here.
    #[error("content is newer than the template")]
    TooNew,
    #[error("no migration from template version {from}")]
    NoPath { from: u32 },
    #[error("a renamed field collides with an existing value")]
    Conflict(FieldId),
    /// The migrations ran but the result does not fit the template: a bug in
    /// the template's migrations, reported by structure codes only.
    #[error("migrated content does not fit the template: {0}")]
    Inconsistent(ItemIssues),
}

/// Issue codes that mean the content's shape is wrong, not a value.
const STRUCTURAL: [IssueCode; 4] = [
    IssueCode::UNKNOWN_FIELD,
    IssueCode::DUPLICATE_FIELD,
    IssueCode::WRONG_SHAPE,
    IssueCode::UNKNOWN_SECTION,
];

impl ItemContent {
    /// Upgrades content to the version of `template`, step by step.
    ///
    /// Returns the version it started from when it changed anything. The
    /// result is checked for shape (known ids, single or list, sections), not
    /// for formats: values that break a newer rule are for the editor to fix.
    /// On error the content may be partly migrated and must be discarded.
    pub fn migrate(&mut self, template: &Template) -> Result<Option<u32>, MigrationError> {
        if self.template != template.id {
            return Err(MigrationError::WrongTemplate);
        }
        let start = self.template_version;
        if start == template.version {
            return Ok(None);
        }
        if start > template.version {
            return Err(MigrationError::TooNew);
        }
        while self.template_version < template.version {
            let migration =
                template
                    .migration_from(self.template_version)
                    .ok_or(MigrationError::NoPath {
                        from: self.template_version,
                    })?;
            for step in &migration.steps {
                self.apply(step)?;
            }
            self.template_version += 1;
        }
        self.check_structure(template)?;
        Ok(Some(start))
    }

    fn apply(&mut self, step: &MigrationStep) -> Result<(), MigrationError> {
        match step {
            MigrationStep::Rename { from, to } => {
                if self.value(to.as_str()).is_some() {
                    return Err(MigrationError::Conflict(to.clone()));
                }
                if let Some(entry) = self.values.iter_mut().find(|entry| entry.id == *from) {
                    entry.id = to.clone();
                }
            }
            MigrationStep::ToList { field } => {
                if let Some(FieldValue::One(text)) = self.value_mut(field.as_str()) {
                    let text = mem::take(text);
                    self.set(field.clone(), FieldValue::Many(vec![text]));
                }
            }
            MigrationStep::Retire {
                field,
                section,
                concealed,
                multiline,
            } => {
                let Some(value) = self.value_mut(field.as_str()) else {
                    return Ok(());
                };
                // Taken out, so the texts move into the custom fields instead
                // of being copied; the emptied entry is then dropped.
                let texts = match value {
                    FieldValue::One(text) => vec![mem::take(text)],
                    FieldValue::Many(list) => mem::take(list),
                };
                self.remove(field.as_str());
                for text in texts.into_iter().filter(|text| !text.is_empty()) {
                    self.custom.push(CustomValue {
                        id: Uuid::new_v4(),
                        section: section.clone(),
                        key: SensitiveText::from(field.as_str()),
                        value: text,
                        multiline: *multiline,
                        concealed: *concealed,
                    });
                }
            }
        }
        Ok(())
    }

    fn check_structure(&self, template: &Template) -> Result<(), MigrationError> {
        let Err(ItemIssues(issues)) = self.validate(template) else {
            return Ok(());
        };
        let structural: Vec<_> = issues
            .into_iter()
            .filter(|issue| STRUCTURAL.contains(&issue.code))
            .collect();
        if structural.is_empty() {
            Ok(())
        } else {
            Err(MigrationError::Inconsistent(ItemIssues(structural)))
        }
    }
}
