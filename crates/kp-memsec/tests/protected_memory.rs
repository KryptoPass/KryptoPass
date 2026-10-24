// Failed setup and unexpected Result variants are contract failures in this suite.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use kryptopass_secure_memory::{ProtectedBytes, SensitiveBytes, memory_diagnostics};
use std::{io, io::Write, process::Command};

fn isolated(case: &str) {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "isolated_protected_memory_contract",
            "--nocapture",
        ])
        .env("KRYPTO_PROTECTED_MEMORY_TEST_CASE", case)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated case {case} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if line.starts_with("PROTECTION_UNAVAILABLE:") {
            eprintln!("{case}: {line}");
        }
    }
}

fn strict_region(capacity: usize) -> Option<ProtectedBytes> {
    match ProtectedBytes::try_with_capacity(capacity) {
        Ok(bytes) => {
            let status = bytes.protection_status();
            assert!(status.dedicated);
            assert!(status.locked);
            if cfg!(target_os = "linux") {
                assert!(status.excluded_from_dumps);
            }
            assert!(bytes.is_empty());
            assert!(bytes.capacity() >= capacity.max(1));
            Some(bytes)
        }
        Err(error) => {
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            let probe = SensitiveBytes::try_with_capacity(capacity.max(1)).unwrap();
            let status = probe.protection_status();
            assert!(
                !status.dedicated
                    || !status.locked
                    || (cfg!(target_os = "linux") && !status.excluded_from_dumps),
                "strict admission rejected a region although all required protections work"
            );
            eprintln!("PROTECTION_UNAVAILABLE: {status:?}");
            None
        }
    }
}

fn assert_no_reservations() {
    let diagnostics = memory_diagnostics();
    assert_eq!(diagnostics.dedicated_regions, 0);
    assert_eq!(diagnostics.fallback_regions, 0);
    assert_eq!(diagnostics.reserved_locked_bytes, 0);
    assert_eq!(diagnostics.release_failures, 0);
}

fn allocation_error(capacity: usize) -> io::Error {
    match ProtectedBytes::try_with_capacity(capacity) {
        Ok(_) => panic!("capacity {capacity} unexpectedly admitted"),
        Err(error) => error,
    }
}

#[test]
fn required_protections_exist_before_the_first_write() {
    isolated("admission");
}

#[test]
fn writes_keep_the_reservation_and_reject_oversized_input_atomically() {
    isolated("writes");
}

#[test]
fn clear_keeps_capacity_and_allows_zeroed_reuse() {
    isolated("clear");
}

#[test]
fn zero_capacity_still_requires_a_protected_region() {
    isolated("zero-capacity");
}

#[test]
fn unrepresentable_capacities_return_recoverable_errors() {
    isolated("extreme-capacity");
}

#[test]
fn exhausted_region_budget_cannot_admit_heap_fallbacks() {
    isolated("region-budget");
}

#[cfg(target_os = "linux")]
#[test]
fn zero_memlock_limit_rejects_reservations_without_changing_the_parent() {
    let before = memory_diagnostics();
    isolated("zero-memlock");
    let after = memory_diagnostics();
    assert_eq!(before.memlock_soft, after.memlock_soft);
    assert_eq!(before.memlock_hard, after.memlock_hard);
}

#[cfg(target_os = "linux")]
fn kernel_flags(address: usize) -> Vec<String> {
    let smaps = std::fs::read_to_string("/proc/self/smaps").unwrap();
    let mut selected = false;
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
        if selected && let Some(flags) = line.strip_prefix("VmFlags:") {
            return flags.split_whitespace().map(str::to_owned).collect();
        }
    }
    panic!("the live protected allocation has no kernel mapping");
}

fn admission_contract() {
    let Some(mut bytes) = strict_region(32) else {
        return;
    };
    assert_eq!(bytes.len(), 0);
    let status = bytes.protection_status();
    #[cfg(target_os = "linux")]
    {
        let flags = kernel_flags(bytes.as_slice().as_ptr() as usize);
        assert!(flags.iter().any(|flag| flag == "lo"));
        assert!(flags.iter().any(|flag| flag == "dd"));
    }
    bytes.try_extend_from_slice(b"synthetic-content").unwrap();
    assert_eq!(bytes.as_slice(), b"synthetic-content");
    assert_eq!(bytes.protection_status(), status);
}

fn writes_contract() {
    let Some(mut bytes) = strict_region(16) else {
        return;
    };
    bytes.try_extend_from_slice(b"original").unwrap();
    let address = bytes.as_slice().as_ptr();
    let capacity = bytes.capacity();
    let status = bytes.protection_status();
    let too_large = vec![0x5a; capacity - bytes.len() + 1];
    let error = bytes.try_extend_from_slice(&too_large).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
    assert_eq!(bytes.as_slice(), b"original");
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(bytes.as_slice().as_ptr(), address);
    let error = bytes.write(&too_large).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
    assert_eq!(bytes.as_slice(), b"original");
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(bytes.as_slice().as_ptr(), address);

    bytes
        .write_all(&vec![0x6b; capacity - bytes.len()])
        .unwrap();
    assert_eq!(bytes.len(), capacity);
    assert_eq!(&bytes.as_slice()[..8], b"original");
    assert!(bytes.as_slice()[8..].iter().all(|byte| *byte == 0x6b));
    let error = bytes.write(b"x").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
    assert_eq!(bytes.len(), capacity);
    assert_eq!(&bytes.as_slice()[..8], b"original");
    assert!(bytes.as_slice()[8..].iter().all(|byte| *byte == 0x6b));
    assert_eq!(bytes.write(b"").unwrap(), 0);
    bytes.flush().unwrap();
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(bytes.as_slice().as_ptr(), address);
    assert_eq!(bytes.protection_status(), status);
}

fn clear_contract() {
    let Some(mut bytes) = strict_region(16) else {
        return;
    };
    bytes.fill_capacity();
    let capacity = bytes.capacity();
    let address = bytes.as_slice().as_ptr() as usize;
    assert_eq!(bytes.len(), capacity);
    assert!(bytes.as_slice().iter().all(|byte| *byte == 0));
    bytes.as_mut_slice().fill(0xa5);

    #[cfg(target_os = "linux")]
    let memory = std::fs::File::open("/proc/self/mem").unwrap();
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        let mut observed = vec![0; capacity];
        memory.read_exact_at(&mut observed, address as u64).unwrap();
        assert!(observed.iter().all(|byte| *byte == 0xa5));
    }

    bytes.clear();
    assert!(bytes.is_empty());
    assert_eq!(bytes.len(), 0);
    assert_eq!(bytes.capacity(), capacity);
    assert_eq!(bytes.as_slice().as_ptr() as usize, address);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        // Read the live reservation before fill_capacity can initialize it again.
        let mut observed = vec![0xa5; capacity];
        memory.read_exact_at(&mut observed, address as u64).unwrap();
        assert!(observed.iter().all(|byte| *byte == 0));
    }

    bytes.try_extend_from_slice(b"new").unwrap();
    assert_eq!(bytes.as_slice(), b"new");
    assert_eq!(bytes.as_slice().as_ptr() as usize, address);
    assert_eq!(bytes.capacity(), capacity);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileExt;
        let mut observed = vec![0xa5; capacity];
        memory.read_exact_at(&mut observed, address as u64).unwrap();
        assert_eq!(&observed[..3], b"new");
        assert!(observed[3..].iter().all(|byte| *byte == 0));
    }
    bytes.clear();
    bytes.fill_capacity();
    assert_eq!(bytes.len(), capacity);
    assert!(bytes.as_slice().iter().all(|byte| *byte == 0));
    assert_eq!(bytes.as_slice().as_ptr() as usize, address);
}

#[test]
fn isolated_protected_memory_contract() {
    let Ok(case) = std::env::var("KRYPTO_PROTECTED_MEMORY_TEST_CASE") else {
        return;
    };
    #[cfg(unix)]
    {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        // Only the disposable child loses dump permission and mutable resource limits.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) }, 0);
    }
    assert_no_reservations();
    match case.as_str() {
        "admission" => admission_contract(),
        "writes" => writes_contract(),
        "clear" => clear_contract(),
        "zero-capacity" => {
            if let Some(mut bytes) = strict_region(0) {
                bytes.write_all(b"x").unwrap();
                assert_eq!(bytes.as_slice(), b"x");
            }
        }
        "extreme-capacity" => {
            for capacity in [usize::MAX, usize::MAX - 1, isize::MAX as usize + 1] {
                assert_eq!(
                    allocation_error(capacity).kind(),
                    io::ErrorKind::InvalidInput
                );
                assert_no_reservations();
            }
        }
        "region-budget" => {
            let limit = memory_diagnostics().dedicated_region_limit;
            let regions: Vec<_> = (0..=limit)
                .map(|_| SensitiveBytes::try_with_capacity(1).unwrap())
                .collect();
            let before = memory_diagnostics();
            assert!(before.fallback_regions > 0);
            assert_eq!(allocation_error(1).kind(), io::ErrorKind::PermissionDenied);
            let after = memory_diagnostics();
            assert_eq!(after.dedicated_regions, before.dedicated_regions);
            assert_eq!(after.fallback_regions, before.fallback_regions);
            assert_eq!(after.reserved_locked_bytes, before.reserved_locked_bytes);
            drop(regions);
        }
        #[cfg(target_os = "linux")]
        "zero-memlock" => {
            let limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            // The parent process keeps its original memlock limits.
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &limit) }, 0);
            assert_eq!(memory_diagnostics().memlock_soft, Some(0));
            for capacity in [0, 1, memory_diagnostics().page_size + 1] {
                assert_eq!(
                    allocation_error(capacity).kind(),
                    io::ErrorKind::PermissionDenied
                );
                assert_no_reservations();
            }
        }
        _ => panic!("unknown child test case"),
    }
    assert_no_reservations();
}
