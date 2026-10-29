//! Template documents.

use super::error::{ParseError, ParseErrorKind};
use super::ids;
use super::reader::{El, open};
use super::vocab::{
    ActionAttr, CheckAttr, CustomFieldsAttr, EventsAttr, FieldAttr, FieldRefAttr, IconAttr, IdAttr,
    LANGUAGE, LimitAttr, MigrationAttr, OPTIONS_PROP, PropAttr, RefAttr, RenameAttr, RetireAttr,
    Tag, TemplateAttr, VersionsAttr, XML_LANG,
};
use crate::model::{
    Action, ActionKind, Behavior, Check, CheckKind, CustomFields, DigitCount, EventKind, Field,
    FieldId, FieldType, GeneratorId, GeneratorParam, GeneratorRef, ITEM_TITLE, IconName,
    LocalizedLiteral, MessageId, MessageRef, MessageScope, Meta, Migration, MigrationStep,
    Multiplicity, Prop, Section, SectionId, Template, TemplateId, TemplateKind, Text,
    USER_ID_PREFIX,
};
use std::collections::HashSet;

/// Longest accepted value of a `<kp:prop>`, in bytes.
const MAX_PROP_BYTES: usize = 256;

/// What the whole document knows before its parts are read.
///
/// `Copy` and passed by value so each helper can read the kind without the
/// parser threading a reference through every call.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    kind: TemplateKind,
    template_id: &'a str,
}

pub(super) fn parse(xml: &str) -> Result<Template, ParseError> {
    let doc = open(xml)?;
    let root = El::root(&doc, Tag::Template)?;
    root.check_attrs(TemplateAttr::ALL)?;
    root.check_children(&[
        Tag::Meta,
        Tag::Fields,
        Tag::Actions,
        Tag::Checks,
        Tag::Behavior,
        Tag::Migrations,
    ])?;

    let kind: TemplateKind = root.require_vocab(TemplateAttr::Kind)?;
    let raw_id = ids::id_attr(&root, TemplateAttr::Id, true)?;
    // `kind` and the id prefix must agree in both directions: a built-in id can
    // never start with `user.` and a user id always does, so a user template
    // cannot take the id of a built-in one and replace it.
    if raw_id.starts_with(USER_ID_PREFIX) != (kind == TemplateKind::User) {
        return Err(root.invalid(TemplateAttr::Id));
    }
    let version: u32 = root.require_number(TemplateAttr::Version)?;
    if version == 0 {
        return Err(root.invalid(TemplateAttr::Version));
    }
    let ctx = Ctx {
        kind,
        template_id: &raw_id,
    };

    // Fields are read before `meta` and `actions` on purpose: `summary`,
    // `search-title` and action fields point at field ids, and checking them
    // while parsing keeps the error on the line of the bad reference instead of
    // reporting it later without a position.
    let (sections, known) = sections(&root.one(Tag::Fields)?, ctx)?;
    let meta = meta(&root.one(Tag::Meta)?, ctx, &known)?;
    let actions = actions(root.optional(Tag::Actions)?.as_ref(), &known)?;
    let checks = checks(root.optional(Tag::Checks)?.as_ref(), &known)?;
    let behavior = root
        .optional(Tag::Behavior)?
        .map(|el| behavior(&el))
        .transpose()?;
    let migrations = match root.optional(Tag::Migrations)? {
        Some(el) => migrations(&el, version, &sections)?,
        None => Vec::new(),
    };

    Ok(Template {
        id: TemplateId::from_validated(raw_id.clone()),
        version,
        kind,
        meta,
        sections,
        actions,
        checks,
        behavior,
        migrations,
    })
}

/// Display text from the child elements named `tag` of `parent`.
///
/// Built-in templates have one empty element that points into the catalog,
/// by default at `default_id`. User templates carry literals, one per language.
fn text(
    parent: &El<'_, '_>,
    tag: Tag,
    ctx: Ctx<'_>,
    scope: MessageScope,
    default_id: &str,
    allow_ref: bool,
) -> Result<Text, ParseError> {
    let found = parent.all(tag)?;
    let Some(first) = found.first() else {
        return Err(parent.error(ParseErrorKind::MissingElement {
            parent: parent.name().to_owned(),
            name: tag.name(),
        }));
    };
    match ctx.kind {
        TemplateKind::Builtin => {
            if let Some(extra) = found.get(1) {
                return Err(extra.error(ParseErrorKind::DuplicateElement(tag.name().to_owned())));
            }
            if allow_ref {
                first.expect_empty(RefAttr::ALL)?;
            } else {
                first.expect_bare()?;
            }
            let id = match first.attr(RefAttr::Ref) {
                Some(raw) => ids::checked(first, raw, RefAttr::Ref)?,
                None => default_id.to_owned(),
            };
            Ok(Text::Key(MessageRef {
                scope,
                id: MessageId::from_validated(id),
            }))
        }
        TemplateKind::User => {
            let mut seen = HashSet::new();
            let mut literals = Vec::new();
            for child in &found {
                child.no_attrs()?;
                let raw = child.lang().ok_or_else(|| {
                    child.error(ParseErrorKind::MissingAttribute {
                        element: tag.name().to_owned(),
                        name: XML_LANG,
                    })
                })?;
                let language = ids::language(child, raw)?;
                if !seen.insert(language.clone()) {
                    return Err(child.error(ParseErrorKind::DuplicateId {
                        what: LANGUAGE,
                        id: language,
                    }));
                }
                literals.push(LocalizedLiteral {
                    language,
                    text: child.text()?,
                });
            }
            Ok(Text::Literal(literals))
        }
    }
}

fn meta(el: &El<'_, '_>, ctx: Ctx<'_>, known: &HashSet<String>) -> Result<Meta, ParseError> {
    el.no_attrs()?;
    el.check_children(&[Tag::Name, Tag::Icon, Tag::SearchTitle, Tag::Summary])?;
    let name = text(
        el,
        Tag::Name,
        ctx,
        MessageScope::Template,
        ctx.template_id,
        false,
    )?;

    let icon = el.one(Tag::Icon)?;
    icon.expect_empty(IconAttr::ALL)?;
    let icon = IconName::from_validated(ids::checked(
        &icon,
        icon.require(IconAttr::Name)?,
        IconAttr::Name,
    )?);

    let search_title = el.one(Tag::SearchTitle)?;
    search_title.expect_empty(FieldRefAttr::ALL)?;
    let search_title = field_ref(&search_title, true, known)?;

    let summary = match el.optional(Tag::Summary)? {
        Some(summary) => {
            summary.expect_empty(FieldRefAttr::ALL)?;
            Some(field_ref(&summary, false, known)?)
        }
        None => None,
    };
    Ok(Meta {
        name,
        icon,
        search_title,
        summary,
    })
}

/// Reads the `field` attribute as a reference to a field of this template.
/// The item title is also accepted when `allow_title` is set.
fn field_ref(
    el: &El<'_, '_>,
    allow_title: bool,
    known: &HashSet<String>,
) -> Result<FieldId, ParseError> {
    let raw = el.require(FieldRefAttr::Field)?;
    if (allow_title && raw == ITEM_TITLE) || known.contains(raw) {
        Ok(FieldId::from_validated(raw.to_owned()))
    } else {
        Err(el.error(ParseErrorKind::UnknownField(raw.to_owned())))
    }
}

fn sections(el: &El<'_, '_>, ctx: Ctx<'_>) -> Result<(Vec<Section>, HashSet<String>), ParseError> {
    el.no_attrs()?;
    el.check_children(&[Tag::Section])?;
    let mut sections = Vec::new();
    let mut section_ids = HashSet::new();
    let mut field_ids = HashSet::new();
    for child in el.children()? {
        let section = section(&child, ctx, &mut field_ids)?;
        if !section_ids.insert(section.id.as_str().to_owned()) {
            return Err(child.error(ParseErrorKind::DuplicateId {
                what: Tag::Section.name(),
                id: section.id.as_str().to_owned(),
            }));
        }
        sections.push(section);
    }
    if sections.is_empty() {
        return Err(el.error(ParseErrorKind::MissingElement {
            parent: el.name().to_owned(),
            name: Tag::Section.name(),
        }));
    }
    check_derived(el, &sections)?;
    Ok((sections, field_ids))
}

fn section(
    el: &El<'_, '_>,
    ctx: Ctx<'_>,
    field_ids: &mut HashSet<String>,
) -> Result<Section, ParseError> {
    el.check_attrs(IdAttr::ALL)?;
    el.check_children(&[Tag::Field, Tag::CustomFields])?;
    let id = ids::id_attr(el, IdAttr::Id, false)?;
    let mut fields = Vec::new();
    for child in el.all(Tag::Field)? {
        let field = field(&child, ctx)?;
        if !field_ids.insert(field.id.as_str().to_owned()) {
            return Err(child.error(ParseErrorKind::DuplicateId {
                what: Tag::Field.name(),
                id: field.id.as_str().to_owned(),
            }));
        }
        fields.push(field);
    }
    // `optional` yields an `Option<El>` and `custom_fields_of` an `Option<..>`
    // (absent when `allowed="false"`), so the result is nested; `flatten` joins
    // "no element" and "element that disallows" into the same `None`.
    let custom_fields = el
        .optional(Tag::CustomFields)?
        .map(|c| custom_fields_of(&c))
        .transpose()?;
    Ok(Section {
        id: SectionId::from_validated(id),
        fields,
        custom_fields: custom_fields.flatten(),
    })
}

/// Derived fields need exactly one field to derive from, and each derived
/// type appears once, so there is no doubt which value goes where.
fn check_derived(el: &El<'_, '_>, sections: &[Section]) -> Result<(), ParseError> {
    let count = |kind: FieldType| {
        sections
            .iter()
            .flat_map(|section| &section.fields)
            .filter(|field| field.field_type == kind)
            .count()
    };
    let sources = count(FieldType::SshPrivateKey);
    let derived = [FieldType::SshPublicKey, FieldType::SshFingerprint].map(count);
    if sources > 1 {
        return Err(el.error(ParseErrorKind::Inconsistent(
            "more than one SSH private key",
        )));
    }
    if derived.iter().any(|n| *n > 1) {
        return Err(el.error(ParseErrorKind::Inconsistent(
            "a derived SSH field appears twice",
        )));
    }
    if sources == 0 && derived.iter().any(|n| *n > 0) {
        return Err(el.error(ParseErrorKind::Inconsistent(
            "a derived SSH field without a private key",
        )));
    }
    Ok(())
}

fn custom_fields_of(el: &El<'_, '_>) -> Result<Option<CustomFields>, ParseError> {
    el.expect_empty(CustomFieldsAttr::ALL)?;
    el.require(CustomFieldsAttr::Allowed)?;
    if el.flag(CustomFieldsAttr::Allowed)? == Some(true) {
        Ok(Some(CustomFields {
            max: el.require_number(CustomFieldsAttr::Max)?,
        }))
    } else {
        Ok(None)
    }
}

fn field(el: &El<'_, '_>, ctx: Ctx<'_>) -> Result<Field, ParseError> {
    el.check_attrs(FieldAttr::ALL)?;
    el.check_children(&[Tag::Label, Tag::Limit, Tag::Generator, Tag::Prop])?;
    let id = ids::id_attr(el, FieldAttr::Id, false)?;
    let field_type: FieldType = el.require_vocab(FieldAttr::Type)?;
    check_applicable(el, field_type)?;
    let label = text(el, Tag::Label, ctx, MessageScope::Label, &id, true)?;

    let multiplicity = match (
        el.flag(FieldAttr::Multiple)?.unwrap_or(false),
        el.number::<u16>(FieldAttr::Max)?,
    ) {
        (true, Some(max)) if max > 0 => Multiplicity::Many { max },
        (false, None) => Multiplicity::Single,
        _ => return Err(el.invalid(FieldAttr::Max)),
    };
    let max_bytes = match el.optional(Tag::Limit)? {
        Some(limit) => {
            limit.expect_empty(LimitAttr::ALL)?;
            let bytes: u32 = limit.require_number(LimitAttr::MaxBytes)?;
            if bytes == 0 {
                return Err(limit.invalid(LimitAttr::MaxBytes));
            }
            Some(bytes)
        }
        None => None,
    };
    let generator = el
        .optional(Tag::Generator)?
        .map(|g| generator_ref(&g))
        .transpose()?;
    let props = el
        .all(Tag::Prop)?
        .iter()
        .map(prop)
        .collect::<Result<Vec<_>, _>>()?;
    let choices = if field_type == FieldType::Choice {
        choices(el, &props)?
    } else {
        Vec::new()
    };
    let digits = if field_type == FieldType::Digits {
        Some(digit_count(el)?)
    } else {
        None
    };

    Ok(Field {
        id: FieldId::from_validated(id),
        field_type,
        label,
        required: el.flag(FieldAttr::Required)?.unwrap_or(false),
        copy: el.flag(FieldAttr::Copy)?.unwrap_or(false),
        conceal: el.flag(FieldAttr::Conceal)?.unwrap_or(false),
        autocomplete: el.vocab(FieldAttr::Autocomplete)?,
        search: el.vocab(FieldAttr::Search)?.unwrap_or_default(),
        history: el.flag(FieldAttr::History)?.unwrap_or(false),
        multiplicity,
        browser_match: el.flag(FieldAttr::BrowserMatch)?.unwrap_or(false),
        format: el.vocab(FieldAttr::Format)?,
        precision: el.vocab(FieldAttr::Precision)?,
        max_bytes,
        digits,
        choices,
        generator,
        // Templates do not carry hints yet; the field exists in the model so
        // the catalog group is ready when they do.
        hint: None,
        props,
    })
}

/// The values of a `choice` field: ids, unique, from the `options` property.
fn choices(el: &El<'_, '_>, props: &[Prop]) -> Result<Vec<String>, ParseError> {
    let Some(options) = props.iter().find(|prop| prop.name == OPTIONS_PROP) else {
        return Err(el.error(ParseErrorKind::MissingElement {
            parent: el.name().to_owned(),
            name: Tag::Prop.name(),
        }));
    };
    let mut choices: Vec<String> = Vec::new();
    for raw in options.value.split(',') {
        let id = ids::checked(el, raw, PropAttr::Value)?;
        if choices.contains(&id) {
            return Err(el.error(ParseErrorKind::DuplicateId {
                what: OPTIONS_PROP,
                id,
            }));
        }
        choices.push(id);
    }
    Ok(choices)
}

/// `min-length` and `max-length` of a `digits` field; 1 to 64 when absent.
fn digit_count(el: &El<'_, '_>) -> Result<DigitCount, ParseError> {
    let min: u8 = el.number(FieldAttr::MinLength)?.unwrap_or(1);
    let max: u8 = el.number(FieldAttr::MaxLength)?.unwrap_or(64);
    if min == 0 || min > max || max > 64 {
        return Err(el.invalid(FieldAttr::MaxLength));
    }
    Ok(DigitCount { min, max })
}

/// Rejects type-specific attributes on a field of another type.
///
/// Walks every attribute of the vocabulary and asks `applies_to`, so no attribute
/// is named here. The model keeps `format`, `precision` and `browser_match` as
/// plain options, which makes this the only place that enforces the pairing.
fn check_applicable(el: &El<'_, '_>, field_type: FieldType) -> Result<(), ParseError> {
    for attribute in FieldAttr::ALL {
        let Some(required) = attribute.applies_to() else {
            continue;
        };
        if required != field_type && el.attr(*attribute).is_some() {
            return Err(el.error(ParseErrorKind::Misplaced {
                attribute: attribute.name(),
                requires: required.name(),
            }));
        }
    }
    Ok(())
}

/// Every attribute besides `ref` is a numeric override of a generator parameter.
///
/// The names are not checked against the generator here because that lives in
/// another document; the check belongs to a later cross-document step.
fn generator_ref(el: &El<'_, '_>) -> Result<GeneratorRef, ParseError> {
    el.no_children()?;
    let id = ids::id_attr(el, RefAttr::Ref, false)?;
    let mut overrides = Vec::new();
    for (name, value) in el.plain_attrs() {
        if name == RefAttr::Ref.name() {
            continue;
        }
        let name = ids::checked(el, name, RefAttr::Ref)?;
        let value = value.parse().map_err(|_| el.invalid(RefAttr::Ref))?;
        overrides.push(GeneratorParam { name, value });
    }
    Ok(GeneratorRef {
        id: GeneratorId::from_validated(id),
        overrides,
    })
}

fn prop(el: &El<'_, '_>) -> Result<Prop, ParseError> {
    el.expect_empty(PropAttr::ALL)?;
    let name = ids::id_attr(el, PropAttr::Name, false)?;
    let value = el.require(PropAttr::Value)?;
    if value.is_empty() || value.len() > MAX_PROP_BYTES {
        return Err(el.invalid(PropAttr::Value));
    }
    Ok(Prop {
        name,
        value: value.to_owned(),
    })
}

fn actions(el: Option<&El<'_, '_>>, known: &HashSet<String>) -> Result<Vec<Action>, ParseError> {
    let Some(el) = el else { return Ok(Vec::new()) };
    el.no_attrs()?;
    el.check_children(&[Tag::Action])?;
    let mut actions = Vec::new();
    for child in el.children()? {
        child.expect_empty(ActionAttr::ALL)?;
        let kind: ActionKind = child.require_vocab(ActionAttr::Ref)?;
        let mut fields = Vec::new();
        for raw in child.require(ActionAttr::Fields)?.split_whitespace() {
            if !known.contains(raw) {
                return Err(child.error(ParseErrorKind::UnknownField(raw.to_owned())));
            }
            fields.push(FieldId::from_validated(raw.to_owned()));
        }
        if fields.is_empty() {
            return Err(child.invalid(ActionAttr::Fields));
        }
        // A `match` and not an `if`: adding an `ActionKind` makes this fail to
        // compile until the new action's fields are read, instead of silently
        // building the wrong action.
        actions.push(match kind {
            ActionKind::ShowQr => Action::ShowQr {
                format: child.require_vocab(ActionAttr::Format)?,
                fields,
            },
        });
    }
    Ok(actions)
}

fn checks(el: Option<&El<'_, '_>>, known: &HashSet<String>) -> Result<Vec<Check>, ParseError> {
    let Some(el) = el else { return Ok(Vec::new()) };
    el.no_attrs()?;
    el.check_children(&[Tag::Check])?;
    let mut checks = Vec::new();
    for child in el.children()? {
        child.expect_empty(CheckAttr::ALL)?;
        let kind: CheckKind = child.require_vocab(CheckAttr::Ref)?;
        let mut fields = Vec::new();
        for raw in child.require(CheckAttr::Fields)?.split_whitespace() {
            if !known.contains(raw) {
                return Err(child.error(ParseErrorKind::UnknownField(raw.to_owned())));
            }
            fields.push(FieldId::from_validated(raw.to_owned()));
        }
        if fields.len() != kind.arity() {
            return Err(child.invalid(CheckAttr::Fields));
        }
        checks.push(Check { kind, fields });
    }
    Ok(checks)
}

fn behavior(el: &El<'_, '_>) -> Result<Behavior, ParseError> {
    el.no_attrs()?;
    el.check_children(&[Tag::Versions, Tag::Events])?;
    let max_versions = match el.optional(Tag::Versions)? {
        Some(versions) => {
            versions.expect_empty(VersionsAttr::ALL)?;
            let max: u8 = versions.require_number(VersionsAttr::Max)?;
            if max == 0 {
                return Err(versions.invalid(VersionsAttr::Max));
            }
            Some(max)
        }
        None => None,
    };
    let mut tracked_events = Vec::new();
    if let Some(events) = el.optional(Tag::Events)? {
        events.expect_empty(EventsAttr::ALL)?;
        for raw in events.require(EventsAttr::Track)?.split_whitespace() {
            let event =
                EventKind::from_name(raw).ok_or_else(|| events.invalid(EventsAttr::Track))?;
            if tracked_events.contains(&event) {
                return Err(events.invalid(EventsAttr::Track));
            }
            tracked_events.push(event);
        }
    }
    Ok(Behavior {
        max_versions,
        tracked_events,
    })
}

/// Reads `<migrations>`: one `<migration from="N">` per older version.
///
/// Only the migration into the current version can be checked against fields:
/// older templates are not shipped, so earlier steps are checked for syntax
/// alone, and the result of a whole chain is checked when content is migrated.
fn migrations(
    el: &El<'_, '_>,
    version: u32,
    sections: &[Section],
) -> Result<Vec<Migration>, ParseError> {
    el.no_attrs()?;
    el.check_children(&[Tag::Migration])?;
    let mut migrations = Vec::new();
    for child in el.children()? {
        child.check_attrs(MigrationAttr::ALL)?;
        child.check_children(&[Tag::Rename, Tag::ToList, Tag::Retire])?;
        let from: u32 = child.require_number(MigrationAttr::From)?;
        if from == 0 || from >= version {
            return Err(child.invalid(MigrationAttr::From));
        }
        if migrations.iter().any(|m: &Migration| m.from == from) {
            return Err(child.error(ParseErrorKind::DuplicateId {
                what: Tag::Migration.name(),
                id: from.to_string(),
            }));
        }
        let last = from + 1 == version;
        let steps = child
            .children()?
            .iter()
            .map(|step| migration_step(step, last.then_some(sections)))
            .collect::<Result<Vec<_>, _>>()?;
        migrations.push(Migration { from, steps });
    }
    migrations.sort_by_key(|migration| migration.from);
    Ok(migrations)
}

/// One step. With `current`, the step leads into the current version and its
/// targets are checked: renamed and listed fields exist, the listed one is a
/// list, retired fields are gone and their section accepts custom fields.
fn migration_step(
    el: &El<'_, '_>,
    current: Option<&[Section]>,
) -> Result<MigrationStep, ParseError> {
    let field_of = |id: &str| {
        current.map(|sections| {
            sections
                .iter()
                .flat_map(|section| &section.fields)
                .find(|field| field.id.as_str() == id)
        })
    };
    let unknown = |raw: &str| el.error(ParseErrorKind::UnknownField(raw.to_owned()));
    if el.is(Tag::Rename) {
        el.expect_empty(RenameAttr::ALL)?;
        let from = ids::id_attr(el, RenameAttr::Field, false)?;
        let to = ids::id_attr(el, RenameAttr::To, false)?;
        if from == to {
            return Err(el.invalid(RenameAttr::To));
        }
        if field_of(&to) == Some(None) {
            return Err(unknown(&to));
        }
        Ok(MigrationStep::Rename {
            from: FieldId::from_validated(from),
            to: FieldId::from_validated(to),
        })
    } else if el.is(Tag::ToList) {
        el.expect_empty(FieldRefAttr::ALL)?;
        let field = ids::id_attr(el, FieldRefAttr::Field, false)?;
        match field_of(&field) {
            Some(None) => return Err(unknown(&field)),
            Some(Some(found)) if found.multiplicity == Multiplicity::Single => {
                return Err(el.invalid(FieldRefAttr::Field));
            }
            _ => {}
        }
        Ok(MigrationStep::ToList {
            field: FieldId::from_validated(field),
        })
    } else {
        el.expect_empty(RetireAttr::ALL)?;
        let field = ids::id_attr(el, RetireAttr::Field, false)?;
        let section = ids::id_attr(el, RetireAttr::Section, false)?;
        if field_of(&field).is_some_and(|found| found.is_some()) {
            return Err(el.invalid(RetireAttr::Field));
        }
        if let Some(sections) = current {
            let accepts = sections
                .iter()
                .any(|s| s.id.as_str() == section && s.custom_fields.is_some());
            if !accepts {
                return Err(el.invalid(RetireAttr::Section));
            }
        }
        Ok(MigrationStep::Retire {
            field: FieldId::from_validated(field),
            section: SectionId::from_validated(section),
            concealed: el.flag(RetireAttr::Conceal)?.unwrap_or(true),
            multiline: el.flag(RetireAttr::Multiline)?.unwrap_or(false),
        })
    }
}
