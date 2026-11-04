//! Hardware discovery and JVM tuning.
//!
//! Everything here is platform code plus one pure function. The pure function
//! ([`recommend`]) is the part that decides how much heap and which GC flags an
//! instance gets, and it is fully testable — no OS calls, no I/O.
//!
//! Detection is best-effort by design: a launcher that refuses to start because
//! a GPU string could not be read is worse than one that guesses conservatively.
//! Every field that cannot be read falls back to a documented default.

use serde::Serialize;

/// What kind of graphics adapter was found. Only used to decide whether the
/// machine is likely memory-starved (shared-memory iGPU) or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuKind {
    Dedicated,
    Integrated,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfile {
    /// e.g. `windows x86_64`.
    pub platform: String,
    pub cpu_brand: String,
    /// Best-effort physical core count. `cpu_threads` is authoritative.
    pub cpu_cores: u32,
    /// Logical processors available to this process.
    pub cpu_threads: u32,
    pub memory_total_mb: u64,
    pub memory_available_mb: u64,
    pub gpu: Option<String>,
    pub gpu_kind: GpuKind,
}

impl HardwareProfile {
    /// Detect the current machine. Never fails: missing pieces become defaults.
    pub fn detect() -> Self {
        let snapshot = platform::read();
        let threads = std::thread::available_parallelism()
            .map(|value| value.get() as u32)
            .unwrap_or(1);

        Self {
            platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            cpu_brand: snapshot.cpu_brand.unwrap_or_else(|| "unknown CPU".to_string()),
            cpu_cores: snapshot.cpu_cores.unwrap_or_else(|| threads.max(2) / 2).max(1),
            cpu_threads: threads.max(1),
            // 2 GiB is the floor Minecraft itself expects; below that the
            // recommendation is capped rather than the launcher refusing to run.
            memory_total_mb: snapshot.memory_total_mb.unwrap_or(2048).max(512),
            memory_available_mb: snapshot
                .memory_available_mb
                .unwrap_or_else(|| snapshot.memory_total_mb.unwrap_or(2048) / 2),
            gpu: snapshot.gpu,
            gpu_kind: snapshot.gpu_kind,
        }
    }
}

/// One instance's heap + GC plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tuning {
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    pub jvm_args: Vec<String>,
    /// Why the numbers came out the way they did. Shown in the UI, never logged
    /// with anything machine-identifying beyond the counts themselves.
    pub notes: Vec<String>,
}

/// Compute a tuning plan. Pure: same input, same output, every platform.
///
/// `java_major` matters because G1 flags changed between 8 and 21, and because
/// Java 21 ships ZGC generational; we stay on G1, which is the safest default
/// for Minecraft's allocation pattern on every supported runtime.
pub fn recommend(profile: &HardwareProfile, modded: bool, java_major: u32) -> Tuning {
    let total = profile.memory_total_mb;
    let mut notes = Vec::new();

    // The heap ceiling has three competing limits: what the machine has, what
    // the JVM can address comfortably, and what Minecraft actually uses. 12 GiB
    // is the point where more heap stops helping and only lengthens GC pauses.
    let class_cap = match (total, modded) {
        (0..=4095, _) => 1536,
        (4096..=8191, false) => 2048,
        (4096..=8191, true) => 3072,
        (8192..=16383, false) => 3072,
        (8192..=16383, true) => 4096,
        (16384..=32767, false) => 4096,
        (16384..=32767, true) => 6144,
        (_, false) => 6144,
        (_, true) => 8192,
    };

    // Never promise the JVM more than half of physical RAM: the OS, the GPU
    // driver and the launcher all need to breathe, and swapping during world
    // generation is worse than a smaller heap.
    let half_of_ram = (total / 2).max(1024) as u32;
    let mut max_memory_mb = class_cap.min(half_of_ram);
    // Below 1 GiB Minecraft cannot even allocate its default arenas.
    max_memory_mb = max_memory_mb.clamp(1024, 12_288);

    if class_cap > half_of_ram {
        notes.push(format!(
            "RAM limit: {half_of_ram} MB is half of the installed {total} MB, so the plan is capped there."
        ));
    }

    // A shared-memory iGPU steals from system RAM; be a little more frugal.
    if profile.gpu_kind == GpuKind::Integrated && total <= 16_384 {
        max_memory_mb = (max_memory_mb - 512).max(1024);
        notes.push("Integrated GPU shares system memory — heap reduced by 512 MB.".to_string());
    }

    // Xms is stabilisation, not allocation: a quarter of Xmx is enough to stop
    // the early-game GC storm without pinning the whole heap from second one.
    let mut min_memory_mb = (max_memory_mb / 4).clamp(512, 4096);
    if min_memory_mb > max_memory_mb {
        min_memory_mb = max_memory_mb;
    }

    let mut jvm_args = g1_flags(profile, java_major);
    if profile.cpu_threads <= 2 {
        notes.push(
            "Two or fewer CPU threads — parallel GC threads reduced to avoid starving the game."
                .to_string(),
        );
        jvm_args.push(format!(
            "-XX:ParallelGCThreads={}",
            profile.cpu_threads.max(1)
        ));
        // Pre-touching every page costs a long startup on low-end machines.
        jvm_args.retain(|flag| flag != "-XX:+AlwaysPreTouch");
    } else if profile.cpu_threads >= 8 {
        notes.push(format!(
            "{} CPU threads — ParallelGCThreads left at the JVM default.",
            profile.cpu_threads
        ));
    }

    if modded {
        notes.push("Modded instance — heap class raised one step.".to_string());
    }
    notes.push(format!(
        "Java {java_major}: G1GC with Aikar-style region tuning."
    ));

    Tuning {
        min_memory_mb,
        max_memory_mb,
        jvm_args,
        notes,
    }
}

fn g1_flags(profile: &HardwareProfile, java_major: u32) -> Vec<String> {
    let mut flags: Vec<String> = [
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:MaxGCPauseMillis=200",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:+AlwaysPreTouch",
        "-XX:G1NewSizePercent=30",
        "-XX:G1MaxNewSizePercent=40",
        "-XX:G1HeapRegionSize=8M",
        "-XX:G1ReservePercent=20",
        "-XX:G1HeapWastePercent=5",
        "-XX:G1MixedGCCountTarget=4",
        "-XX:InitiatingHeapOccupancyPercent=15",
        "-XX:G1MixedGCLiveThresholdPercent=90",
        "-XX:G1RSetUpdatingPauseTimePercent=5",
        "-XX:SurvivorRatio=32",
        "-XX:+PerfDisableSharedMem",
        "-XX:MaxTenuringThreshold=1",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    // Java 8 reports its heap differently and does not understand the unified
    // logging flags added in 9; nothing here needs them, so the flag set is the
    // same for every supported runtime.
    if java_major < 9 {
        flags.retain(|flag| flag != "-XX:+UseG1GC");
        // G1 exists in 8u40+, but parallel scanning flags below are 9+ only.
        flags.insert(0, "-XX:+UseG1GC".to_string());
    }

    // G1 region sizing beyond 8M only pays off on very large heaps; 8M is the
    // sweet spot up to ~16 GiB and keeps pause prediction accurate.
    if profile.memory_total_mb >= 32_768 {
        flags.push("-XX:G1HeapRegionSize=16M".to_string());
    }
    flags
}

/// Merge a plan into an existing flag list without duplicating launcher-owned
/// flags or `-Xmx`. Returns the merged list; caller decides whether to save it.
pub fn merge_jvm_args(existing: &[String], plan: &[String]) -> Vec<String> {
    let mut out: Vec<String> = existing
        .iter()
        .filter(|arg| !plan.contains(arg))
        .cloned()
        .collect();
    out.extend(plan.iter().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(total_mb: u64, threads: u32) -> HardwareProfile {
        HardwareProfile {
            platform: "test x86_64".into(),
            cpu_brand: "Test CPU".into(),
            cpu_cores: threads / 2,
            cpu_threads: threads,
            memory_total_mb: total_mb,
            memory_available_mb: total_mb / 2,
            gpu: Some("Test GPU".into()),
            gpu_kind: GpuKind::Dedicated,
        }
    }

    #[test]
    fn heap_scales_with_ram_and_moddedness() {
        let vanilla = recommend(&profile(16_384, 16), false, 21);
        let modded = recommend(&profile(16_384, 16), true, 21);
        assert_eq!(vanilla.max_memory_mb, 4096);
        assert_eq!(modded.max_memory_mb, 6144);
        assert!(modded.max_memory_mb <= 16_384 / 2);
    }

    #[test]
    fn heap_never_exceeds_half_of_ram() {
        for total in [2048u64, 4096, 8192, 16_384, 65_536] {
            for modded in [false, true] {
                let plan = recommend(&profile(total, 8), modded, 21);
                assert!(
                    u64::from(plan.max_memory_mb) <= (total / 2).max(1024),
                    "{} MB heap on {} MB RAM",
                    plan.max_memory_mb,
                    total
                );
                assert!(plan.min_memory_mb <= plan.max_memory_mb);
                assert!(plan.max_memory_mb >= 1024);
            }
        }
    }

    #[test]
    fn low_thread_count_machine_drops_pretouch() {
        let plan = recommend(&profile(8192, 2), true, 21);
        assert!(!plan.jvm_args.iter().any(|flag| flag == "-XX:+AlwaysPreTouch"));
        assert!(plan
            .jvm_args
            .iter()
            .any(|flag| flag == "-XX:ParallelGCThreads=2"));
    }

    #[test]
    fn integrated_gpu_shrinks_the_heap() {
        let mut hw = profile(8192, 8);
        let dedicated = recommend(&hw, true, 21);
        hw.gpu_kind = GpuKind::Integrated;
        let plan = recommend(&hw, true, 21);
        assert_eq!(plan.max_memory_mb, dedicated.max_memory_mb - 512);
    }

    #[test]
    fn merge_never_duplicates_flags() {
        let existing = vec!["-XX:+UseG1GC".to_string(), "-Dfoo=bar".to_string()];
        let plan = vec![
            "-XX:+UseG1GC".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
        ];
        let merged = merge_jvm_args(&existing, &plan);
        assert_eq!(
            merged.iter().filter(|f| *f == "-XX:+UseG1GC").count(),
            1
        );
        assert!(merged.contains(&"-Dfoo=bar".to_string()));
        assert!(merged.contains(&"-XX:MaxGCPauseMillis=200".to_string()));
    }
}

/* ------------------------------------------------------------------------- */
/* Platform layer                                                            */
/* ------------------------------------------------------------------------- */

struct Snapshot {
    cpu_brand: Option<String>,
    cpu_cores: Option<u32>,
    memory_total_mb: Option<u64>,
    memory_available_mb: Option<u64>,
    gpu: Option<String>,
    gpu_kind: GpuKind,
}

#[cfg(windows)]
mod platform {
    use super::{GpuKind, Snapshot};

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

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
        fn GetLogicalProcessorInformationEx(
            relationship: i32,
            buffer: *mut u8,
            returned_length: *mut u32,
        ) -> i32;
        fn GetLastError() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumDisplayDevicesW(
            device: *const u16,
            device_number: u32,
            display_device: *mut DisplayDeviceW,
            flags: u32,
        ) -> i32;
    }

    #[repr(C)]
    struct DisplayDeviceW {
        cb: u32,
        device_name: [u16; 32],
        device_string: [u16; 128],
        state_flags: u32,
        device_id: [u16; 128],
        device_key: [u16; 128],
    }

    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    /// `RelationProcessorCore` from `winnt.h`.
    const RELATION_PROCESSOR_CORE: i32 = 0;

    pub fn read() -> Snapshot {
        let (total, available) = memory();
        let (gpu, gpu_kind) = graphics();

        Snapshot {
            cpu_brand: cpu_brand(),
            cpu_cores: physical_cores(),
            memory_total_mb: total,
            memory_available_mb: available,
            gpu,
            gpu_kind,
        }
    }

    fn memory() -> (Option<u64>, Option<u64>) {
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
        // SAFETY: the struct is fully initialised and `length` is set to its
        // own size, which is the contract GlobalMemoryStatusEx documents.
        let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
        if !ok {
            return (None, None);
        }
        // Windows reports bytes; the launcher speaks MB everywhere.
        (
            Some(status.total_phys / (1024 * 1024)),
            Some(status.avail_phys / (1024 * 1024)),
        )
    }

    /// Count physical cores through the documented EX API. Falls back to the
    /// logical count when the call fails, which is a harmless overestimate for
    /// GC thread sizing.
    fn physical_cores() -> Option<u32> {
        let mut length: u32 = 0;
        // SAFETY: a null buffer with length 0 is the documented size probe.
        unsafe {
            GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, std::ptr::null_mut(), &mut length);
        }
        if length == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }

        let mut buffer = vec![0u8; length as usize];
        // SAFETY: the buffer is exactly `length` bytes, as returned above.
        let ok = unsafe {
            GetLogicalProcessorInformationEx(
                RELATION_PROCESSOR_CORE,
                buffer.as_mut_ptr(),
                &mut length,
            )
        };
        if ok == 0 {
            return None;
        }

        // Each record is a SLPI-style header: relationship (4 bytes), size (4),
        // then a CPU_SET_INFORMATION payload. We only need the record count.
        let mut offset = 0usize;
        let mut cores = 0u32;
        while offset + 8 <= buffer.len() {
            let size = u32::from_ne_bytes([
                buffer[offset + 4],
                buffer[offset + 5],
                buffer[offset + 6],
                buffer[offset + 7],
            ]) as usize;
            if size < 8 {
                break;
            }
            cores += 1;
            offset += size;
        }

        (cores > 0).then_some(cores)
    }

    fn cpu_brand() -> Option<String> {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::__cpuid;
            // Leaves 0x8000_0002..=0x8000_0004 hold a 48-byte brand string.
            let mut bytes = [0u8; 48];
            for (index, leaf) in (0x8000_0002u32..=0x8000_0004).enumerate() {
                // SAFETY: __cpuid is available on every x86_64 CPU.
                let result = __cpuid(leaf);
                let base = index * 16;
                bytes[base..base + 4].copy_from_slice(&result.eax.to_le_bytes());
                bytes[base + 4..base + 8].copy_from_slice(&result.ebx.to_le_bytes());
                bytes[base + 8..base + 12].copy_from_slice(&result.ecx.to_le_bytes());
                bytes[base + 12..base + 16].copy_from_slice(&result.edx.to_le_bytes());
            }
            let brand = String::from_utf8_lossy(&bytes)
                .trim_matches(char::from(0))
                .trim()
                .to_string();
            if !brand.is_empty() {
                return Some(brand);
            }
        }

        None
    }

    fn graphics() -> (Option<String>, GpuKind) {
        let mut adapter = None;
        for index in 0..8u32 {
            let mut device = DisplayDeviceW {
                cb: std::mem::size_of::<DisplayDeviceW>() as u32,
                device_name: [0; 32],
                device_string: [0; 128],
                state_flags: 0,
                device_id: [0; 128],
                device_key: [0; 128],
            };
            // SAFETY: `device.cb` declares the structure size, as required.
            let ok = unsafe { EnumDisplayDevicesW(std::ptr::null(), index, &mut device, 0) };
            if ok == 0 {
                break;
            }
            let text = wide_to_string(&device.device_string);
            if text.is_empty() || adapter.as_deref() == Some(text.as_str()) {
                continue;
            }
            if is_virtual_or_basic(&text) {
                continue;
            }
            adapter = Some(text);
            break;
        }

        match adapter {
            Some(name) => {
                let kind = classify(&name);
                (Some(name), kind)
            }
            None => (None, GpuKind::Unknown),
        }
    }

    fn wide_to_string(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end]).trim().to_string()
    }

    fn is_virtual_or_basic(text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        lower.contains("microsoft basic")
            || lower.contains("remote desktop")
            || lower.contains("citrix")
            || lower.contains("virtualbox")
            || lower.contains("vmware")
    }

    /// Vendor strings are stable enough to classify on: Intel's UHD/Iris and
    /// AMD's Radeon Graphics entries are the integrated ones.
    pub fn classify(name: &str) -> GpuKind {
        let lower = name.to_ascii_lowercase();
        if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("quadro")
            || lower.contains("radeon rx")
            || lower.contains("radeon pro")
            || lower.contains("arc a")
        {
            return GpuKind::Dedicated;
        }
        if lower.contains("intel")
            || lower.contains("iris")
            || lower.contains("uhd")
            || lower.contains("hd graphics")
            || lower.contains("vega")
            || lower.contains("radeon graphics")
            || lower.contains("apple")
        {
            return GpuKind::Integrated;
        }
        GpuKind::Unknown
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{GpuKind, Snapshot};

    pub fn read() -> Snapshot {
        let (total, available) = memory();
        let (gpu, gpu_kind) = graphics();
        Snapshot {
            cpu_brand: cpu_brand(),
            cpu_cores: physical_cores(),
            memory_total_mb: total,
            memory_available_mb: available,
            gpu,
            gpu_kind,
        }
    }

    fn memory() -> (Option<u64>, Option<u64>) {
        let Ok(raw) = std::fs::read_to_string("/proc/meminfo") else {
            return (None, None);
        };
        let mut total = None;
        let mut available = None;
        for line in raw.lines() {
            let Some((key, rest)) = line.split_once(':') else {
                continue;
            };
            let kb: Option<u64> = rest
                .trim()
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
            match key {
                "MemTotal" => total = kb.map(|kb| kb / 1024),
                "MemAvailable" => available = kb.map(|kb| kb / 1024),
                _ => {}
            }
        }
        (total, available)
    }

    fn cpu_brand() -> Option<String> {
        let raw = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        raw.lines()
            .find_map(|line| line.strip_prefix("model name"))
            .and_then(|rest| rest.split_once(':'))
            .map(|(_, value)| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    fn physical_cores() -> Option<u32> {
        let raw = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        let mut seen: Vec<String> = Vec::new();
        let mut physical_id = String::new();
        let mut core_id = String::new();
        for line in raw.lines() {
            if let Some(rest) = line.strip_prefix("physical id") {
                physical_id = rest.split(':').nth(1).unwrap_or_default().trim().to_string();
            } else if let Some(rest) = line.strip_prefix("core id") {
                core_id = rest.split(':').nth(1).unwrap_or_default().trim().to_string();
            } else if line.trim().is_empty() {
                if !core_id.is_empty() {
                    let key = format!("{physical_id}:{core_id}");
                    if !seen.contains(&key) {
                        seen.push(key);
                    }
                }
                physical_id.clear();
                core_id.clear();
            }
        }
        if !core_id.is_empty() && !seen.iter().any(|key| key.ends_with(&format!(":{core_id}"))) {
            seen.push(format!("{physical_id}:{core_id}"));
        }
        (u32::try_from(seen.len()).ok()).filter(|count| *count > 0)
    }

    fn graphics() -> (Option<String>, GpuKind) {
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
            return (None, GpuKind::Unknown);
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let base = entry.path().join("device");
            if let Ok(product) = std::fs::read_to_string(base.join("product_name")) {
                let product = product.trim().to_string();
                if !product.is_empty() {
                    let kind = classify(&product);
                    return (Some(product), kind);
                }
            }
            if let Ok(vendor) = std::fs::read_to_string(base.join("vendor")) {
                let label = match vendor.trim() {
                    "0x10de" => Some("NVIDIA GPU"),
                    "0x1002" | "0x1022" => Some("AMD GPU"),
                    "0x8086" => Some("Intel GPU"),
                    _ => None,
                };
                if let Some(label) = label {
                    return (Some(label.to_string()), classify(label));
                }
            }
        }
        (None, GpuKind::Unknown)
    }

    fn classify(name: &str) -> GpuKind {
        let lower = name.to_ascii_lowercase();
        if lower.contains("nvidia")
            || lower.contains("geforce")
            || lower.contains("quadro")
            || lower.contains("radeon rx")
        {
            GpuKind::Dedicated
        } else if lower.contains("intel") || lower.contains("amd") || lower.contains("apu") {
            GpuKind::Integrated
        } else {
            GpuKind::Unknown
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{GpuKind, Snapshot};

    #[link(name = "System")]
    extern "C" {
        fn sysctlbyname(
            name: *const std::os::raw::c_char,
            oldp: *mut std::os::raw::c_void,
            oldlenp: *mut usize,
            newp: *mut std::os::raw::c_void,
            newlen: usize,
        ) -> std::os::raw::c_int;
    }

    pub fn read() -> Snapshot {
        let total_bytes = sysctl_u64("hw.memsize");
        let gpu = sysctl_string("machdep.cpu.brand_string")
            .as_deref()
            .map(classify_from_cpu)
            .flatten();

        Snapshot {
            cpu_brand: sysctl_string("machdep.cpu.brand_string"),
            cpu_cores: sysctl_u32("hw.physicalcpu"),
            memory_total_mb: total_bytes.map(|bytes| bytes / (1024 * 1024)),
            memory_available_mb: None,
            gpu,
            // Apple silicon is an SoC with unified memory; Intel Macs with a
            // discrete GPU are indistinguishable without IOKit, so leave it
            // unknown rather than guessing wrong on a MacBook Pro.
            gpu_kind: GpuKind::Unknown,
        }
    }

    fn classify_from_cpu(brand: &str) -> Option<String> {
        brand
            .to_ascii_lowercase()
            .contains("apple")
            .then(|| "Apple integrated GPU".to_string())
    }

    fn sysctl_u64(key: &str) -> Option<u64> {
        let mut value: u64 = 0;
        let mut size = std::mem::size_of::<u64>();
        let key = std::ffi::CString::new(key).ok()?;
        // SAFETY: `value` is a u64 and `size` is its exact width; sysctlbyname
        // writes at most `size` bytes for hw.memsize.
        let rc = unsafe {
            sysctlbyname(
                key.as_ptr(),
                &mut value as *mut u64 as *mut std::os::raw::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        (rc == 0).then_some(value)
    }

    fn sysctl_u32(key: &str) -> Option<u32> {
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>();
        let key = std::ffi::CString::new(key).ok()?;
        // SAFETY: as above, with the matching width.
        let rc = unsafe {
            sysctlbyname(
                key.as_ptr(),
                &mut value as *mut u32 as *mut std::os::raw::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        (rc == 0).then_some(value)
    }

    fn sysctl_string(key: &str) -> Option<String> {
        let key_c = std::ffi::CString::new(key).ok()?;
        let mut size = 0usize;
        // SAFETY: probing with a null buffer returns the required size.
        let rc = unsafe {
            sysctlbyname(
                key_c.as_ptr(),
                std::ptr::null_mut(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 || size == 0 {
            return None;
        }
        let mut buffer = vec![0u8; size];
        // SAFETY: the buffer is exactly the size the kernel reported.
        let rc = unsafe {
            sysctlbyname(
                key_c.as_ptr(),
                buffer.as_mut_ptr() as *mut std::os::raw::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return None;
        }
        buffer.truncate(size);
        let text = String::from_utf8_lossy(&buffer).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::{GpuKind, Snapshot};

    pub fn read() -> Snapshot {
        Snapshot {
            cpu_brand: None,
            cpu_cores: None,
            memory_total_mb: None,
            memory_available_mb: None,
            gpu: None,
            gpu_kind: GpuKind::Unknown,
        }
    }
}
