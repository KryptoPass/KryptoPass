use kryptopass_core::search::MetadataSearch;

#[test]
fn query_words_can_match_title_folder_and_kind_in_any_order() {
    let mut search = MetadataSearch::default();
    let metadata = "Correo personal Familia y cuentas Login";

    for query in [
        "correo familia login",
        "login correo familia",
        "familia login correo",
        "correo corre login",
    ] {
        assert!(search.matches(query, metadata), "query: {query:?}");
    }
    assert!(!search.matches("correo trabajo login", metadata));
    assert!(!search.matches("correo corre ausente", metadata));
}

#[test]
fn plain_queries_match_accents_and_ignore_letter_case() {
    let mut search = MetadataSearch::default();
    let metadata = "Conexión doméstica Compañía Wi-Fi";

    for query in [
        "conexion compania",
        "CONEXION DOMESTICA",
        "compania",
        "wi-fi",
        "compania wi-fi",
    ] {
        assert!(search.matches(query, metadata), "query: {query:?}");
    }
    assert!(search.matches("correo familia login", "CORREO FAMILIA LOGIN"));
}

#[test]
fn decomposed_accents_do_not_interrupt_plain_queries() {
    let mut search = MetadataSearch::default();
    let metadata = "Conexio\u{301}n dome\u{301}stica Compan\u{303}i\u{301}a Wi-Fi";

    for query in [
        "conexion",
        "domestica",
        "compania",
        "conexion domestica compania",
    ] {
        assert!(search.matches(query, metadata), "query: {query:?}");
    }
    assert!(search.matches("CONEXION COMPANIA", metadata));
}

#[test]
fn empty_or_whitespace_only_queries_match_all_metadata() {
    let mut search = MetadataSearch::default();

    for query in ["", " ", "   ", "\t\n"] {
        for metadata in ["Correo personal Familia Login", ""] {
            assert!(search.matches(query, metadata), "query: {query:?}");
        }
    }
}

#[test]
fn changing_queries_and_clearing_them_does_not_reuse_old_matches() {
    let mut search = MetadataSearch::default();
    let metadata = "Correo personal Familia Login";

    for (query, expected) in [
        ("correo", true),
        ("correo ausente", false),
        ("familia", true),
        ("trabajo", false),
        ("", true),
        ("ausente", false),
        ("   ", true),
        ("", true),
    ] {
        assert_eq!(
            search.matches(query, metadata),
            expected,
            "query: {query:?}"
        );
    }
}

#[test]
fn search_syntax_symbols_are_literal_metadata_characters() {
    let mut search = MetadataSearch::default();

    for (query, metadata, without_symbols) in [
        (
            "^correo",
            "Cuenta ^correo personal Login",
            "correo personal Login",
        ),
        (
            "!correo",
            "Cuenta !correo personal Login",
            "Cuenta personal Login",
        ),
        (
            "correo$",
            "Cuenta correo$ personal Login",
            "Cuenta correo personal Login",
        ),
        (
            "cuenta[1].*",
            "Correo cuenta[1].* personal Login",
            "Correo cuenta1 cualquiera personal Login",
        ),
    ] {
        assert!(search.matches(query, metadata), "query: {query:?}");
        assert!(!search.matches(query, without_symbols), "query: {query:?}");
    }
}

#[test]
fn grouped_fingerprint_queries_match_continuous_fingerprints() {
    let mut search = MetadataSearch::default();
    let metadata = "Llave personal ABCD1234EF567890A1B2C3D4E5F60718293A4B5C";

    for query in [
        "ABCD", "1234", "EF56", "7890", "A1B2", "C3D4", "E5F6", "0718", "293A", "4B5C",
    ] {
        assert!(search.matches(query, metadata), "query: {query:?}");
    }
    assert!(search.matches(
        "ABCD 1234 EF56 7890 A1B2 C3D4 E5F6 0718 293A 4B5C",
        metadata
    ));
    assert!(search.matches("abcd 1234 ef56", metadata));
    assert!(!search.matches("ABCD 1234 FFFF", metadata));
}
