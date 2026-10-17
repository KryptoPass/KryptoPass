//! Process-local restrictions applied before Qt and application workers start.

#[cfg(target_os = "linux")]
pub fn initialize() {
    // This changes only this process. It does not touch GnuPG Agent or system
    // policy, and denial must not prevent opening the application.
    // SAFETY: PR_SET_DUMPABLE takes a scalar boolean and no pointer arguments.
    let dumpable_result = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if dumpable_result != 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "Process dump restriction unavailable");
    }

    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is a writable rlimit and remains alive for the call.
    if unsafe { libc::getrlimit(libc::RLIMIT_CORE, &mut limit) } != 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "Could not read core dump limit");
        return;
    }
    limit.rlim_cur = 0;
    // Preserve the administrator's hard limit; never raise any limit.
    // SAFETY: `limit` is initialized and remains alive for the call.
    if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &limit) } != 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "Could not disable process core dumps");
    }
}

#[cfg(not(target_os = "linux"))]
pub fn initialize() {
    tracing::info!("Process dump restrictions are not implemented on this platform");
}

#[cfg(test)]
#[path = "../tests/unit/process_security.rs"]
mod tests;
