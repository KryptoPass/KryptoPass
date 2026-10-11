//! Time-based one-time password configuration and generation.

use anyhow::{Context, Result, bail, ensure};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// Maximum encoded length of a TOTP shared secret.
pub const MAX_TOTP_SECRET_BYTES: usize = 8 * 1024;
/// Maximum byte length of either optional TOTP label.
pub const MAX_TOTP_LABEL_BYTES: usize = 8 * 1024;

/// Hash algorithm used to calculate a TOTP value.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl TotpAlgorithm {
    const fn library_algorithm(self) -> Algorithm {
        match self {
            Self::Sha1 => Algorithm::SHA1,
            Self::Sha256 => Algorithm::SHA256,
            Self::Sha512 => Algorithm::SHA512,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        if value.eq_ignore_ascii_case("sha1") {
            Ok(Self::Sha1)
        } else if value.eq_ignore_ascii_case("sha256") {
            Ok(Self::Sha256)
        } else if value.eq_ignore_ascii_case("sha512") {
            Ok(Self::Sha512)
        } else {
            bail!("unsupported TOTP algorithm")
        }
    }
}

/// TOTP parameters stored inside the encrypted login payload.
///
/// `secret` is the canonical uppercase Base32 representation without padding.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TotpConfig {
    pub secret: String,
    #[zeroize(skip)]
    pub algorithm: TotpAlgorithm,
    #[zeroize(skip)]
    pub digits: u8,
    #[zeroize(skip)]
    pub period: u64,
    pub issuer: Option<String>,
    pub account: Option<String>,
}

/// The current and next TOTP code, plus their step timing.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct TotpSnapshot {
    pub current: String,
    pub next: String,
    pub period: u64,
    pub remaining: u64,
}

/// Parses a Base32 secret or a strict `otpauth://totp/...` URI.
///
/// Empty input maps to `None`. HOTP URIs, duplicate parameters, unknown
/// parameters, malformed percent encoding, and unsupported values are errors.
pub fn parse_totp_input(input: &str) -> Result<Option<TotpConfig>> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }

    ensure!(
        input.len() <= MAX_TOTP_SECRET_BYTES + MAX_TOTP_LABEL_BYTES * 3 + 256,
        "TOTP input exceeds the maximum size"
    );

    let config = if let Some(uri) = input.strip_prefix("otpauth://") {
        parse_otpauth_uri(uri)?
    } else {
        TotpConfig {
            secret: canonical_base32_secret(input)?,
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: None,
            account: None,
        }
    };

    validate_totp_config(&config)?;
    Ok(Some(config))
}

/// Formats a config as a canonical otpauth URI held in a zeroizing buffer.
pub fn format_totp_input(config: &TotpConfig) -> Result<Zeroizing<String>> {
    validate_totp_config(config)?;

    let secret = parse_base32_secret(&config.secret)?;
    let canonical_secret = Zeroizing::new(encoded_secret(&secret));
    let label_capacity = config
        .issuer
        .as_ref()
        .map_or(0, String::len)
        .saturating_add(config.account.as_ref().map_or(0, String::len));
    let mut uri = Zeroizing::new(String::with_capacity(
        canonical_secret
            .len()
            .saturating_add(label_capacity.saturating_mul(3))
            .saturating_add(128),
    ));

    uri.push_str("otpauth://totp/");
    if let Some(issuer) = config.issuer.as_deref() {
        push_percent_encoded(&mut uri, issuer);
        uri.push_str("%3A");
    }
    if let Some(account) = config.account.as_deref() {
        push_percent_encoded(&mut uri, account);
    }

    uri.push_str("?secret=");
    uri.push_str(&canonical_secret);
    if let Some(issuer) = config.issuer.as_deref() {
        uri.push_str("&issuer=");
        push_percent_encoded(&mut uri, issuer);
    }
    uri.push_str("&algorithm=");
    uri.push_str(config.algorithm.as_str());
    uri.push_str("&digits=");
    uri.push_str(&config.digits.to_string());
    uri.push_str("&period=");
    uri.push_str(&config.period.to_string());

    Ok(uri)
}

/// Generates the current and next TOTP codes at a Unix timestamp in seconds.
pub fn generate_totp(config: &TotpConfig, unix_seconds: u64) -> Result<TotpSnapshot> {
    validate_totp_config(config)?;
    let totp = build_totp(config)?;
    let remainder = unix_seconds % config.period;
    let remaining = config.period - remainder;
    let next_time = unix_seconds
        .checked_add(remaining)
        .context("next TOTP time step exceeds the supported Unix timestamp")?;
    let mut current = Zeroizing::new(totp.generate(unix_seconds).to_string());
    let mut next = Zeroizing::new(totp.generate(next_time).to_string());

    Ok(TotpSnapshot {
        current: std::mem::take(&mut *current),
        next: std::mem::take(&mut *next),
        period: config.period,
        remaining,
    })
}

pub(crate) fn validate_totp_config(config: &TotpConfig) -> Result<()> {
    ensure!(
        config.secret.len() <= MAX_TOTP_SECRET_BYTES,
        "TOTP secret exceeds the maximum size"
    );
    ensure!(matches!(config.digits, 6 | 8), "TOTP digits must be 6 or 8");
    ensure!(
        (15..=120).contains(&config.period),
        "TOTP period must be between 15 and 120 seconds"
    );
    validate_label("issuer", config.issuer.as_deref())?;
    validate_label("account", config.account.as_deref())?;
    let secret = parse_base32_secret(&config.secret)?;
    let canonical_secret = Zeroizing::new(encoded_secret(&secret));
    ensure!(
        config.secret == *canonical_secret,
        "TOTP secret must use canonical Base32"
    );
    Ok(())
}

fn validate_label(name: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        ensure!(
            !value.is_empty(),
            "TOTP {name} cannot be empty when provided"
        );
        ensure!(
            value.len() <= MAX_TOTP_LABEL_BYTES,
            "TOTP {name} exceeds the maximum size"
        );
        ensure!(
            !value.contains(':') && !value.chars().any(char::is_control),
            "TOTP {name} contains an unsupported character"
        );
    }
    Ok(())
}

fn build_totp(config: &TotpConfig) -> Result<TOTP> {
    let secret = parse_base32_secret(&config.secret)?;
    Ok(TOTP::new_unchecked(
        config.algorithm.library_algorithm(),
        usize::from(config.digits),
        0,
        config.period,
        secret.to_bytes().context("invalid TOTP Base32 secret")?,
    ))
}

fn parse_base32_secret(value: &str) -> Result<Secret> {
    let mut value = canonicalize_base32(value)?;
    Secret::Encoded(std::mem::take(&mut *value))
        .to_raw()
        .context("invalid TOTP Base32 secret")
}

fn encoded_secret(secret: &Secret) -> String {
    match &mut secret.to_encoded() {
        Secret::Encoded(encoded) => std::mem::take(encoded),
        Secret::Raw(_) => unreachable!("to_encoded always returns an encoded secret"),
    }
}

fn canonical_base32_secret(value: &str) -> Result<String> {
    let secret = parse_base32_secret(value)?;
    let mut canonical = Zeroizing::new(encoded_secret(&secret));
    Ok(std::mem::take(&mut *canonical))
}

fn canonicalize_base32(value: &str) -> Result<Zeroizing<String>> {
    ensure!(!value.is_empty(), "TOTP Base32 secret is empty");
    ensure!(
        value.len() <= MAX_TOTP_SECRET_BYTES,
        "TOTP secret exceeds the maximum size"
    );

    let padding_start = value.find('=').unwrap_or(value.len());
    let encoded = &value[..padding_start];
    let padding = &value[padding_start..];
    ensure!(
        padding.bytes().all(|byte| byte == b'='),
        "invalid TOTP Base32 padding"
    );
    ensure!(
        encoded.bytes().all(|byte| byte.is_ascii_alphanumeric()
            && (byte.is_ascii_digit() && (b'2'..=b'7').contains(&byte)
                || byte.is_ascii_alphabetic())),
        "invalid TOTP Base32 secret"
    );

    let expected_padding = match encoded.len() % 8 {
        0 => 0,
        2 => 6,
        4 => 4,
        5 => 3,
        7 => 1,
        _ => bail!("invalid TOTP Base32 length"),
    };
    ensure!(
        padding.is_empty() || (value.len() % 8 == 0 && padding.len() == expected_padding),
        "invalid TOTP Base32 padding"
    );
    ensure!(
        value.len() - padding.len() > 0,
        "TOTP Base32 secret is empty"
    );

    Ok(Zeroizing::new(encoded.to_ascii_uppercase()))
}

fn parse_otpauth_uri(uri: &str) -> Result<TotpConfig> {
    ensure!(
        !uri.contains('#') && !uri.contains('\\'),
        "invalid TOTP URI"
    );
    let (authority_path, raw_query) = uri
        .split_once('?')
        .context("TOTP URI is missing query parameters")?;
    ensure!(
        authority_path.starts_with("totp/") && authority_path.matches('/').count() == 1,
        "only otpauth://totp URIs with one label path segment are supported"
    );

    let raw_label = &authority_path["totp/".len()..];
    validate_percent_encoding(raw_label)?;
    let decoded_label = percent_decode_str(raw_label)
        .decode_utf8()
        .context("TOTP URI label is not valid UTF-8")?;
    let (path_issuer, account) = parse_label(&decoded_label)?;

    ensure!(!raw_query.is_empty(), "TOTP URI query is empty");
    validate_query(raw_query)?;

    let mut secret: Option<Zeroizing<String>> = None;
    let mut algorithm: Option<Zeroizing<String>> = None;
    let mut digits: Option<Zeroizing<String>> = None;
    let mut period: Option<Zeroizing<String>> = None;
    let mut issuer: Option<Zeroizing<String>> = None;
    for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
        let value = Zeroizing::new(value.into_owned());
        match key.as_ref() {
            "secret" => set_once(&mut secret, value, "secret")?,
            "issuer" => {
                ensure!(!value.is_empty(), "TOTP URI issuer is empty");
                set_once(&mut issuer, value, "issuer")?;
            }
            "algorithm" => set_once(&mut algorithm, value, "algorithm")?,
            "digits" => set_once(&mut digits, value, "digits")?,
            "period" => set_once(&mut period, value, "period")?,
            _ => bail!("unsupported TOTP URI parameter"),
        }
    }

    let secret_value = secret.as_ref().context("TOTP URI is missing secret")?;
    let mut secret = Zeroizing::new(canonical_base32_secret(secret_value)?);
    let algorithm = algorithm
        .as_ref()
        .map(|value| TotpAlgorithm::parse(value))
        .transpose()?
        .unwrap_or(TotpAlgorithm::Sha1);
    let digits = digits
        .as_ref()
        .map(|value| parse_digits(value))
        .transpose()?
        .unwrap_or(6);
    let period = period
        .as_ref()
        .map(|value| parse_period(value))
        .transpose()?
        .unwrap_or(30);

    if let (Some(path_issuer), Some(query_issuer)) = (path_issuer.as_deref(), issuer.as_ref()) {
        ensure!(
            path_issuer == query_issuer.as_str(),
            "TOTP URI issuer values do not match"
        );
    }
    let issuer = match issuer {
        Some(mut value) => Some(std::mem::take(&mut *value)),
        None => path_issuer,
    };
    Ok(TotpConfig {
        secret: std::mem::take(&mut *secret),
        algorithm,
        digits,
        period,
        issuer,
        account,
    })
}

fn parse_label(label: &str) -> Result<(Option<String>, Option<String>)> {
    if let Some((issuer, account)) = label.split_once(':') {
        ensure!(!issuer.is_empty(), "TOTP URI label issuer is empty");
        Ok((Some(issuer.to_owned()), nonempty(account)))
    } else {
        Ok((None, nonempty(label)))
    }
}

fn validate_query(raw_query: &str) -> Result<()> {
    for pair in raw_query.split('&') {
        ensure!(!pair.is_empty(), "TOTP URI contains an empty parameter");
        let (key, value) = pair
            .split_once('=')
            .context("TOTP URI parameter is missing a value")?;
        ensure!(
            !key.is_empty() && !key.contains('%') && key.is_ascii(),
            "invalid TOTP URI parameter name"
        );
        validate_percent_encoding(value)?;
        let decoded = percent_decode_str(value)
            .decode_utf8()
            .context("TOTP URI parameter is not valid UTF-8")?;
        let _decoded = Zeroizing::new(decoded.into_owned());
    }
    Ok(())
}

fn validate_percent_encoding(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            ensure!(
                index + 2 < bytes.len()
                    && bytes[index + 1].is_ascii_hexdigit()
                    && bytes[index + 2].is_ascii_hexdigit(),
                "malformed percent encoding in TOTP URI"
            );
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn push_percent_encoded(output: &mut String, value: &str) {
    output.push_str(&utf8_percent_encode(value, NON_ALPHANUMERIC).to_string());
}

fn set_once<T>(target: &mut Option<T>, value: T, name: &str) -> Result<()> {
    ensure!(target.is_none(), "duplicate TOTP URI {name} parameter");
    *target = Some(value);
    Ok(())
}

fn parse_digits(value: &str) -> Result<u8> {
    match value {
        "6" => Ok(6),
        "8" => Ok(8),
        _ => bail!("TOTP digits must be 6 or 8"),
    }
}

fn parse_period(value: &str) -> Result<u64> {
    ensure!(
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
        "invalid TOTP period"
    );
    let period = value.parse::<u64>().context("invalid TOTP period")?;
    ensure!(
        (15..=120).contains(&period),
        "TOTP period must be between 15 and 120 seconds"
    );
    Ok(period)
}

fn nonempty(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}
