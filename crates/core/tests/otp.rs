#![allow(clippy::unwrap_used)]

use kryptopass_core::{
    CONTENT_SCHEMA_VERSION, MAX_TOTP_EXTENSION_BYTES, MAX_TOTP_EXTENSIONS, SecretContent,
    TotpContent, TotpUriParameter, decode_content, encode_content, format_totp_input,
    generate_totp, parse_totp_input,
};
use serde_json::Value;

fn parse_uri(uri: &str) -> kryptopass_core::TotpConfig {
    parse_totp_input(uri).unwrap().unwrap()
}

fn base_uri(period: u64) -> String {
    format!("otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&period={period}")
}

#[test]
fn standard_and_unknown_uri_parameters_survive_parse_format_and_encrypted_content_roundtrips() {
    let input = "otpauth://totp/Lab%C3%A9%3Aops%2Bnight%20shift?secret=JBSWY3DPEHPK3PXP&issuer=Lab%C3%A9&algorithm=SHA512&digits=8&period=86400&image=https%3A%2F%2Fcdn.example.invalid%2Ficon%3Fsize%3Dsmall&vendor=alpha%2Bbeta%20gamma&vendor%20key=space%20preserved&%23fragment=hash%20preserved&caf%C3%A9=cr%C3%A8me";
    let config = parse_uri(input);
    assert_eq!(
        config.original_label.as_deref(),
        Some("Lab%C3%A9%3Aops%2Bnight%20shift")
    );
    assert_eq!(config.issuer.as_deref(), Some("Labé"));
    assert_eq!(config.account.as_deref(), Some("ops+night shift"));
    assert!(config.algorithm == kryptopass_core::TotpAlgorithm::Sha512);
    assert_eq!(config.digits, 8);
    assert_eq!(config.period, 86_400);
    assert!(
        config.extensions
            == vec![
                TotpUriParameter {
                    key: "image".to_owned(),
                    value: "https://cdn.example.invalid/icon?size=small".to_owned(),
                },
                TotpUriParameter {
                    key: "vendor".to_owned(),
                    value: "alpha+beta gamma".to_owned(),
                },
                TotpUriParameter {
                    key: "vendor key".to_owned(),
                    value: "space preserved".to_owned(),
                },
                TotpUriParameter {
                    key: "#fragment".to_owned(),
                    value: "hash preserved".to_owned(),
                },
                TotpUriParameter {
                    key: "café".to_owned(),
                    value: "crème".to_owned(),
                },
            ]
    );

    let formatted = format_totp_input(&config).unwrap();
    assert!(formatted.starts_with("otpauth://totp/Lab%C3%A9%3Aops%2Bnight%20shift?"));
    let reparsed = parse_uri(&formatted);
    assert!(reparsed == config);

    let encrypted_payload = encode_content(&SecretContent::Totp(TotpContent {
        config: reparsed,
        notes: "Authenticator notes".to_owned(),
    }))
    .unwrap();
    let stored: Value = serde_json::from_slice(encrypted_payload.as_slice()).unwrap();
    assert_eq!(stored["schemaVersion"], CONTENT_SCHEMA_VERSION);
    assert_eq!(
        stored["fields"]["config"]["originalLabel"],
        "Lab%C3%A9%3Aops%2Bnight%20shift"
    );
    assert_eq!(stored["fields"]["config"]["extensions"][0]["key"], "image");

    let decoded = decode_content(encrypted_payload.as_slice()).unwrap();
    let SecretContent::Totp(decoded) = &decoded else {
        panic!("expected standalone TOTP content");
    };
    let formatted_again = format_totp_input(&decoded.config).unwrap();
    assert!(parse_uri(&formatted_again) == decoded.config);
}

#[test]
fn every_positive_u64_period_is_supported_but_zero_and_overflow_fail() {
    for period in [1, 86_400, u64::MAX] {
        let config = parse_uri(&base_uri(period));
        assert_eq!(config.period, period);
        assert!(format_totp_input(&config).is_ok());
    }
    assert!(parse_totp_input(&base_uri(0)).is_err(), "accepted period 0");
    assert!(
        parse_totp_input(
            "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&period=18446744073709551616"
        )
        .is_err()
    );
    let config = parse_uri(&base_uri(2));
    assert!(generate_totp(&config, u64::MAX).is_err());
}

#[test]
fn extension_count_and_value_bounds_are_enforced_without_dropping_valid_extensions() {
    let mut input = base_uri(30);
    input.push_str(&format!("&x-size={}", "v".repeat(MAX_TOTP_EXTENSION_BYTES)));
    let config = parse_uri(&input);
    assert_eq!(config.extensions[0].value.len(), MAX_TOTP_EXTENSION_BYTES);
    assert!(format_totp_input(&config).is_ok());

    let mut too_long = base_uri(30);
    too_long.push_str(&format!(
        "&x-size={}",
        "v".repeat(MAX_TOTP_EXTENSION_BYTES + 1)
    ));
    assert!(parse_totp_input(&too_long).is_err());

    let mut too_many = base_uri(30);
    for index in 0..=MAX_TOTP_EXTENSIONS {
        too_many.push_str(&format!("&x{index}=v"));
    }
    assert!(parse_totp_input(&too_many).is_err());
}

#[test]
fn duplicate_parameters_invalid_extension_names_and_invalid_uri_characters_are_rejected() {
    for uri in [
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&secret=JBSWY3DPEHPK3PXP",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&s%65cret=JBSWY3DPEHPK3PXP",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&image=one&image=two",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&digits=7",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&%01=value",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&%FF=value",
        "otpauth://totp/Example%3Aada?secret=JBSWY3DPEHPK3PXP&=value",
        "otpauth://totp/Example%ZZada?secret=JBSWY3DPEHPK3PXP",
    ] {
        assert!(parse_totp_input(uri).is_err(), "accepted invalid URI {uri}");
    }

    let mut duplicate_config = kryptopass_core::TotpConfig::default();
    duplicate_config.secret = "JBSWY3DPEHPK3PXP".to_owned();
    duplicate_config.extensions = vec![
        TotpUriParameter {
            key: "vendor".to_owned(),
            value: "first".to_owned(),
        },
        TotpUriParameter {
            key: "vendor".to_owned(),
            value: "second".to_owned(),
        },
    ];
    assert!(format_totp_input(&duplicate_config).is_err());
}

#[test]
fn hotp_is_reported_as_unsupported_and_is_never_reinterpreted_as_totp() {
    let error = match parse_totp_input("otpauth://hotp/Example%3Aada?secret=JBSWY3DPEHPK3PXP") {
        Ok(_) => panic!("HOTP input must be rejected"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("HOTP"));
}
