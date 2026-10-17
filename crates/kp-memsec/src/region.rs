use std::{
    io,
    ptr::NonNull,
    sync::{Mutex, OnceLock},
};
use zeroize::Zeroize;

const MAX_REGIONS: usize = 256;
const MAX_LOCKED: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProtectionStatus {
    pub dedicated: bool,
    pub locked: bool,
    pub excluded_from_dumps: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryDiagnostics {
    pub page_size: usize,
    pub memlock_soft: Option<u64>,
    pub memlock_hard: Option<u64>,
    pub observed_locked_bytes: Option<usize>,
    pub observed_mapping_count: Option<usize>,
    pub max_map_count: Option<usize>,
    pub dedicated_region_limit: usize,
    pub lock_budget_bytes: usize,
    pub effective_lock_budget_bytes: usize,
    pub dedicated_regions: usize,
    pub reserved_locked_bytes: usize,
    pub fallback_regions: usize,
    pub release_failures: usize,
}

#[derive(Default)]
struct Budget {
    regions: usize,
    locked: usize,
    fallback: usize,
    failures: usize,
}
static BUDGET: Mutex<Budget> = Mutex::new(Budget {
    regions: 0,
    locked: 0,
    fallback: 0,
    failures: 0,
});
fn budget() -> std::sync::MutexGuard<'static, Budget> {
    BUDGET.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn page_size() -> usize {
    static PAGE: OnceLock<usize> = OnceLock::new();
    *PAGE.get_or_init(|| {
        #[cfg(unix)]
        {
            // sysconf has no pointer arguments; nonpositive results mean unavailable.
            let value = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            if value > 0 { value as usize } else { 4096 }
        }
        #[cfg(windows)]
        {
            let mut info = std::mem::MaybeUninit::uninit();
            unsafe {
                windows_sys::Win32::System::SystemInformation::GetSystemInfo(info.as_mut_ptr());
                info.assume_init().dwPageSize as usize
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            4096
        }
    })
}

fn limits() -> (Option<u64>, Option<u64>) {
    #[cfg(unix)]
    {
        let mut value = std::mem::MaybeUninit::<libc::rlimit>::uninit();
        // getrlimit initializes value only on success.
        if unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, value.as_mut_ptr()) } == 0 {
            let value = unsafe { value.assume_init() };
            // rlim_t is narrower on some Unix targets.
            #[allow(clippy::unnecessary_cast)]
            return (Some(value.rlim_cur as u64), Some(value.rlim_max as u64));
        }
    }
    (None, None)
}

fn proc_observation() -> (Option<usize>, Option<usize>, Option<usize>) {
    #[cfg(target_os = "linux")]
    {
        let locked = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines().find_map(|line| {
                    line.strip_prefix("VmLck:")
                        .and_then(|v| v.split_whitespace().next())
                        .and_then(|v| v.parse::<usize>().ok())
                        .and_then(|v| v.checked_mul(1024))
                })
            });
        let maps = std::fs::read_to_string("/proc/self/maps")
            .ok()
            .map(|s| s.lines().count());
        let max = std::fs::read_to_string("/proc/sys/vm/max_map_count")
            .ok()
            .and_then(|s| s.trim().parse().ok());
        (locked, maps, max)
    }
    #[cfg(not(target_os = "linux"))]
    {
        (None, None, None)
    }
}

fn lock_budget(soft: Option<u64>) -> usize {
    soft.map(|value| (value / 2).min(MAX_LOCKED as u64) as usize)
        .unwrap_or(64 * 1024)
}

pub fn memory_diagnostics() -> MemoryDiagnostics {
    let (soft, hard) = limits();
    let (locked, maps, max) = proc_observation();
    let b = budget();
    MemoryDiagnostics {
        page_size: page_size(),
        memlock_soft: soft,
        memlock_hard: hard,
        observed_locked_bytes: locked,
        observed_mapping_count: maps,
        max_map_count: max,
        dedicated_region_limit: MAX_REGIONS,
        lock_budget_bytes: MAX_LOCKED,
        effective_lock_budget_bytes: lock_budget(soft),
        dedicated_regions: b.regions,
        reserved_locked_bytes: b.locked,
        fallback_regions: b.fallback,
        release_failures: b.failures,
    }
}

enum Storage {
    Mapping(NonNull<u8>),
    Heap(Box<[u8]>),
}
pub(crate) struct Region {
    storage: Storage,
    size: usize,
    pub status: ProtectionStatus,
}
// Each region exclusively owns its allocation. Shared access exposes immutable bytes only.
unsafe impl Send for Region {}
unsafe impl Sync for Region {}

impl Region {
    pub fn new(capacity: usize) -> io::Result<Self> {
        let page = page_size();
        let size = capacity
            .max(1)
            .checked_add(page - 1)
            .and_then(|v| v.checked_div(page))
            .and_then(|v| v.checked_mul(page))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "sensitive capacity overflow")
            })?;
        if size > isize::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "sensitive capacity exceeds slice limit",
            ));
        }
        // /proc is advisory and never required. Keep substantial mapping headroom.
        // Refresh kernel observations at most once per second instead of per keystroke.
        // A stale sample never overrides syscall results or internal reservations.
        type Observation = (Option<usize>, Option<usize>, Option<usize>);
        static OBSERVATION: Mutex<Option<(std::time::Instant, Observation)>> = Mutex::new(None);
        let (observed, maps, max_maps) = {
            let mut cache = OBSERVATION.lock().unwrap_or_else(|p| p.into_inner());
            if cache
                .as_ref()
                .is_none_or(|(when, _)| when.elapsed() >= std::time::Duration::from_secs(1))
            {
                *cache = Some((std::time::Instant::now(), proc_observation()));
            }
            cache
                .as_ref()
                .map(|(_, values)| *values)
                .unwrap_or_default()
        };
        let allow_mapping = maps
            .zip(max_maps)
            .is_none_or(|(count, max)| count.saturating_add(MAX_REGIONS * 2) < max);
        let reserved = {
            let mut b = budget();
            if allow_mapping && b.regions < MAX_REGIONS {
                b.regions += 1;
                true
            } else {
                false
            }
        };
        if reserved {
            if let Some(ptr) = map(size) {
                let (soft, _) = limits();
                let lock = {
                    let mut b = budget();
                    // Reserve at most half the soft limit; unknown limits use a small cap.
                    let limit = lock_budget(soft);
                    let external = observed.unwrap_or(0).saturating_sub(b.locked);
                    if b.locked
                        .checked_add(size)
                        .is_some_and(|v| v.saturating_add(external) <= limit)
                    {
                        b.locked += size;
                        true
                    } else {
                        false
                    }
                };
                // Region is page aligned, exclusively owned, readable and writable for size bytes.
                let locked = lock && unsafe { memsec::mlock(ptr.as_ptr(), size) };
                if lock && !locked {
                    budget().locked -= size;
                }
                let excluded_from_dumps = exclude_dump(ptr, size);
                return Ok(Self {
                    storage: Storage::Mapping(ptr),
                    size,
                    status: ProtectionStatus {
                        dedicated: true,
                        locked,
                        excluded_from_dumps,
                    },
                });
            }
            budget().regions -= 1;
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(io::Error::other)?;
        bytes.resize(capacity, 0);
        let bytes = bytes.into_boxed_slice();
        budget().fallback += 1;
        Ok(Self {
            storage: Storage::Heap(bytes),
            size: capacity,
            status: ProtectionStatus::default(),
        })
    }
    pub fn bytes(&self) -> &[u8] {
        match &self.storage {
            Storage::Heap(bytes) => bytes,
            // Mapping remains allocated for self's lifetime and size was checked at creation.
            Storage::Mapping(ptr) => unsafe { std::slice::from_raw_parts(ptr.as_ptr(), self.size) },
        }
    }
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        match &mut self.storage {
            Storage::Heap(bytes) => bytes,
            // Exclusive borrow prevents overlapping access to this owned mapping.
            Storage::Mapping(ptr) => unsafe {
                std::slice::from_raw_parts_mut(ptr.as_ptr(), self.size)
            },
        }
    }
}
impl Drop for Region {
    fn drop(&mut self) {
        self.bytes_mut().zeroize();
        match self.storage {
            Storage::Mapping(ptr) => {
                // memsec also wipes; never call it for an unowned/unlocked range.
                let unlocked =
                    !self.status.locked || unsafe { memsec::munlock(ptr.as_ptr(), self.size) };
                let released = unmap(ptr, self.size);
                let mut b = budget();
                if self.status.locked && (unlocked || released) {
                    b.locked -= self.size;
                }
                if released {
                    b.regions -= 1;
                } else {
                    b.failures += 1;
                }
            }
            Storage::Heap(_) => budget().fallback -= 1,
        }
    }
}

fn map(size: usize) -> Option<NonNull<u8>> {
    #[cfg(unix)]
    {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            None
        } else {
            NonNull::new(ptr.cast())
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Memory::*;
        NonNull::new(
            unsafe {
                VirtualAlloc(
                    std::ptr::null(),
                    size,
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_READWRITE,
                )
            }
            .cast(),
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = size;
        None
    }
}
fn unmap(ptr: NonNull<u8>, size: usize) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::munmap(ptr.as_ptr().cast(), size) == 0 }
    }
    #[cfg(windows)]
    {
        let _ = size;
        unsafe {
            windows_sys::Win32::System::Memory::VirtualFree(
                ptr.as_ptr().cast(),
                0,
                windows_sys::Win32::System::Memory::MEM_RELEASE,
            ) != 0
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (ptr, size);
        false
    }
}
fn exclude_dump(ptr: NonNull<u8>, size: usize) -> bool {
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::madvise(ptr.as_ptr().cast(), size, libc::MADV_DONTDUMP) == 0 }
    }
    #[cfg(any(target_os = "freebsd", target_os = "dragonfly"))]
    {
        unsafe { libc::madvise(ptr.as_ptr().cast(), size, libc::MADV_NOCORE) == 0 }
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "dragonfly")))]
    {
        let _ = (ptr, size);
        false
    }
}
