use super::service_url;

const FINGERPRINT: &str = "0123456789ABCDEF0123456789ABCDEF01234567";

#[test]
fn expands_kgpg_tokens_with_the_requested_case() -> Result<(), String> {
    let template = "https://keys.example.test/lookup?id8=$$ID8$$&id16=$$ID16$$&fpr=$$FPR$$&lower8=$$id8$$&lower16=$$id16$$&lowerfpr=$$fpr$$";

    let url = service_url(template, FINGERPRINT)?;

    assert_eq!(
        url,
        "https://keys.example.test/lookup?id8=01234567&id16=89ABCDEF01234567&fpr=0123456789ABCDEF0123456789ABCDEF01234567&lower8=01234567&lower16=89abcdef01234567&lowerfpr=0123456789abcdef0123456789abcdef01234567"
    );
    Ok(())
}

#[test]
fn normalizes_spaces_and_mixed_case_in_a_full_fingerprint() -> Result<(), String> {
    let fingerprint = " 0123 4567 89ab cDef 0123 4567 89AB CDEF 0123 4567 ";

    assert_eq!(
        service_url("https://keys.example.test/$$FPR$$", fingerprint)?,
        "https://keys.example.test/0123456789ABCDEF0123456789ABCDEF01234567"
    );
    Ok(())
}

#[test]
fn expands_every_occurrence_and_preserves_literal_url_parts() -> Result<(), String> {
    assert_eq!(
        service_url(
            "http://keys.example.test:8080/$$ID16$$/$$ID16$$?op=get&search=0x$$id16$$#result",
            FINGERPRINT,
        )?,
        "http://keys.example.test:8080/89ABCDEF01234567/89ABCDEF01234567?op=get&search=0x89abcdef01234567#result"
    );
    Ok(())
}

#[test]
fn supports_64_character_fingerprints() -> Result<(), String> {
    let fingerprint = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    assert_eq!(
        service_url(
            "https://keys.example.test/$$FPR$$?short=$$ID8$$&long=$$ID16$$&lower=$$fpr$$&lower8=$$id8$$&lower16=$$id16$$",
            fingerprint,
        )?,
        "https://keys.example.test/0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF?short=89ABCDEF&long=0123456789ABCDEF&lower=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef&lower8=89abcdef&lower16=0123456789abcdef"
    );
    Ok(())
}

#[test]
fn rejects_values_that_are_not_complete_hex_fingerprints() {
    for fingerprint in [
        "",
        " ",
        "89ABCDEF",
        "0123456789ABCDEF",
        "0123456789ABCDEF0123456789ABCDEF012345678",
        "0123456789ABCDEF0123456789ABCDEF01234567890",
        "0123456789ABCDEF0123456789ABCDEF0123456G",
        "0x0123456789ABCDEF0123456789ABCDEF0123456789",
        "0123456789ABCDEF0123456789ABCDEF0123456/",
    ] {
        assert!(
            service_url("https://keys.example.test/$$FPR$$", fingerprint).is_err(),
            "accepted invalid fingerprint: {fingerprint:?}"
        );
    }
}

#[test]
fn rejects_non_web_schemes_and_urls_without_hosts() {
    for template in [
        "file:///tmp/$$FPR$$",
        "ftp://keys.example.test/$$FPR$$",
        "hkp://keys.example.test/$$FPR$$",
        "javascript:alert('$$FPR$$')",
        "data:text/plain,$$FPR$$",
        "/lookup/$$FPR$$",
        "//keys.example.test/$$FPR$$",
        "https://",
        "http://?search=$$FPR$$",
        "https://#$$FPR$$",
    ] {
        assert!(
            service_url(template, FINGERPRINT).is_err(),
            "accepted invalid service URL: {template:?}"
        );
    }
}

#[test]
fn rejects_credentials_in_service_urls() {
    for template in [
        "https://user@keys.example.test/$$FPR$$",
        "https://user:password@keys.example.test/$$FPR$$",
        "https://:password@keys.example.test/$$FPR$$",
    ] {
        assert!(
            service_url(template, FINGERPRINT).is_err(),
            "accepted credentials in service URL: {template:?}"
        );
    }
}

#[test]
fn rejects_unknown_tokens_in_any_url_component() {
    for template in [
        "https://keys.example.test/$$UNKNOWN$$",
        "https://keys.example.test/$$FPR$$?search=$$ID32$$",
        "https://keys.example.test/$$Fpr$$",
        "https://keys.example.test/$$FPR$$#$$unknown$$",
    ] {
        assert!(
            service_url(template, FINGERPRINT).is_err(),
            "accepted unknown placeholder: {template:?}"
        );
    }
}
