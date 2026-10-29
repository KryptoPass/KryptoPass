//! Generators documents.

use super::error::{ParseError, ParseErrorKind};
use super::ids;
use super::reader::{El, open};
use super::vocab::{ClassAttr, IdAttr, ParamAttr, Tag};
use crate::model::{CharClass, ClassDef, GeneratorDef, GeneratorId, ParamDef};
use std::collections::HashSet;

pub(super) fn parse(xml: &str) -> Result<Vec<GeneratorDef>, ParseError> {
    let doc = open(xml)?;
    let root = El::root(&doc, Tag::Generators)?;
    root.no_attrs()?;
    root.check_children(&[Tag::Generator])?;
    let mut seen = HashSet::new();
    let mut generators = Vec::new();
    for el in root.children()? {
        let generator = generator(&el)?;
        if !seen.insert(generator.id.as_str().to_owned()) {
            return Err(el.error(ParseErrorKind::DuplicateId {
                what: Tag::Generator.name(),
                id: generator.id.as_str().to_owned(),
            }));
        }
        generators.push(generator);
    }
    Ok(generators)
}

fn generator(el: &El<'_, '_>) -> Result<GeneratorDef, ParseError> {
    el.check_attrs(IdAttr::ALL)?;
    el.check_children(&[Tag::Param, Tag::Class])?;
    let id = GeneratorId::from_validated(ids::id_attr(el, IdAttr::Id, false)?);
    let mut params: Vec<ParamDef> = Vec::new();
    for child in el.all(Tag::Param)? {
        let param = param(&child)?;
        if params.iter().any(|existing| existing.name == param.name) {
            return Err(child.error(ParseErrorKind::DuplicateId {
                what: Tag::Param.name(),
                id: param.name,
            }));
        }
        params.push(param);
    }
    let mut classes: Vec<ClassDef> = Vec::new();
    for child in el.all(Tag::Class)? {
        let class = class(&child)?;
        if classes.iter().any(|existing| existing.class == class.class) {
            return Err(child.invalid(ClassAttr::Id));
        }
        classes.push(class);
    }
    Ok(GeneratorDef {
        id,
        params,
        classes,
    })
}

fn param(el: &El<'_, '_>) -> Result<ParamDef, ParseError> {
    el.expect_empty(ParamAttr::ALL)?;
    let name = ids::id_attr(el, ParamAttr::Name, false)?;
    let default: u32 = el.require_number(ParamAttr::Default)?;
    let min: u32 = el.require_number(ParamAttr::Min)?;
    let max: u32 = el.require_number(ParamAttr::Max)?;
    if !(min <= default && default <= max) {
        return Err(el.invalid(ParamAttr::Default));
    }
    Ok(ParamDef {
        name,
        default,
        min,
        max,
    })
}

fn class(el: &El<'_, '_>) -> Result<ClassDef, ParseError> {
    el.expect_empty(ClassAttr::ALL)?;
    let class: CharClass = el.require_vocab(ClassAttr::Id)?;
    el.require(ClassAttr::Default)?;
    Ok(ClassDef {
        class,
        default: el.flag(ClassAttr::Default)?.unwrap_or(false),
    })
}
