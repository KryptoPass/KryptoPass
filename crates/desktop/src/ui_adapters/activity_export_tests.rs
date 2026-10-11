#![allow(clippy::unwrap_used)]

use super::*;
use kryptopass_core::{LoginContent, LoginHistory, TotpAlgorithm, TotpConfig};

struct ReportFile(std::path::PathBuf);

impl Drop for ReportFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn activity_export_preserves_all_events_and_exact_times_without_credentials() {
    let mut history = LoginHistory::default();
    let first_timestamp = 1_792_000_001;
    for offset in 0..35 {
        history.record_event(
            LoginHistoryEventKind::CopyPassword,
            first_timestamp + offset,
        );
    }
    let login = LoginContent {
        username: "fake-history-user@example.invalid".to_owned(),
        password: "fake-history-password-unique".to_owned(),
        url: "https://fake-history-site.invalid".to_owned(),
        urls: vec!["https://fake-history-site.invalid".to_owned()],
        notes: "fake-history-private-notes".to_owned(),
        totp: Some(TotpConfig {
            secret: "JBSWY3DPEHPK3PXP".to_owned(),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: Some("fake-history-issuer".to_owned()),
            account: Some("fake-history-account".to_owned()),
        }),
        history: Box::default(),
    };
    history
        .record_password("fake-legacy-password-unique".to_owned(), first_timestamp)
        .unwrap();
    history
        .record_version(&login, "fake-history-title", first_timestamp)
        .unwrap();

    let report = activity_report(&history).unwrap();
    let file = ReportFile(write_activity_report(&report).unwrap());
    assert_eq!(file.0.extension().unwrap(), "txt");
    let bytes = std::fs::read(&file.0).unwrap();
    assert_eq!(bytes, report.as_bytes());
    let exported = String::from_utf8(bytes).unwrap();
    let event_lines: Vec<_> = exported
        .lines()
        .filter(|line| line.contains("  Contraseña copiada"))
        .collect();
    assert_eq!(event_lines.len(), 35);
    for (index, line) in event_lines.iter().enumerate() {
        let (timestamp, label) = line.split_once("  ").unwrap();
        assert_eq!(label, "Contraseña copiada");
        assert_eq!(&timestamp[10..11], "T");
        assert_eq!(&timestamp[16..17], ":");
        assert!(timestamp[17..19].bytes().all(|byte| byte.is_ascii_digit()));
        let parsed =
            QDateTime::from_string(&QString::from(timestamp), cxx_qt_lib::DateFormat::ISODate)
                .unwrap();
        assert_eq!(
            parsed.to_secs_since_epoch(),
            i64::try_from(first_timestamp + 34 - index as u64).unwrap()
        );
    }
    for value in [
        login.username.as_str(),
        login.password.as_str(),
        login.url.as_str(),
        login.notes.as_str(),
        "JBSWY3DPEHPK3PXP",
        "fake-history-issuer",
        "fake-history-account",
        "fake-history-title",
        "fake-legacy-password-unique",
    ] {
        assert!(!exported.contains(value));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file.0).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
