//! The stored envelope: version, size limit and typed failures.
//!
//! The envelope is the only place that knows `schemaVersion`; content
//! carries what the template needs and nothing about storage.
//!
//! serde_json quotes the offending value in some messages (`invalid type:
//! string "…"`), which would put plaintext in logs. Failures here keep only
//! the error category and position.

use thiserror::Error;

use super::content::{CustomValue, ITEM_SCHEMA_VERSION, ItemContent, ItemValue};
use super::format::FormatRules;
use super::issue::ItemIssues;
use crate::SensitiveBytes;
use crate::util::MAX_CONTENT_BYTES;

use kp_template::{Template, TemplateId};
use serde::{Deserialize, Serialize};
use serde_json::error::Category;

/// Why a payload could not be written or read.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("item content exceeds the maximum size")]
    TooLarge,
    #[error("unsupported item schema version")]
    UnsupportedVersion,
    #[error("item content is malformed ({category} at line {line}, column {column})")]
    Malformed {
        category: &'static str,
        line: usize,
        column: usize,
    },
    #[error(transparent)]
    Invalid(#[from] ItemIssues),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvelopeRef<'a> {
    schema_version: u32,
    template: &'a TemplateId,
    template_version: u32,
    values: &'a [ItemValue],
    custom: &'a [CustomValue],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    template: TemplateId,
    template_version: u32,
    values: Vec<ItemValue>,
    #[serde(default)]
    custom: Vec<CustomValue>,
}

/// Validates with the standard rules and encodes. The plaintext buffer is
/// zeroized on drop.
pub fn encode_item(
    content: &ItemContent,
    template: &Template,
) -> Result<SensitiveBytes, CodecError> {
    encode_item_with(content, template, FormatRules::shared())
}

/// Validates with `rules` and encodes.
pub fn encode_item_with(
    content: &ItemContent,
    template: &Template,
    rules: &FormatRules,
) -> Result<SensitiveBytes, CodecError> {
    content.validate_with(template, rules)?;
    let envelope = EnvelopeRef {
        schema_version: ITEM_SCHEMA_VERSION,
        template: &content.template,
        template_version: content.template_version,
        values: &content.values,
        custom: &content.custom,
    };
    let mut bytes = SensitiveBytes::new();
    // The only writer failure is the size bound.
    serde_json::to_writer(bytes.bounded_writer(MAX_CONTENT_BYTES), &envelope)
        .map_err(|_| CodecError::TooLarge)?;
    if bytes.len() > MAX_CONTENT_BYTES {
        return Err(CodecError::TooLarge);
    }
    Ok(bytes)
}

/// Reads a payload without accepting unknown fields.
///
/// It checks the envelope only. The caller resolves the template the content
/// names, migrates it and validates before trusting the values.
///
/// This is the only supported reader: values are decoded from the borrowed
/// input without leaving copies in parser buffers, which other serde readers
/// cannot do and therefore refuse.
pub fn decode_item(bytes: &[u8]) -> Result<ItemContent, CodecError> {
    if bytes.len() > MAX_CONTENT_BYTES {
        return Err(CodecError::TooLarge);
    }
    let Envelope {
        schema_version,
        template,
        template_version,
        values,
        custom,
    } = serde_json::from_slice(bytes).map_err(malformed)?;
    if schema_version != ITEM_SCHEMA_VERSION {
        return Err(CodecError::UnsupportedVersion);
    }
    Ok(ItemContent {
        template,
        template_version,
        values,
        custom,
    })
}

fn malformed(error: serde_json::Error) -> CodecError {
    CodecError::Malformed {
        category: match error.classify() {
            Category::Io => "io",
            Category::Syntax => "syntax",
            Category::Data => "data",
            Category::Eof => "eof",
        },
        line: error.line(),
        column: error.column(),
    }
}
