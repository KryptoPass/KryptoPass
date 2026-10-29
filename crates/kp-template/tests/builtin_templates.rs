#![allow(clippy::unwrap_used)]
//! The shipped templates, catalogs and generators, embedded and read with the real parser.

use kp_template::{
    FieldType, Library, LibraryError, MessageRef, MessageScope, SearchMode, TemplateKind,
    referenced_messages,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

/// Property that lists the values of a `select` widget.
const OPTIONS_PROP: &str = "options";
/// Property that picks how a field is drawn.
const WIDGET_PROP: &str = "widget";
/// Widget value for a drop-down list.
const SELECT_WIDGET: &str = "select";
/// Option whose label is translated; the rest are shown as written.
const TRANSLATED_OPTION: &str = "none";

fn builtin() -> Library {
    Library::builtin().unwrap()
}

fn file_stems(dir: &str) -> BTreeSet<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(dir);
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn builtin_library_builds() {
    let library = builtin();
    assert_eq!(library.templates().len(), 19);
    assert_eq!(library.catalogs().len(), 4);
    assert_eq!(library.generators().len(), 2);
    assert!(
        library
            .templates()
            .iter()
            .all(|t| t.kind == TemplateKind::Builtin)
    );
}

#[test]
fn embedded_lists_match_the_directories() {
    let library = builtin();
    let templates: BTreeSet<_> = library
        .templates()
        .iter()
        .map(|t| t.id.as_str().to_owned())
        .collect();
    assert_eq!(
        templates,
        file_stems("builtin"),
        "a template file is not embedded, or the reverse"
    );
    let languages: BTreeSet<_> = library
        .catalogs()
        .iter()
        .map(|c| c.language.as_str().to_owned())
        .collect();
    assert_eq!(
        languages,
        file_stems("i18n"),
        "a catalog file is not embedded, or the reverse"
    );
}

#[test]
fn lookups_find_what_was_loaded() {
    let library = builtin();
    assert!(library.template("wifi").is_some());
    assert!(library.template("nope").is_none());
    assert!(library.catalog("pt-BR").is_some());
    assert!(library.catalog("pt").is_none(), "lookup must not fall back");
    assert!(library.generator("pin").is_some());
}

#[test]
fn catalogs_hold_the_same_messages_in_every_language() {
    let library = builtin();
    let reference: BTreeSet<_> = library
        .catalog("es")
        .unwrap()
        .messages
        .keys()
        .cloned()
        .collect();
    for catalog in library.catalogs() {
        let keys: BTreeSet<_> = catalog.messages.keys().cloned().collect();
        assert_eq!(
            keys,
            reference,
            "catalog `{}` differs from `es`",
            catalog.language.as_str()
        );
        assert!(
            catalog
                .messages
                .values()
                .all(|text| !text.trim().is_empty())
        );
    }
}

/// Everything a catalog should hold: what templates refer to, plus the
/// translated option of any `select`.
fn expected_messages(library: &Library) -> BTreeSet<MessageRef> {
    let mut expected: BTreeSet<_> = library
        .templates()
        .iter()
        .flat_map(referenced_messages)
        .collect();
    let has_translated_option = library
        .templates()
        .iter()
        .flat_map(|t| &t.sections)
        .flat_map(|s| &s.fields)
        .any(|f| {
            f.props.iter().any(|p| {
                p.name == OPTIONS_PROP && p.value.split(',').any(|v| v == TRANSLATED_OPTION)
            })
        });
    if has_translated_option {
        let option = library
            .catalog("es")
            .unwrap()
            .messages
            .keys()
            .find(|m| m.scope == MessageScope::Option);
        expected.insert(option.unwrap().clone());
    }
    expected
}

#[test]
fn no_catalog_message_is_unused() {
    let library = builtin();
    let expected = expected_messages(&library);
    for catalog in library.catalogs() {
        let actual: BTreeSet<_> = catalog.messages.keys().cloned().collect();
        let unused: Vec<_> = actual
            .difference(&expected)
            .map(|m| m.id.as_str())
            .collect();
        assert!(
            unused.is_empty(),
            "`{}` has unused messages: {unused:?}",
            catalog.language.as_str()
        );
    }
}

#[test]
fn concealed_fields_are_never_indexed() {
    for template in builtin().templates() {
        for field in template.sections.iter().flat_map(|s| &s.fields) {
            if field.field_type.is_concealed() || field.conceal {
                assert_eq!(
                    field.search,
                    SearchMode::None,
                    "{}.{} is concealed but asks to be indexed",
                    template.id.as_str(),
                    field.id.as_str()
                );
            }
        }
    }
}

#[test]
fn history_fields_come_with_a_version_limit() {
    for template in builtin().templates() {
        let wants_history = template
            .sections
            .iter()
            .flat_map(|s| &s.fields)
            .any(|f| f.history);
        if wants_history {
            let limit = template.behavior.as_ref().and_then(|b| b.max_versions);
            assert!(
                limit.is_some(),
                "{} marks history without a version limit",
                template.id.as_str()
            );
        }
    }
}

#[test]
fn select_widgets_declare_their_options() {
    for template in builtin().templates() {
        for field in template.sections.iter().flat_map(|s| &s.fields) {
            let is_select = field
                .props
                .iter()
                .any(|p| p.name == WIDGET_PROP && p.value == SELECT_WIDGET);
            if is_select {
                assert!(
                    field.props.iter().any(|p| p.name == OPTIONS_PROP),
                    "{}.{} is a select without options",
                    template.id.as_str(),
                    field.id.as_str()
                );
            }
        }
    }
}

#[test]
fn login_has_a_totp_field_and_wifi_a_qr_action() {
    let library = builtin();
    let login = library.template("login").unwrap();
    let totp = login
        .sections
        .iter()
        .flat_map(|s| &s.fields)
        .find(|f| f.id.as_str() == "totp");
    assert_eq!(totp.unwrap().field_type, FieldType::Totp);
    assert_eq!(library.template("wifi").unwrap().actions.len(), 1);
}

// ---- Cross-document checks must fail when the parts disagree ----

fn parts() -> (
    Vec<kp_template::Template>,
    Vec<kp_template::Catalog>,
    Vec<kp_template::GeneratorDef>,
) {
    let library = builtin();
    (
        library.templates().to_vec(),
        library.catalogs().to_vec(),
        library.generators().to_vec(),
    )
}

#[test]
fn a_catalog_with_a_gap_is_refused() {
    let (templates, mut catalogs, generators) = parts();
    let removed = catalogs[0]
        .messages
        .keys()
        .find(|m| m.scope == MessageScope::Label)
        .unwrap()
        .clone();
    catalogs[0].messages.remove(&removed);
    let error = Library::new(templates, catalogs, generators).unwrap_err();
    assert!(
        matches!(error, LibraryError::MissingMessage { .. }),
        "{error}"
    );
}

#[test]
fn an_undeclared_generator_is_refused() {
    let (templates, catalogs, mut generators) = parts();
    generators.retain(|g| g.id.as_str() != "pin");
    let error = Library::new(templates, catalogs, generators).unwrap_err();
    assert!(
        matches!(error, LibraryError::UnknownGenerator { .. }),
        "{error}"
    );
}

#[test]
fn a_generator_override_out_of_range_is_refused() {
    let (templates, catalogs, mut generators) = parts();
    for param in generators.iter_mut().flat_map(|g| &mut g.params) {
        param.max = param.min;
        param.default = param.min;
    }
    let error = Library::new(templates, catalogs, generators).unwrap_err();
    assert!(
        matches!(error, LibraryError::GeneratorValueOutOfRange { .. }),
        "{error}"
    );
}

#[test]
fn an_unknown_generator_parameter_is_refused() {
    let (templates, catalogs, mut generators) = parts();
    for generator in &mut generators {
        generator.params.clear();
    }
    let error = Library::new(templates, catalogs, generators).unwrap_err();
    assert!(
        matches!(error, LibraryError::UnknownGeneratorParam { .. }),
        "{error}"
    );
}

#[test]
fn duplicates_are_refused() {
    let (templates, catalogs, generators) = parts();
    let mut twice = templates.clone();
    twice.push(templates[0].clone());
    assert!(matches!(
        Library::new(twice, catalogs.clone(), generators.clone()).unwrap_err(),
        LibraryError::DuplicateTemplate(_)
    ));
    let mut two_languages = catalogs.clone();
    two_languages.push(catalogs[0].clone());
    assert!(matches!(
        Library::new(templates.clone(), two_languages, generators.clone()).unwrap_err(),
        LibraryError::DuplicateLanguage(_)
    ));
    let mut two_generators = generators.clone();
    two_generators.push(generators[0].clone());
    assert!(matches!(
        Library::new(templates, catalogs, two_generators).unwrap_err(),
        LibraryError::DuplicateGenerator(_)
    ));
}

#[test]
fn user_templates_need_no_catalog_entries() {
    let (mut templates, catalogs, generators) = parts();
    let xml = r#"<kp:template xmlns:kp="https://kryptopass.org/ns/template/1" id="user.casa" version="1" kind="user">
        <kp:meta><kp:name xml:lang="es">Casa</kp:name><kp:icon name="go-home"/><kp:search-title field="title"/></kp:meta>
        <kp:fields><kp:section id="s"><kp:field id="alarm" type="secret"><kp:label xml:lang="es">Alarma</kp:label></kp:field></kp:section></kp:fields>
      </kp:template>"#;
    templates.push(kp_template::parse_template(xml).unwrap());
    let library = Library::new(templates, catalogs, generators).unwrap();
    assert!(library.template("user.casa").is_some());
}
