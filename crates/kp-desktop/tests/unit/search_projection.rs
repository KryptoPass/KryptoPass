#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::login_username;
use kryptopass_search::{CatalogRecord, Index, Limits, Projection, RecordKey, SearchError};
use serde_json::{Value, json};

const RECORD: RecordKey = RecordKey {
    id: [7; 16],
    revision: 1,
};

fn login(username_json: &str) -> String {
    format!(
        r#"{{"schemaVersion":1,"kind":"login","fields":{{"username":{username_json},"password":"passwordmarkersecret","notes":"notesmarkersecret","urls":[],"totp":null,"history":{{"events":[],"versions":[]}}}}}}"#
    )
}

fn password_value(value_json: &str) -> String {
    login(r#""alice""#).replace(
        r#""password":"passwordmarkersecret""#,
        &format!(r#""password":{value_json}"#),
    )
}

fn projected(bytes: &[u8]) -> Option<Projection> {
    match login_username(bytes) {
        Ok(projection) => Some(projection),
        Err(SearchError::ProtectionUnavailable) => {
            eprintln!("private-search projection unavailable: required memory protection absent");
            None
        }
        Err(error) => panic!("valid synthetic login rejected: {error:?}"),
    }
}

fn indexed(bytes: &[u8]) -> Option<Index> {
    let projection = projected(bytes)?;
    let mut index = Index::new(Limits {
        block_bytes: 4096,
        max_private_bytes: 128 * 1024,
        ..Limits::default()
    });
    index
        .reconcile(vec![CatalogRecord {
            key: RECORD,
            public_fields: vec![],
            private_eligible: true,
        }])
        .unwrap();
    index.set_active(true);
    assert_eq!(index.next_pending(), Some(RECORD));
    match index.complete(RECORD, Ok(projection)) {
        Ok(()) => Some(index),
        Err(SearchError::ProtectionUnavailable) => {
            eprintln!("private-search index unavailable: required memory protection absent");
            None
        }
        Err(error) => panic!("synthetic projection could not enter the index: {error:?}"),
    }
}

fn rejects(bytes: &[u8], expected: SearchError) {
    match login_username(bytes) {
        Ok(_) => panic!("invalid synthetic login unexpectedly projected"),
        Err(error) => assert_eq!(error, expected),
    }
}

#[test]
fn indexes_only_the_current_username_and_excludes_other_private_fields() {
    let document = json!({
        "schemaVersion": 1,
        "kind": "login",
        "fields": {
            "username": "Current.Account",
            "password": "passwordmarkersecret",
            "notes": "notesmarkersecret",
            "urls": ["https://urlmarkersecret.invalid/login"],
            "totp": null,
            "history": {
                "events": [{"timestamp": 10, "kind": "edited"}],
                "versions": [{
                    "changedAt": 9,
                    "title": "historytitlemarker",
                    "username": "previousaccountmarker",
                    "password": "previouspasswordmarker",
                    "notes": "previousnotesmarker",
                    "urls": ["https://previousurlmarker.invalid"],
                    "totp": null
                }]
            }
        }
    });
    let Some(index) = indexed(&serde_json::to_vec(&document).unwrap()) else {
        return;
    };
    assert_eq!(index.search("current.account").unwrap(), vec![RECORD.id]);
    assert_eq!(index.coverage().indexed, 1);
    assert_eq!(
        index.memory_usage().private_live_bytes,
        "current.account".len()
    );
    for excluded in [
        "passwordmarkersecret",
        "notesmarkersecret",
        "urlmarkersecret",
        "historytitlemarker",
        "previousaccountmarker",
        "previouspasswordmarker",
        "previousnotesmarker",
        "previousurlmarker",
        "username",
        "password",
        "schemaVersion",
    ] {
        assert!(
            index.search(excluded).unwrap().is_empty(),
            "indexed {excluded}"
        );
    }
}

#[test]
fn escaped_keys_and_reordered_fields_select_the_current_username() {
    let document = r#" {
        "fields": {
            "history": {"username": "historicalmarker"},
            "totp": null, "urls": [], "notes": "ignorednotemarker",
            "password": "ignoredpasswordmarker",
            "user\u006eame": "Ali\u0063e\u0040example.com"
        },
        "k\u0069nd": "lo\u0067in", "schema\u0056ersion": 1
    } "#;
    let Some(index) = indexed(document.as_bytes()) else {
        return;
    };
    assert_eq!(index.search("alice@example.com").unwrap(), vec![RECORD.id]);
    assert!(index.search("historicalmarker").unwrap().is_empty());
    assert!(index.search("ignorednotemarker").unwrap().is_empty());
    assert!(index.search("ignoredpasswordmarker").unwrap().is_empty());
}

#[test]
fn strings_decode_utf8_surrogate_pairs_and_all_json_escapes() {
    for (encoded, query) in [
        (r#""Ana María 東京""#, "Ana María 東京"),
        (r#""ana\u00ed\u0061""#, "anaía"),
        (r#""crab\uD83E\uDD80account""#, "crab🦀account"),
        (r#""edge\udbff\udfffvalue""#, "edge\u{10ffff}value"),
        (r#""a\"b\\c\/d""#, "a\"b\\c/d"),
        (r#""a\bb\fc\nd\re\tf""#, "a\u{8}b\u{c}c\nd\re\tf"),
        (r#""nul\u0000name""#, "nul\0name"),
    ] {
        let Some(index) = indexed(login(encoded).as_bytes()) else {
            return;
        };
        assert_eq!(index.search(query).unwrap(), vec![RECORD.id], "{encoded}");
        assert!(index.search("unrelatedmarker").unwrap().is_empty());
    }
}

#[test]
fn empty_and_whitespace_usernames_do_not_index_ignored_markers() {
    for encoded in [r#""""#, r#"" \t\r\n ""#] {
        let Some(index) = indexed(login(encoded).as_bytes()) else {
            return;
        };
        assert_eq!(index.coverage().indexed, 1);
        assert_eq!(index.memory_usage().private_live_bytes, 0);
        assert!(index.search("passwordmarkersecret").unwrap().is_empty());
    }
}

#[test]
fn malformed_strings_are_rejected_even_in_ignored_values() {
    for encoded in [
        r#""\x20""#,
        r#""\u""#,
        r#""\u123""#,
        r#""\u12xz""#,
        r#""\ud800""#,
        r#""\udfff""#,
        r#""\ud800\u0041""#,
        r#""\ud800\ud800""#,
        r#""\udc00\ud800""#,
        r#""\ud800x""#,
        r#""unterminated"#,
        "\"raw\ncontrol\"",
        "\"raw\0control\"",
    ] {
        for document in [login(encoded), password_value(encoded)] {
            assert!(serde_json::from_str::<Value>(&document).is_err());
            rejects(document.as_bytes(), SearchError::InvalidInput);
        }
    }
    let malformed_key = login(r#""alice""#).replace("username", r"user\uD800name");
    rejects(malformed_key.as_bytes(), SearchError::InvalidInput);
}

#[test]
fn invalid_utf8_is_rejected_before_projection() {
    let mut document = login(r#""alice""#).into_bytes();
    let position = document
        .windows(5)
        .position(|bytes| bytes == b"alice")
        .unwrap();
    document.splice(position..position + 5, [0xff, 0xc0, 0xaf]);
    rejects(&document, SearchError::InvalidInput);
}

#[test]
fn duplicate_known_fields_are_rejected_including_escaped_aliases() {
    let original = login(r#""alice""#);
    let parsed: Value = serde_json::from_str(&original).unwrap();
    let fields = parsed["fields"].to_string();
    let root_duplicates = [
        original.replace(
            r#""schemaVersion":1"#,
            r#""schemaVersion":1,"schemaVersion":1"#,
        ),
        original.replace(r#""kind":"login""#, r#""kind":"login","k\u0069nd":"login""#),
        format!(r#"{{"schemaVersion":1,"kind":"login","fields":{fields},"fields":{fields}}}"#),
    ];
    for document in root_duplicates {
        assert!(serde_json::from_str::<Value>(&document).is_ok());
        rejects(document.as_bytes(), SearchError::InvalidInput);
    }
    for field in ["username", "password", "notes", "urls", "totp", "history"] {
        let value = &parsed["fields"][field];
        let document = original.replace(
            &format!(r#""{field}":"#),
            &format!(r#""{field}":{value},"{field}":"#),
        );
        assert!(serde_json::from_str::<Value>(&document).is_ok());
        rejects(document.as_bytes(), SearchError::InvalidInput);
    }
    let escaped_username = original.replace(
        r#""username":"alice""#,
        r#""username":"first","user\u006eame":"alice""#,
    );
    rejects(escaped_username.as_bytes(), SearchError::InvalidInput);
}

#[test]
fn required_fields_must_exist_and_unknown_fields_are_rejected() {
    let original: Value = serde_json::from_str(&login(r#""alice""#)).unwrap();
    for key in ["schemaVersion", "kind", "fields"] {
        let mut document = original.clone();
        document.as_object_mut().unwrap().remove(key);
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
    for key in ["username", "password", "notes", "urls", "totp", "history"] {
        let mut document = original.clone();
        document["fields"].as_object_mut().unwrap().remove(key);
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
    for (at_root, key) in [(true, "extra"), (false, "extra"), (false, "Username")] {
        let mut document = original.clone();
        if at_root {
            document[key] = json!("unknownmarker");
        } else {
            document["fields"][key] = json!("unknownmarker");
        }
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
}

#[test]
fn incompatible_schema_kind_fields_and_username_types_are_rejected() {
    let original: Value = serde_json::from_str(&login(r#""alice""#)).unwrap();
    for version in [
        json!(0),
        json!(2),
        json!(-1),
        json!("1"),
        json!(1.0),
        json!(null),
        json!([]),
    ] {
        let mut document = original.clone();
        document["schemaVersion"] = version;
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
    let exponent_version =
        login(r#""alice""#).replace(r#""schemaVersion":1"#, r#""schemaVersion":1e0"#);
    rejects(exponent_version.as_bytes(), SearchError::InvalidInput);
    for kind in [
        json!("note"),
        json!("Login"),
        json!(""),
        json!(1),
        json!(null),
        json!({}),
    ] {
        let mut document = original.clone();
        document["kind"] = kind;
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
    for value in [json!(null), json!(true), json!(1), json!([]), json!({})] {
        let mut document = original.clone();
        document["fields"] = value.clone();
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
        document = original.clone();
        document["fields"]["username"] = value;
        rejects(
            &serde_json::to_vec(&document).unwrap(),
            SearchError::InvalidInput,
        );
    }
}

#[test]
fn ignored_values_accept_valid_nested_json_without_indexing_it() {
    let values = [
        "null",
        "true",
        "false",
        "0",
        "-0",
        "123",
        "-456",
        "1.5",
        "1e3",
        "-2.4E-2",
        "1e+2",
        r#""ignoredvaluemarker""#,
        "[]",
        "{}",
        "[1,2,[true,false,null],{}]",
        r#"{"username":"ignoredvaluemarker","nested":[{"key":"ignoredvaluemarker"}]}"#,
        r#"{"username":"firstignoredmarker","username":"secondignoredmarker"}"#,
    ];
    for value in values {
        let document = password_value(value);
        assert!(serde_json::from_str::<Value>(&document).is_ok(), "{value}");
        let Some(index) = indexed(document.as_bytes()) else {
            return;
        };
        assert_eq!(index.search("alice").unwrap(), vec![RECORD.id]);
        for excluded in [
            "ignoredvaluemarker",
            "firstignoredmarker",
            "secondignoredmarker",
        ] {
            assert!(index.search(excluded).unwrap().is_empty());
        }
    }
}

#[test]
fn ignored_values_reject_malformed_numbers_arrays_objects_and_literals() {
    for value in [
        "-",
        "+1",
        "01",
        "-01",
        "1.",
        ".1",
        "1e",
        "1e+",
        "1e-",
        "1.0.0",
        "NaN",
        "Infinity",
        "[1,]",
        "[,1]",
        "[1 2]",
        "[1",
        "[}",
        "{",
        r#"{"key" 1}"#,
        r#"{"key":1,}"#,
        "truefalse",
        "nul",
        "False",
        "undefined",
        "/*comment*/null",
    ] {
        let document = password_value(value);
        assert!(serde_json::from_str::<Value>(&document).is_err(), "{value}");
        rejects(document.as_bytes(), SearchError::InvalidInput);
    }
}

#[test]
fn malformed_envelopes_and_trailing_data_are_rejected() {
    let valid = login(r#""alice""#);
    for document in [
        String::new(),
        " \n\t".into(),
        "null".into(),
        "[]".into(),
        "{}".into(),
        valid[..valid.len() - 1].to_owned(),
        format!("{valid}null"),
        format!("{valid}{valid}"),
        format!("\u{feff}{valid}"),
        valid.replacen(r#""fields":{"#, r#""fields":["#, 1),
        valid.replacen(r#""kind":"login""#, r#"kind:"login""#, 1),
    ] {
        rejects(document.as_bytes(), SearchError::InvalidInput);
    }
}

#[test]
fn decoded_username_limit_counts_utf8_bytes_and_not_escape_bytes() {
    for (encoded, expected_bytes, query) in [
        (format!("\"{}\"", "a".repeat(16 * 1024)), 16 * 1024, "aaaa"),
        (format!("\"{}\"", "é".repeat(8 * 1024)), 8 * 1024, "eeee"),
        (
            format!("\"{}\"", r"\u0061".repeat(16 * 1024)),
            16 * 1024,
            "aaaa",
        ),
        (
            format!("\"{}\"", r"\u00e9".repeat(8 * 1024)),
            8 * 1024,
            "eeee",
        ),
    ] {
        let Some(index) = indexed(login(&encoded).as_bytes()) else {
            return;
        };
        assert_eq!(index.memory_usage().private_live_bytes, expected_bytes);
        assert_eq!(index.search(query).unwrap(), vec![RECORD.id]);
    }
    for encoded in [
        format!("\"{}\"", "a".repeat(16 * 1024 + 1)),
        format!("\"{}\"", "é".repeat(8 * 1024 + 1)),
        format!("\"{}\"", r"\u0061".repeat(16 * 1024 + 1)),
        format!("\"{}\"", r"\u00e9".repeat(8 * 1024 + 1)),
    ] {
        rejects(login(&encoded).as_bytes(), SearchError::LimitExceeded);
    }
}

#[test]
fn normalization_expansion_cannot_exceed_its_independent_budget() {
    // This ligature expands to a full ASCII phrase under transliteration.
    let username = "ﷺ".repeat(3000);
    assert!(username.len() < 16 * 1024);
    let document = login(&serde_json::to_string(&username).unwrap());
    rejects(document.as_bytes(), SearchError::LimitExceeded);
}

#[test]
fn maximum_content_size_is_accepted_and_one_more_byte_is_rejected() {
    let mut document = login(r#""alice""#).into_bytes();
    document.resize(kryptopass_core::MAX_CONTENT_BYTES, b' ');
    let Some(index) = indexed(&document) else {
        return;
    };
    assert_eq!(index.search("alice").unwrap(), vec![RECORD.id]);
    document.push(b' ');
    rejects(&document, SearchError::LimitExceeded);
}

#[test]
fn ignored_nested_values_obey_the_depth_limit() {
    for object_nesting in [false, true] {
        let nested = |depth: usize| {
            if object_nesting {
                format!("{}0{}", r#"{"key":"#.repeat(depth), "}".repeat(depth))
            } else {
                format!("{}0{}", "[".repeat(depth), "]".repeat(depth))
            }
        };
        // The envelope and fields object occupy the first two nesting levels.
        let within_limit = password_value(&nested(125));
        let Some(index) = indexed(within_limit.as_bytes()) else {
            return;
        };
        assert_eq!(index.search("alice").unwrap(), vec![RECORD.id]);
        let over_limit = password_value(&nested(126));
        rejects(over_limit.as_bytes(), SearchError::LimitExceeded);
    }
}
