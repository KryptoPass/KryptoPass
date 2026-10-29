//! Message catalog documents.

use super::error::{ParseError, ParseErrorKind};
use super::ids;
use super::reader::{El, open};
use super::vocab::{CatalogAttr, IdAttr, MESSAGE, Tag, XML_LANG};
use crate::model::{Catalog, LanguageTag, MessageId, MessageRef, MessageScope};
use std::collections::BTreeMap;

/// The only catalog format version this reader understands.
const SUPPORTED_VERSION: u32 = 1;

pub(super) fn parse(xml: &str) -> Result<Catalog, ParseError> {
    let doc = open(xml)?;
    let root = El::root(&doc, Tag::Catalog)?;
    root.check_attrs(CatalogAttr::ALL)?;
    root.check_children(MessageScope::ALL)?;

    let version: u32 = root.require_number(CatalogAttr::Version)?;
    if version != SUPPORTED_VERSION {
        return Err(root.invalid(CatalogAttr::Version));
    }
    let tag = root.lang().ok_or_else(|| {
        root.error(ParseErrorKind::MissingAttribute {
            element: Tag::Catalog.name().to_owned(),
            name: XML_LANG,
        })
    })?;
    let language = LanguageTag::from_validated(ids::language(&root, tag)?);

    // Groups are found through `MessageScope::ALL`, so the catalog layout and the
    // scopes templates refer to cannot disagree: a new scope is read automatically.
    let mut messages = BTreeMap::new();
    for scope in MessageScope::ALL {
        let Some(group) = root.optional(*scope)? else {
            continue;
        };
        group.no_attrs()?;
        group.check_children(&[Tag::Text])?;
        for entry in group.children()? {
            entry.check_attrs(IdAttr::ALL)?;
            let id = ids::id_attr(&entry, IdAttr::Id, false)?;
            let key = MessageRef {
                scope: *scope,
                id: MessageId::from_validated(id.clone()),
            };
            // `insert` returns the previous value, so a repeated id is caught here
            // without a separate lookup.
            if messages.insert(key, entry.text()?).is_some() {
                return Err(entry.error(ParseErrorKind::DuplicateId { what: MESSAGE, id }));
            }
        }
    }
    Ok(Catalog {
        language,
        version,
        messages,
    })
}
