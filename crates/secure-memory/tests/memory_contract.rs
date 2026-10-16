// Contract tests treat failed setup and unexpected Result variants as assertion failures.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use kryptopass_secure_memory::{SensitiveBytes, SensitiveText, memory_diagnostics};
use std::{io::Write, process::Command};

#[test]
fn growth_preserves_content_and_spare_capacity_keeps_address() {
    let mut bytes = SensitiveBytes::with_capacity(16);
    bytes.extend_from_slice(b"owned-secret");
    let address = bytes.as_slice().as_ptr();
    let capacity = bytes.capacity();
    bytes.resize(capacity, 0x5a);
    assert_eq!(address, bytes.as_slice().as_ptr());
    bytes.push(0x6b);
    assert!(bytes.capacity() > capacity);
    assert_eq!(&bytes[..12], b"owned-secret");
    assert!(bytes[12..capacity].iter().all(|byte| *byte == 0x5a));
    assert_eq!(bytes[capacity], 0x6b);
}

#[test]
fn truncate_and_clear_wipe_initialized_storage() {
    let mut bytes = SensitiveBytes::from(&b"wipe-this-tail"[..]);
    let address = bytes.as_slice().as_ptr();
    let old_len = bytes.len();
    bytes.truncate(4);
    assert_eq!(bytes.as_slice(), b"wipe");
    bytes.resize(old_len, 0);
    assert_eq!(&bytes[..4], b"wipe");
    assert!(bytes[4..].iter().all(|byte| *byte == 0));
    bytes.clear();
    assert!(bytes.is_empty());
    bytes.resize(bytes.capacity(), 0);
    assert_eq!(address, bytes.as_slice().as_ptr());
    assert!(bytes.iter().all(|byte| *byte == 0));
}

#[test]
fn text_validates_utf8_and_debug_never_contains_content() {
    let marker = "sensitive-ñ-🦀";
    let text = SensitiveText::from_utf8(SensitiveBytes::from(marker.as_bytes())).unwrap();
    assert_eq!(text.as_str(), marker);
    assert!(!format!("{text:?}").contains(marker));
    assert!(!format!("{:?}", SensitiveBytes::from(marker.as_bytes())).contains(marker));
    let invalid = SensitiveBytes::from(&b"private-prefix\xff"[..]);
    let error = SensitiveText::from_utf8(invalid).unwrap_err();
    assert_eq!(error.valid_up_to(), 14);
    assert!(!format!("{error:?}").contains("private-prefix"));
    let mut text = text;
    assert_eq!(text.remove("sensitive-ñ-".len()), '🦀');
    text.push('🦀');
    assert_eq!(text.as_str(), marker);
}

#[test]
fn serde_roundtrip_preserves_text_and_bytes() {
    let text = SensitiveText::from("escaped \"secret\"\\\nñ🦀");
    let mut serialized = SensitiveBytes::new();
    serde_json::to_writer(&mut serialized, &text).unwrap();
    let decoded: SensitiveText = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(decoded, text);
    let bytes = SensitiveBytes::from(&[0, 1, 128, 255][..]);
    serialized.clear();
    serde_json::to_writer(&mut serialized, &bytes).unwrap();
    let decoded: SensitiveBytes = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(decoded, bytes);
}

#[test]
fn bounded_writer_rejects_entire_write_without_changing_content() {
    let mut bytes = SensitiveBytes::from(&b"abc"[..]);
    let address = bytes.as_slice().as_ptr();
    let mut writer = bytes.bounded_writer(5);
    writer.write_all(b"de").unwrap();
    let error = writer.write_all(b"private-value").unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(bytes.as_slice(), b"abcde");
    assert_eq!(address, bytes.as_slice().as_ptr());
}

fn isolated(case: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "isolated_process_contract", "--nocapture"])
        .env("KRYPTO_MEMORY_TEST_CASE", case)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated case {case} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn region_budget_falls_back_and_releases_all_reservations() {
    isolated("budget");
}

#[test]
fn concurrent_allocations_respect_budget_and_release_reservations() {
    isolated("concurrent");
}

#[cfg(target_os = "linux")]
#[test]
fn zero_memlock_limit_degrades_only_in_child() {
    let before = memory_diagnostics();
    isolated("zero-limit");
    let after = memory_diagnostics();
    assert_eq!(before.memlock_soft, after.memlock_soft);
    assert_eq!(before.memlock_hard, after.memlock_hard);
}

#[cfg(target_os = "linux")]
#[test]
fn reported_protections_match_kernel_mapping_flags() {
    isolated("kernel");
}

#[test]
fn isolated_process_contract() {
    let Ok(case) = std::env::var("KRYPTO_MEMORY_TEST_CASE") else {
        return;
    };
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // This process is a dedicated child; disabling its dumps cannot alter the parent.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) }, 0);
    }
    let initial = memory_diagnostics();
    assert_eq!(initial.dedicated_regions, 0);
    assert_eq!(initial.fallback_regions, 0);
    assert_eq!(initial.reserved_locked_bytes, 0);
    match case.as_str() {
        "budget" => {
            let mut regions: Vec<_> = (0..initial.dedicated_region_limit + 17)
                .map(|_| SensitiveBytes::from(&b"secret"[..]))
                .collect();
            let during = memory_diagnostics();
            assert!(during.dedicated_regions <= initial.dedicated_region_limit);
            assert!(during.fallback_regions >= 17);
            assert_eq!(
                during.dedicated_regions + during.fallback_regions,
                regions.len()
            );
            for region in &mut regions {
                region.push(0x7f);
                assert_eq!(region.as_slice(), b"secret\x7f");
            }
            drop(regions);
        }
        "concurrent" => {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(9));
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        let mut regions: Vec<_> = (0..40)
                            .map(|_| SensitiveBytes::from(&b"thread-secret"[..]))
                            .collect();
                        barrier.wait();
                        barrier.wait();
                        for region in &mut regions {
                            region.resize(region.capacity() + 1, 0x42);
                            assert_eq!(&region[..13], b"thread-secret");
                        }
                    })
                })
                .collect();
            barrier.wait();
            let during = memory_diagnostics();
            assert_eq!(during.dedicated_regions + during.fallback_regions, 320);
            assert!(during.dedicated_regions <= initial.dedicated_region_limit);
            assert!(during.reserved_locked_bytes <= initial.lock_budget_bytes);
            barrier.wait();
            for thread in threads {
                thread.join().unwrap();
            }
        }
        #[cfg(target_os = "linux")]
        "zero-limit" => {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            // Limit mutation is confined to this disposable test process.
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &limit) }, 0);
            assert_eq!(memory_diagnostics().memlock_soft, Some(0));
            let mut bytes = SensitiveBytes::from(&b"still-usable"[..]);
            assert!(!bytes.protection_status().locked);
            bytes.resize(bytes.capacity() + 1, 0);
            assert_eq!(&bytes[..12], b"still-usable");
            assert!(!bytes.protection_status().locked);
            assert_eq!(memory_diagnostics().reserved_locked_bytes, 0);
        }
        #[cfg(target_os = "linux")]
        "kernel" => {
            let bytes = SensitiveBytes::from(&b"kernel-flags"[..]);
            let address = bytes.as_slice().as_ptr() as usize;
            let status = bytes.protection_status();
            let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
            let mut selected = false;
            let mut flags = None;
            for line in smaps.lines() {
                if let Some(range) = line.split_whitespace().next()
                    && let Some((start, end)) = range.split_once('-')
                    && let (Ok(start), Ok(end)) = (
                        usize::from_str_radix(start, 16),
                        usize::from_str_radix(end, 16),
                    )
                {
                    selected = start <= address && address < end;
                }
                if selected && let Some(value) = line.strip_prefix("VmFlags:") {
                    flags = Some(value.split_whitespace().collect::<Vec<_>>());
                    break;
                }
            }
            let flags = flags.expect("live region must have a kernel mapping");
            if status.excluded_from_dumps {
                assert!(flags.contains(&"dd"));
            }
            if status.locked {
                assert!(flags.contains(&"lo"));
            }
        }
        _ => panic!("unknown child test case"),
    }
    let final_state = memory_diagnostics();
    assert_eq!(final_state.dedicated_regions, 0);
    assert_eq!(final_state.fallback_regions, 0);
    assert_eq!(final_state.reserved_locked_bytes, 0);
    assert_eq!(final_state.release_failures, 0);
}
