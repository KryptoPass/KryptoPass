//! Checks of a payload against the template that produced it.

use std::collections::HashSet;

use super::content::{CustomValue, ItemContent};
use super::format::FormatRules;
use super::issue::{IssueCode, IssueTarget, ItemIssue, ItemIssues};
use super::ssh::{DerivedKey, private_key_field};
use super::value::FieldValue;

use crate::util::has_control_characters;

use kp_template::{Field, Multiplicity, Template};

const MAX_CUSTOM_KEY_BYTES: usize = 512;
const MAX_CUSTOM_VALUE_BYTES: usize = 256 * 1024;

impl ItemContent {
    /// Checks the payload with the standard format rules.
    pub fn validate(&self, template: &Template) -> Result<(), ItemIssues> {
        self.validate_with(template, FormatRules::shared())
    }

    /// Checks the payload against the template that produced it.
    ///
    /// Reports unknown or repeated ids, a list on a single-value field and the
    /// reverse, values over the field limits, control characters, values that
    /// break `rules`, missing required values and custom fields the template
    /// does not allow. Every problem is collected. Content of another
    /// template or version is reported alone, since its fields mean nothing
    /// against this one; migrate it first.
    pub fn validate_with(
        &self,
        template: &Template,
        rules: &FormatRules,
    ) -> Result<(), ItemIssues> {
        if self.template != template.id || self.template_version != template.version {
            return Err(ItemIssues(vec![ItemIssue::new(
                IssueCode::TEMPLATE_MISMATCH,
                IssueTarget::Item,
            )]));
        }
        let mut issues = Vec::new();

        let mut seen = HashSet::with_capacity(self.values.len());
        for (index, entry) in self.values.iter().enumerate() {
            match template.field(entry.id.as_str()) {
                None => issues.push(ItemIssue::new(
                    IssueCode::UNKNOWN_FIELD,
                    IssueTarget::Value(index),
                )),
                Some(_) if !seen.insert(entry.id.as_str()) => issues.push(ItemIssue::new(
                    IssueCode::DUPLICATE_FIELD,
                    IssueTarget::Value(index),
                )),
                Some(field) => check_value(field, &entry.value, rules, &mut issues),
            }
        }
        for field in template.fields().filter(|field| field.required) {
            let filled = self
                .value(field.id.as_str())
                .is_some_and(FieldValue::has_content);
            if !filled {
                issues.push(ItemIssue::new(
                    IssueCode::REQUIRED,
                    IssueTarget::Field(field.id.clone()),
                ));
            }
        }
        check_custom(&self.custom, template, &mut issues);
        self.check_derived(template, &mut issues);
        self.run_checks(template, &mut issues);

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ItemIssues(issues))
        }
    }
}

impl ItemContent {
    /// Derived fields must hold exactly what their source produces, so a
    /// payload from elsewhere cannot show a public key that is not the key's.
    fn check_derived(&self, template: &Template, issues: &mut Vec<ItemIssue>) {
        let Some(source) = private_key_field(template) else {
            return;
        };
        let key = self.value(source).and_then(FieldValue::first);
        let derived = key.and_then(|key| DerivedKey::from_private(key));
        if key.is_some() && derived.is_none() {
            // The source itself is reported by its format rule.
            return;
        }
        for field in template
            .fields()
            .filter(|field| field.field_type.is_derived())
        {
            let stored = self.value(field.id.as_str()).and_then(FieldValue::first);
            let expected = derived
                .as_ref()
                .and_then(|key| key.value_for(field.field_type));
            if stored.map(|text| text.as_str()) != expected {
                issues.push(ItemIssue::new(
                    IssueCode::DERIVED_MISMATCH,
                    IssueTarget::Field(field.id.clone()),
                ));
            }
        }
    }
}

fn check_value(
    field: &Field,
    value: &FieldValue,
    rules: &FormatRules,
    issues: &mut Vec<ItemIssue>,
) {
    let target = || IssueTarget::Field(field.id.clone());
    match (field.multiplicity, value) {
        (Multiplicity::Single, FieldValue::One(_)) => {}
        (Multiplicity::Many { max }, FieldValue::Many(list)) => {
            if list.len() > usize::from(max) {
                issues.push(
                    ItemIssue::new(IssueCode::TOO_MANY, target()).with_limit(usize::from(max)),
                );
            }
        }
        _ => {
            issues.push(ItemIssue::new(IssueCode::WRONG_SHAPE, target()));
            return;
        }
    }
    let limit = field.max_bytes.map(|bytes| bytes as usize);
    let multiline = field.field_type.is_multiline();
    for text in value.texts() {
        let code = if limit.is_some_and(|limit| text.len() > limit) {
            Some(IssueCode::TOO_LONG)
        } else if has_control_characters(text, multiline) {
            Some(IssueCode::INVALID_CHARACTERS)
        } else if text.is_empty() {
            None
        } else {
            rules.check(field, text).err()
        };
        if let Some(code) = code {
            let issue = ItemIssue::new(code, target());
            issues.push(match limit {
                Some(limit) if code == IssueCode::TOO_LONG => issue.with_limit(limit),
                _ => issue,
            });
        }
    }
}

fn check_custom(custom: &[CustomValue], template: &Template, issues: &mut Vec<ItemIssue>) {
    let mut ids = HashSet::with_capacity(custom.len());
    for (index, entry) in custom.iter().enumerate() {
        let target = || IssueTarget::Custom(index);
        if entry.id.is_nil() {
            issues.push(ItemIssue::new(IssueCode::INVALID_CUSTOM_ID, target()));
        } else if !ids.insert(entry.id) {
            issues.push(ItemIssue::new(IssueCode::DUPLICATE_CUSTOM_ID, target()));
        }
        if template.section(entry.section.as_str()).is_none() {
            issues.push(ItemIssue::new(IssueCode::UNKNOWN_SECTION, target()));
        }
        if entry.key.is_empty()
            || entry.key.len() > MAX_CUSTOM_KEY_BYTES
            || has_control_characters(&entry.key, false)
        {
            issues.push(
                ItemIssue::new(IssueCode::INVALID_CUSTOM_KEY, target())
                    .with_limit(MAX_CUSTOM_KEY_BYTES),
            );
        }
        if entry.value.len() > MAX_CUSTOM_VALUE_BYTES {
            issues.push(
                ItemIssue::new(IssueCode::TOO_LONG, target()).with_limit(MAX_CUSTOM_VALUE_BYTES),
            );
        } else if has_control_characters(&entry.value, entry.multiline) {
            issues.push(ItemIssue::new(IssueCode::INVALID_CHARACTERS, target()));
        }
    }
    for section in &template.sections {
        let count = custom
            .iter()
            .filter(|entry| entry.section == section.id)
            .count();
        let allowed = section
            .custom_fields
            .map_or(0, |limit| usize::from(limit.max));
        if count > allowed {
            issues.push(
                ItemIssue::new(
                    IssueCode::TOO_MANY,
                    IssueTarget::Section(section.id.clone()),
                )
                .with_limit(allowed),
            );
        }
    }
}
