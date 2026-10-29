#![allow(clippy::unwrap_used)]
//! Documents that must be rejected (or, where stated, accepted) by the parser.
//!
//! Each case starts from a minimal valid document and changes one thing, so a
//! failure points at the rule that broke.

use kp_template::{ParseErrorKind, Text, parse_catalog, parse_generators, parse_template};

const NS: &str = "https://kryptopass.org/ns/template/1";

fn template(kind: &str, id: &str, body: &str) -> String {
    let name = if kind == "builtin" {
        "<kp:name/>".to_owned()
    } else {
        r#"<kp:name xml:lang="en">Home</kp:name>"#.to_owned()
    };
    format!(
        r#"<kp:template xmlns:kp="{NS}" id="{id}" version="1" kind="{kind}">
             <kp:meta>{name}<kp:icon name="go-home"/><kp:search-title field="title"/></kp:meta>
             <kp:fields><kp:section id="s">{body}</kp:section></kp:fields>
           </kp:template>"#
    )
}

fn builtin(body: &str) -> String {
    template("builtin", "x", body)
}

fn field(attrs: &str) -> String {
    builtin(&format!(
        r#"<kp:field id="f" {attrs}><kp:label/></kp:field>"#
    ))
}

fn kind_of(xml: &str) -> ParseErrorKind {
    parse_template(xml).unwrap_err().kind
}

fn catalog(body: &str) -> String {
    format!(r#"<kp:catalog xmlns:kp="{NS}" xml:lang="en" version="1">{body}</kp:catalog>"#)
}

#[test]
fn minimal_template_is_valid() {
    parse_template(&field(r#"type="text""#)).unwrap();
}

#[test]
fn dtd_is_refused() {
    let xml = format!(r#"<!DOCTYPE a [<!ENTITY e "x">]><kp:template xmlns:kp="{NS}"/>"#);
    assert!(matches!(kind_of(&xml), ParseErrorKind::Xml(_)));
}

#[test]
fn oversized_document_is_refused() {
    assert!(matches!(
        kind_of(&"a".repeat(300_000)),
        ParseErrorKind::TooLarge
    ));
}

#[test]
fn wrong_root_is_refused() {
    let xml = format!(r#"<kp:catalog xmlns:kp="{NS}" xml:lang="en" version="1"/>"#);
    assert!(matches!(kind_of(&xml), ParseErrorKind::WrongRoot { .. }));
}

#[test]
fn unknown_element_in_the_namespace_is_refused() {
    assert!(matches!(
        kind_of(&builtin("<kp:evil/>")),
        ParseErrorKind::UnexpectedElement(_)
    ));
}

#[test]
fn element_without_namespace_is_refused() {
    assert!(matches!(
        kind_of(&builtin("<field/>")),
        ParseErrorKind::UnexpectedElement(_)
    ));
}

#[test]
fn element_of_another_namespace_is_ignored() {
    let body = r#"<x:note xmlns:x="urn:x"/><kp:field id="f" type="text"><kp:label/></kp:field>"#;
    parse_template(&builtin(body)).unwrap();
}

#[test]
fn unknown_attribute_is_refused() {
    let error = kind_of(&field(r#"type="text" onclick="x""#));
    assert!(matches!(error, ParseErrorKind::UnexpectedAttribute { .. }));
}

#[test]
fn namespaced_attribute_is_ignored() {
    parse_template(&field(r#"type="text" xmlns:x="urn:x" x:hint="y""#)).unwrap();
}

#[test]
fn unknown_field_type_is_refused() {
    assert!(matches!(
        kind_of(&field(r#"type="select""#)),
        ParseErrorKind::InvalidValue { .. }
    ));
}

#[test]
fn type_specific_attributes_are_refused_on_other_types() {
    for attrs in [
        r#"type="text" format="markdown""#,
        r#"type="text" precision="month""#,
        r#"type="text" browser-match="true""#,
        r#"type="url" format="markdown""#,
    ] {
        assert!(
            matches!(kind_of(&field(attrs)), ParseErrorKind::Misplaced { .. }),
            "{attrs}"
        );
    }
}

#[test]
fn type_specific_attributes_are_accepted_on_their_type() {
    for attrs in [
        r#"type="multiline" format="markdown""#,
        r#"type="date" precision="month""#,
        r#"type="url" browser-match="true""#,
    ] {
        parse_template(&field(attrs)).unwrap();
    }
}

#[test]
fn closed_values_outside_their_list_are_refused() {
    for attrs in [
        r#"type="text" search="always""#,
        r#"type="text" autocomplete="evil""#,
        r#"type="multiline" format="html""#,
        r#"type="date" precision="year""#,
        r#"type="text" copy="ture""#,
    ] {
        assert!(
            matches!(kind_of(&field(attrs)), ParseErrorKind::InvalidValue { .. }),
            "{attrs}"
        );
    }
}

#[test]
fn multiplicity_needs_both_flag_and_max() {
    for attrs in [
        r#"type="url" max="3""#,
        r#"type="url" multiple="true""#,
        r#"type="url" multiple="true" max="0""#,
    ] {
        assert!(
            matches!(kind_of(&field(attrs)), ParseErrorKind::InvalidValue { .. }),
            "{attrs}"
        );
    }
    parse_template(&field(r#"type="url" multiple="true" max="3""#)).unwrap();
}

#[test]
fn identifiers_are_validated() {
    for id in ["Foo", "1abc", "a b", "a_b", "-a", "a-", "", &"a".repeat(65)] {
        let xml = builtin(&format!(
            r#"<kp:field id="{id}" type="text"><kp:label/></kp:field>"#
        ));
        assert!(parse_template(&xml).is_err(), "id `{id}` was accepted");
    }
}

#[test]
fn duplicate_ids_are_refused() {
    let two = r#"<kp:field id="f" type="text"><kp:label/></kp:field>"#.repeat(2);
    assert!(matches!(
        kind_of(&builtin(&two)),
        ParseErrorKind::DuplicateId { .. }
    ));
    let sections = r#"<kp:fields><kp:section id="s"/><kp:section id="s"/></kp:fields>"#;
    let xml = format!(
        r#"<kp:template xmlns:kp="{NS}" id="x" version="1" kind="builtin"><kp:meta><kp:name/><kp:icon name="a"/><kp:search-title field="title"/></kp:meta>{sections}</kp:template>"#
    );
    assert!(matches!(kind_of(&xml), ParseErrorKind::DuplicateId { .. }));
}

#[test]
fn builtin_and_user_templates_cannot_swap_id_namespaces() {
    let body = r#"<kp:field id="f" type="text"><kp:label xml:lang="en">F</kp:label></kp:field>"#;
    assert!(parse_template(&template("builtin", "user.x", body)).is_err());
    assert!(parse_template(&template("user", "x", body)).is_err());
    parse_template(&template("user", "user.x", body)).unwrap();
}

#[test]
fn version_zero_is_refused() {
    let xml = builtin("").replace(r#"version="1""#, r#"version="0""#);
    assert!(parse_template(&xml).is_err());
}

#[test]
fn references_to_missing_fields_are_refused() {
    let summary = builtin(r#"<kp:field id="f" type="text"><kp:label/></kp:field>"#)
        .replace("</kp:meta>", r#"<kp:summary field="ghost"/></kp:meta>"#);
    assert!(matches!(kind_of(&summary), ParseErrorKind::UnknownField(_)));
    let action = builtin(r#"<kp:field id="f" type="text"><kp:label/></kp:field>"#)
        .replace("</kp:fields>", r#"</kp:fields><kp:actions><kp:action ref="show-qr" format="wifi" fields="ghost"/></kp:actions>"#);
    assert!(matches!(kind_of(&action), ParseErrorKind::UnknownField(_)));
}

#[test]
fn builtin_labels_cannot_carry_text() {
    let xml = builtin(r#"<kp:field id="f" type="text"><kp:label>Texto</kp:label></kp:field>"#);
    assert!(matches!(kind_of(&xml), ParseErrorKind::InvalidValue { .. }));
}

#[test]
fn builtin_label_appearing_twice_is_refused() {
    let xml = builtin(r#"<kp:field id="f" type="text"><kp:label/><kp:label/></kp:field>"#);
    assert!(matches!(kind_of(&xml), ParseErrorKind::DuplicateElement(_)));
}

#[test]
fn user_template_reads_literals_per_language() {
    let body = r#"<kp:field id="alarm" type="secret">
                    <kp:label xml:lang="es">Alarma</kp:label><kp:label xml:lang="en">Alarm</kp:label>
                  </kp:field>"#;
    let parsed = parse_template(&template("user", "user.casa", body)).unwrap();
    let Text::Literal(literals) = &parsed.sections[0].fields[0].label else {
        panic!("expected literals")
    };
    assert_eq!(literals.len(), 2);
}

#[test]
fn user_labels_need_a_language_and_no_repeats() {
    let missing = template(
        "user",
        "user.x",
        r#"<kp:field id="f" type="text"><kp:label>F</kp:label></kp:field>"#,
    );
    assert!(matches!(
        kind_of(&missing),
        ParseErrorKind::MissingAttribute { .. }
    ));
    let repeated = template(
        "user",
        "user.x",
        r#"<kp:field id="f" type="text"><kp:label xml:lang="en">A</kp:label><kp:label xml:lang="en">B</kp:label></kp:field>"#,
    );
    assert!(matches!(
        kind_of(&repeated),
        ParseErrorKind::DuplicateId { .. }
    ));
}

#[test]
fn limits_and_custom_fields_are_validated() {
    let zero =
        builtin(r#"<kp:field id="f" type="text"><kp:label/><kp:limit max-bytes="0"/></kp:field>"#);
    assert!(parse_template(&zero).is_err());
    let no_max = builtin(r#"<kp:custom-fields allowed="true"/>"#);
    assert!(matches!(
        kind_of(&no_max),
        ParseErrorKind::MissingAttribute { .. }
    ));
    let off = parse_template(&builtin(r#"<kp:custom-fields allowed="false"/>"#)).unwrap();
    assert!(off.sections[0].custom_fields.is_none());
}

#[test]
fn errors_carry_a_line_and_no_stored_value() {
    let xml = "<kp:template xmlns:kp=\"https://kryptopass.org/ns/template/1\" id=\"x\" version=\"1\" kind=\"builtin\">\n\n<kp:evil/></kp:template>";
    let error = parse_template(xml).unwrap_err();
    assert_eq!(error.line, 3);
    assert_eq!(error.to_string(), "line 3: unexpected element <evil>");
}

#[test]
fn catalog_rules() {
    parse_catalog(&catalog(
        r#"<kp:labels><kp:text id="a">A</kp:text></kp:labels>"#,
    ))
    .unwrap();
    let duplicate =
        catalog(r#"<kp:labels><kp:text id="a">A</kp:text><kp:text id="a">B</kp:text></kp:labels>"#);
    assert!(matches!(
        parse_catalog(&duplicate).unwrap_err().kind,
        ParseErrorKind::DuplicateId { .. }
    ));
    let unknown_group = catalog(r#"<kp:colors/>"#);
    assert!(parse_catalog(&unknown_group).is_err());
    let empty_text = catalog(r#"<kp:labels><kp:text id="a">  </kp:text></kp:labels>"#);
    assert!(parse_catalog(&empty_text).is_err());
    assert!(parse_catalog(&catalog("").replace(r#"version="1""#, r#"version="2""#)).is_err());
    assert!(parse_catalog(&catalog("").replace(r#" xml:lang="en""#, "")).is_err());
    assert!(parse_catalog(&catalog("").replace("en", "e n!")).is_err());
}

#[test]
fn same_id_in_different_catalog_scopes_is_not_a_duplicate() {
    let xml = catalog(
        r#"<kp:labels><kp:text id="a">A</kp:text></kp:labels><kp:sections><kp:text id="a">B</kp:text></kp:sections>"#,
    );
    assert_eq!(parse_catalog(&xml).unwrap().messages.len(), 2);
}

#[test]
fn generator_rules() {
    let wrap = |body: &str| format!(r#"<kp:generators xmlns:kp="{NS}">{body}</kp:generators>"#);
    parse_generators(&wrap(r#"<kp:generator id="g"><kp:param name="length" default="8" min="4" max="16"/><kp:class id="lower" default="true"/></kp:generator>"#)).unwrap();
    for body in [
        r#"<kp:generator id="g"><kp:param name="n" default="99" min="4" max="16"/></kp:generator>"#,
        r#"<kp:generator id="g"/><kp:generator id="g"/>"#,
        r#"<kp:generator id="g"><kp:class id="emoji" default="true"/></kp:generator>"#,
        r#"<kp:generator id="g"><kp:class id="lower" default="true"/><kp:class id="lower" default="false"/></kp:generator>"#,
        r#"<kp:generator id="g"><kp:param name="n" default="8" min="4" max="16"/><kp:param name="n" default="8" min="4" max="16"/></kp:generator>"#,
    ] {
        assert!(parse_generators(&wrap(body)).is_err(), "{body}");
    }
}
