#![cfg(target_os = "linux")]
// A failed child launch or test executable lookup must fail this test.
#![allow(clippy::unwrap_used)]

use super::initialize;

fn core_limit() -> libc::rlimit {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: limit is valid writable storage for the requested resource limit.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) }, 0);
    limit
}

fn dumpable() -> libc::c_int {
    // SAFETY: PR_GET_DUMPABLE accepts no pointer arguments.
    let value = unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
    assert!(value >= 0);
    value
}

#[test]
fn restrictions_are_process_local_and_preserve_core_hard_limit() {
    let before = core_limit();
    let before_dumpable = dumpable();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process_security::tests::process_security_child",
            "--nocapture",
        ])
        .env("KRYPTO_PROCESS_SECURITY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child security contract failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let after = core_limit();
    assert_eq!(after.rlim_cur, before.rlim_cur);
    assert_eq!(after.rlim_max, before.rlim_max);
    assert_eq!(dumpable(), before_dumpable);
}

#[test]
fn process_security_child() {
    if std::env::var_os("KRYPTO_PROCESS_SECURITY_CHILD").is_none() {
        return;
    }
    let inherited = core_limit();
    // SAFETY: PR_SET_DUMPABLE receives a scalar; only this disposable child changes.
    assert_eq!(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 1, 0, 0, 0) }, 0);
    assert_eq!(dumpable(), 1);
    let baseline = libc::rlimit {
        rlim_cur: inherited.rlim_max.min(1),
        rlim_max: inherited.rlim_max,
    };
    // SAFETY: baseline is initialized and its lifetime covers this call.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_CORE, &baseline) }, 0);
    for _ in 0..2 {
        initialize();
        let hardened = core_limit();
        assert_eq!(hardened.rlim_cur, 0);
        assert_eq!(hardened.rlim_max, inherited.rlim_max);
        assert_eq!(dumpable(), 0);
    }
}
