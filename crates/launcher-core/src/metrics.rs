//! Live counters for the overlay: CPU load, memory pressure, process RSS.
//!
//! Static discovery lives in [`crate::hardware`] and runs once per machine;
//! this module runs once per second while the overlay is open, so it stays
//! small and allocation-free where it can.
//!
//! The one piece worth reading twice is [`cpu_percent`]: CPU load is a delta
//! over wall time, not a snapshot. The first sample therefore has no answer and
//! returns `None` rather than a fabricated 0%. Every platform reports counters
//! as monotonic tick totals, which is what makes the subtraction safe.
//!
//! macOS returns counters only for memory: reading CPU ticks there needs Mach
//! host statistics, which is a different binding table from the `sysctlbyname`
//! calls [`crate::hardware`] already has, and a wrong CPU number in an overlay
//! is worse than an empty row.

use std::sync::Mutex;

use serde::Serialize;

/// One reading of the platform's CPU tick counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CpuTimes {
    /// Ticks the CPU spent idle.
    pub idle: u64,
    /// All ticks, idle included.
    pub total: u64,
}

/// Busy share between two readings, in percent.
///
/// Returns `None` when nothing elapsed (`total` did not move), which is the
/// honest answer for a counter that has not been ticked yet.
pub fn cpu_percent(previous: CpuTimes, current: CpuTimes) -> Option<f32> {
    let total = current.total.checked_sub(previous.total)?;
    let idle = current.idle.saturating_sub(previous.idle);
    if total == 0 || idle > total {
        return None;
    }
    let busy = total - idle;
    Some(((busy as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32)
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveMetrics {
    /// `None` on the first sample and wherever the platform cannot report it.
    pub cpu_percent: Option<f32>,
    pub memory_total_mb: Option<u64>,
    pub memory_used_mb: Option<u64>,
    /// Resident set size of the game process, when a pid was supplied.
    pub process_rss_mb: Option<u64>,
    pub process_id: Option<u32>,
}

/// Keeps the previous tick reading so consecutive samples produce a percentage.
#[derive(Default)]
pub struct Sampler {
    previous: Mutex<Option<CpuTimes>>,
}

impl Sampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take one reading. `pid` is the game process, when one is running.
    pub fn sample(&self, pid: Option<u32>) -> LiveMetrics {
        let cpu_percent = match platform::cpu_times() {
            Some(current) => {
                let previous = self
                    .previous
                    .lock()
                    .ok()
                    .and_then(|mut guard| guard.replace(current));
                previous.and_then(|before| cpu_percent(before, current))
            }
            None => None,
        };

        let (total, available) = platform::memory_mb();
        LiveMetrics {
            cpu_percent,
            memory_total_mb: total,
            memory_used_mb: match (total, available) {
                (Some(total), Some(available)) => Some(total.saturating_sub(available)),
                _ => None,
            },
            process_rss_mb: pid.and_then(platform::process_rss_mb),
            process_id: pid,
        }
    }
}

/* ------------------------------------------------------------------------- */
/* Platform layer                                                            */
/* ------------------------------------------------------------------------- */

#[cfg(windows)]
mod platform {
    use super::CpuTimes;

    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn ticks(&self) -> u64 {
            ((self.high as u64) << 32) | self.low as u64
        }
    }

    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn CloseHandle(handle: isize) -> i32;
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    /// `PROCESS_QUERY_LIMITED_INFORMATION` — enough for RSS, no more.
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    pub fn cpu_times() -> Option<CpuTimes> {
        let mut idle = FileTime { low: 0, high: 0 };
        let mut kernel = FileTime { low: 0, high: 0 };
        let mut user = FileTime { low: 0, high: 0 };
        // SAFETY: three distinct, fully initialised `FILETIME` buffers; the API
        // writes into all of them and returns zero on failure.
        let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0;
        if !ok {
            return None;
        }
        // Kernel time already includes idle time, so the sum is the full budget.
        Some(CpuTimes {
            idle: idle.ticks(),
            total: kernel.ticks().saturating_add(user.ticks()),
        })
    }

    pub fn memory_mb() -> (Option<u64>, Option<u64>) {
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        // SAFETY: `length` is the struct's own size, as documented.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
        if !ok {
            return (None, None);
        }
        (
            Some(status.total_phys / (1024 * 1024)),
            Some(status.avail_phys / (1024 * 1024)),
        )
    }

    pub fn process_rss_mb(pid: u32) -> Option<u64> {
        // SAFETY: the access mask is the read-only one; the handle is closed on
        // every path below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle == 0 {
            return None;
        }
        let mut counters = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: `counters.cb` carries the struct size the API expects.
        let ok = unsafe { GetProcessMemoryInfo(handle, &mut counters, counters.cb) } != 0;
        // SAFETY: `handle` came from `OpenProcess` and is owned here.
        unsafe {
            CloseHandle(handle);
        }
        if !ok {
            return None;
        }
        Some(counters.working_set_size as u64 / (1024 * 1024))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::CpuTimes;

    pub fn cpu_times() -> Option<CpuTimes> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let line = stat.lines().next()?;
        let mut fields = line.split_whitespace();
        if fields.next() != Some("cpu") {
            return None;
        }
        // user nice system idle iowait irq softirq steal — guest time is already
        // counted inside user, so it is excluded from the total.
        let values: Vec<u64> = fields.filter_map(|value| value.parse().ok()).collect();
        if values.len() < 5 {
            return None;
        }
        let idle = values[3].saturating_add(values[4]);
        let total = values.iter().take(8).sum();
        Some(CpuTimes { idle, total })
    }

    pub fn memory_mb() -> (Option<u64>, Option<u64>) {
        let Ok(info) = std::fs::read_to_string("/proc/meminfo") else {
            return (None, None);
        };
        let mut total = None;
        let mut available = None;
        for line in info.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                total = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|kb| kb / 1024);
            } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                available = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .map(|kb| kb / 1024);
            }
        }
        (total, available)
    }

    pub fn process_rss_mb(pid: u32) -> Option<u64> {
        let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
        // "size resident shared ..." in pages. 4 KiB is the page size on every
        // architecture Minecraft runs on (x86_64 and aarch64).
        let resident: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        Some(resident * 4 / 1024)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::CpuTimes;

    pub fn cpu_times() -> Option<CpuTimes> {
        None
    }

    pub fn memory_mb() -> (Option<u64>, Option<u64>) {
        // `hw.memsize` is the total; the used figure needs `vm_statistics64`,
        // which this binary does not bind.
        #[link(name = "c")]
        extern "C" {
            fn sysctlbyname(
                name: *const u8,
                oldp: *mut u8,
                oldlenp: *mut usize,
                newp: *mut u8,
                newlen: usize,
            ) -> i32;
        }
        let mut total: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        let name = b"hw.memsize\0";
        // SAFETY: `total` and `size` describe a valid u64 out-parameter.
        let ok = unsafe {
            sysctlbyname(
                name.as_ptr(),
                &mut total as *mut u64 as *mut u8,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ok != 0 || total == 0 {
            return (None, None);
        }
        (Some(total / (1024 * 1024)), None)
    }

    pub fn process_rss_mb(_pid: u32) -> Option<u64> {
        None
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::CpuTimes;

    pub fn cpu_times() -> Option<CpuTimes> {
        None
    }

    pub fn memory_mb() -> (Option<u64>, Option<u64>) {
        (None, None)
    }

    pub fn process_rss_mb(_pid: u32) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_counter_that_did_not_move_has_no_answer() {
        let same = CpuTimes {
            idle: 100,
            total: 1000,
        };
        assert_eq!(cpu_percent(same, same), None);
    }

    #[test]
    fn idle_share_is_reported_as_busy_share() {
        // 500 of 1000 ticks idle: the machine was half busy.
        let before = CpuTimes { idle: 0, total: 0 };
        let after = CpuTimes {
            idle: 500,
            total: 1000,
        };
        assert_eq!(cpu_percent(before, after), Some(50.0));
    }

    #[test]
    fn a_fully_idle_and_a_fully_busy_machine_sit_at_the_edges() {
        let zero = CpuTimes { idle: 0, total: 0 };
        assert_eq!(
            cpu_percent(
                zero,
                CpuTimes {
                    idle: 1000,
                    total: 1000
                }
            ),
            Some(0.0)
        );
        assert_eq!(
            cpu_percent(
                zero,
                CpuTimes {
                    idle: 0,
                    total: 1000
                }
            ),
            Some(100.0)
        );
    }

    #[test]
    fn a_backwards_counter_is_refused_rather_than_negative() {
        let before = CpuTimes {
            idle: 10,
            total: 100,
        };
        let after = CpuTimes { idle: 5, total: 50 };
        assert_eq!(cpu_percent(before, after), None);
    }

    #[test]
    fn idle_above_total_is_not_reported_as_negative_load() {
        let before = CpuTimes { idle: 0, total: 0 };
        let after = CpuTimes {
            idle: 900,
            total: 800,
        };
        assert_eq!(cpu_percent(before, after), None);
    }

    #[test]
    fn the_first_sample_seeds_the_counter_and_a_later_one_answers() {
        let sampler = Sampler::new();
        let first = sampler.sample(None);
        // The first reading only seeds the counter; a fabricated 0% would be a
        // lie the UI would draw as a flat line.
        assert!(
            first.cpu_percent.is_none(),
            "the first sample must not invent a percentage"
        );

        if !cfg!(any(windows, target_os = "linux")) {
            // Platforms without CPU counters keep answering `None`, which is
            // what the overlay draws as an em dash.
            assert!(sampler.sample(None).cpu_percent.is_none());
            return;
        }

        // Windows reports CPU counters at the scheduler tick (~15 ms), so two
        // immediate reads can return the same tick and honestly answer `None`.
        // Waiting for the counter to move tests the delta without racing it.
        let mut value = None;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            if let Some(percent) = sampler.sample(None).cpu_percent {
                value = Some(percent);
                break;
            }
        }

        let value = value.expect("a moving counter must produce a percentage");
        assert!((0.0..=100.0).contains(&value));
    }
}
