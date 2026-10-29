//! Names of the XML elements and attributes of the template format.
//!
//! This is the only place the parser's vocabulary is spelled out. Each element
//! has its own attribute enum, so the attributes it accepts come from `ALL`
//! instead of a list kept by hand next to the parsing code.
//!
//! One enum per element, and not one big `Attr` enum, because the allowed set
//! is "all variants of this enum". A shared enum would need a separate list per
//! element again, which is the duplication this module exists to remove.

use crate::model::FieldType;
use crate::model::vocabulary::vocabulary;

// `xml:lang` is spelled twice because two different APIs need it: errors show the
// prefixed form people write, while the namespace-aware lookup takes only the
// local name plus the namespace URI.
/// Name of the language attribute, which belongs to the XML namespace.
pub(super) const XML_LANG: &str = "xml:lang";
/// Local name of the language attribute inside the XML namespace.
pub(super) const XML_LANG_LOCAL: &str = "lang";
/// Placeholder used in errors for the text content of an element.
pub(super) const TEXT_CONTENT: &str = "text";
/// Kinds of item that can be repeated, for duplicate-id errors without an element.
pub(super) const MESSAGE: &str = "message";
pub(super) const LANGUAGE: &str = "language";
/// Values of an XML boolean attribute.
pub(super) const TRUE: &str = "true";
pub(super) const FALSE: &str = "false";

vocabulary! {
    /// Every element the format knows.
    pub(super) enum Tag {
        Template => "template",
        Meta => "meta",
        Name => "name",
        Icon => "icon",
        SearchTitle => "search-title",
        Summary => "summary",
        Fields => "fields",
        Section => "section",
        Field => "field",
        CustomFields => "custom-fields",
        Label => "label",
        Limit => "limit",
        Generator => "generator",
        Prop => "prop",
        Actions => "actions",
        Action => "action",
        Checks => "checks",
        Check => "check",
        Behavior => "behavior",
        Versions => "versions",
        Events => "events",
        Migrations => "migrations",
        Migration => "migration",
        Rename => "rename",
        ToList => "to-list",
        Retire => "retire",
        Catalog => "catalog",
        Text => "text",
        Generators => "generators",
        Param => "param",
        Class => "class",
    }
}

vocabulary! {
    /// Attributes of `<template>`.
    pub(super) enum TemplateAttr {
        Id => "id",
        Version => "version",
        Kind => "kind",
    }
}

vocabulary! {
    /// Attributes of `<field>`.
    pub(super) enum FieldAttr {
        Id => "id",
        Type => "type",
        Required => "required",
        Copy => "copy",
        Conceal => "conceal",
        Autocomplete => "autocomplete",
        Search => "search",
        History => "history",
        Multiple => "multiple",
        Max => "max",
        Format => "format",
        Precision => "precision",
        BrowserMatch => "browser-match",
        MinLength => "min-length",
        MaxLength => "max-length",
    }
}

impl FieldAttr {
    /// The only field type that accepts the attribute, if it is type-specific.
    ///
    /// Kept beside the names so a new type-specific attribute is declared in one
    /// place and the validation in `template.rs` picks it up without changes.
    pub(super) const fn applies_to(self) -> Option<FieldType> {
        match self {
            Self::Format => Some(FieldType::Multiline),
            Self::Precision => Some(FieldType::Date),
            Self::BrowserMatch => Some(FieldType::Url),
            Self::MinLength | Self::MaxLength => Some(FieldType::Digits),
            _ => None,
        }
    }
}

vocabulary! {
    /// The `id` attribute, on `<section>`, `<generator>` and catalog `<text>`.
    pub(super) enum IdAttr {
        Id => "id",
    }
}

vocabulary! {
    /// Attributes of `<custom-fields>`.
    pub(super) enum CustomFieldsAttr {
        Allowed => "allowed",
        Max => "max",
    }
}

vocabulary! {
    /// Attributes of `<icon>`.
    pub(super) enum IconAttr {
        Name => "name",
    }
}

vocabulary! {
    /// Attributes of `<search-title>` and `<summary>`.
    pub(super) enum FieldRefAttr {
        Field => "field",
    }
}

vocabulary! {
    /// The `ref` attribute, on `<label>`, `<name>` and `<generator>`.
    pub(super) enum RefAttr {
        Ref => "ref",
    }
}

vocabulary! {
    /// Attributes of `<limit>`.
    pub(super) enum LimitAttr {
        MaxBytes => "max-bytes",
    }
}

/// Property that lists the values of a `choice` field, comma-separated.
pub(super) const OPTIONS_PROP: &str = "options";

vocabulary! {
    /// Attributes of `<prop>`.
    pub(super) enum PropAttr {
        Name => "name",
        Value => "value",
    }
}

vocabulary! {
    /// Attributes of `<action>`.
    pub(super) enum ActionAttr {
        Ref => "ref",
        Format => "format",
        Fields => "fields",
    }
}

vocabulary! {
    /// Attributes of `<check>`.
    pub(super) enum CheckAttr {
        Ref => "ref",
        Fields => "fields",
    }
}

vocabulary! {
    /// Attributes of `<versions>`.
    pub(super) enum VersionsAttr {
        Max => "max",
    }
}

vocabulary! {
    /// Attributes of `<events>`.
    pub(super) enum EventsAttr {
        Track => "track",
    }
}

vocabulary! {
    /// Attributes of `<migration>`.
    pub(super) enum MigrationAttr {
        From => "from",
    }
}

vocabulary! {
    /// Attributes of `<rename>`.
    pub(super) enum RenameAttr {
        Field => "field",
        To => "to",
    }
}

vocabulary! {
    /// Attributes of `<retire>`.
    pub(super) enum RetireAttr {
        Field => "field",
        Section => "section",
        Conceal => "conceal",
        Multiline => "multiline",
    }
}

vocabulary! {
    /// Attributes of `<catalog>`, besides `xml:lang`.
    pub(super) enum CatalogAttr {
        Version => "version",
    }
}

vocabulary! {
    /// Attributes of `<param>`.
    pub(super) enum ParamAttr {
        Name => "name",
        Default => "default",
        Min => "min",
        Max => "max",
    }
}

vocabulary! {
    /// Attributes of `<class>`.
    pub(super) enum ClassAttr {
        Id => "id",
        Default => "default",
    }
}
