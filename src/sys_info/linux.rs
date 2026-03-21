use std::fs;
use std::process::Command;

/// Reads the motherboard serial directly from the DMI sysfs virtual file
pub fn get_motherboard_serial() -> Option<String> {
    fs::read_to_string("/sys/class/dmi/id/board_serial")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "Default string")
}

/// Executes `lsblk` to grab hardcoded factory serials of all connected block devices
pub fn get_drive_serials() -> Vec<String> {
    let mut serials = Vec::new();
    
    // Run: lsblk -J -d -o MODEL,SERIAL
    if let Ok(output) = Command::new("lsblk")
        .args(["-J", "-d", "-o", "MODEL,SERIAL"])
        .output() 
    {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            if let Some(blockdevices) = json.get("blockdevices").and_then(|b| b.as_array()) {
                for dev in blockdevices {
                    let serial = dev.get("serial").and_then(|s| s.as_str()).unwrap_or("").trim();
                    let model = dev.get("model").and_then(|m| m.as_str()).unwrap_or("Unknown Drive").trim();
                    if !serial.is_empty() {
                        serials.push(format!("{} (S/N: {})", model, serial));
                    }
                }
            }
        }
    }
    
    serials
}

/// Scans the sysfs network class for permanent MAC addresses, strictly filtering for physical hardware
pub fn get_mac_addresses() -> Vec<String> {
    let mut macs = Vec::new();
    
    if let Ok(entries) = fs::read_dir("/sys/class/net/") {
        for entry in entries.flatten() {
            let path = entry.path();
            
            // THE FIX: Check if this network interface is backed by physical hardware.
            // Physical devices have a "device" symlink pointing to the PCI/USB bus.
            // Virtual devices (Docker, VMs, loopback) do not.
            let device_path = path.join("device");
            if !device_path.exists() {
                continue; // Skip this iteration, it's just software!
            }
            
            let mac_path = path.join("address");
            let iface_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            
            if let Ok(mac) = fs::read_to_string(mac_path) {
                let trimmed_mac = mac.trim().to_string();
                if !trimmed_mac.is_empty() && trimmed_mac != "00:00:00:00:00:00" {
                    macs.push(format!("{} [{}]", iface_name, trimmed_mac.to_uppercase()));
                }
            }
        }
    }
    
    macs
}

/// Executes `dmidecode` to extract the factory serial number of each physical RAM stick.
/// Requires root privileges to execute successfully.
pub fn get_ram_serials() -> Vec<String> {
    let mut serials = Vec::new();
    
    let mut current_manufacturer = String::from("Unknown");
    let mut current_locator = String::from("Unknown");
    let mut current_part = String::new();
    
    // Run: dmidecode -t memory
    if let Ok(output) = Command::new("dmidecode").args(["-t", "memory"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Locator:") {
                current_locator = trimmed.replace("Locator:", "").trim().to_string();
            } else if trimmed.starts_with("Manufacturer:") {
                current_manufacturer = trimmed.replace("Manufacturer:", "").trim().to_string();
            } else if trimmed.starts_with("Part Number:") {
                current_part = trimmed.replace("Part Number:", "").trim().to_string();
            } else if trimmed.starts_with("Serial Number:") {
                let serial = trimmed.replace("Serial Number:", "").trim().to_string();
                
                // Filter out empty slots or generic manufacturer filler data
                if !serial.is_empty() && serial != "Not Specified" && serial != "Unknown" {
                    serials.push(format!("{} {} {} (S/N: {})", current_manufacturer, current_part, current_locator, serial));
                }
                
                // Reset for next DIMM block
                current_manufacturer = String::from("Unknown");
                current_locator = String::from("Unknown");
                current_part = String::new();
            }
        }
    }
    
    serials
}

/// Collects GPU identifiers using multiple strategies, vendor-agnostic.
///
/// Strategy (in order of attempt):
/// 1. **sysfs DRM** — reads `/sys/class/drm/card*/device/{vendor,device,subsystem_vendor,subsystem_device}`
///    Works for ALL GPU vendors (NVIDIA, AMD, Intel) without any external tooling.
/// 2. **nvidia-smi** — if the NVIDIA driver + tool is installed, grab the true per-chip UUID.
/// 3. **lspci fallback** — last resort, grabs PCI slot IDs for VGA/3D controllers.
pub fn get_gpu_uuids() -> Vec<String> {
    let mut ids = Vec::new();

    // ── Strategy 1: sysfs DRM subsystem (works for any vendor) ──
    if let Ok(entries) = fs::read_dir("/sys/class/drm/") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            // Only look at top-level cards (card0, card1, ...), skip card0-DP-1 etc.
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }

            let device_dir = entry.path().join("device");
            if !device_dir.exists() {
                continue;
            }

            // Read the PCI vendor:device pair — these are etched into the silicon
            let vendor = read_sysfs_hex(&device_dir.join("vendor"));
            let device = read_sysfs_hex(&device_dir.join("device"));
            let sub_vendor = read_sysfs_hex(&device_dir.join("subsystem_vendor"));
            let sub_device = read_sysfs_hex(&device_dir.join("subsystem_device"));

            if let (Some(v), Some(d)) = (&vendor, &device) {
                let id = format!(
                    "PCI-{}-{}{}",
                    v,
                    d,
                    match (&sub_vendor, &sub_device) {
                        (Some(sv), Some(sd)) => format!("-{}-{}", sv, sd),
                        _ => String::new(),
                    }
                );
                ids.push(id);
            }
        }
    }

    // ── Strategy 2: nvidia-smi (NVIDIA-specific, gives descriptive string with UUID) ──
    if let Ok(output) = Command::new("nvidia-smi").arg("-L").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                // e.g. "GPU 0: NVIDIA GeForce RTX 3080 (UUID: GPU-abcd...)"
                if !trimmed.is_empty() {
                    let desc = trimmed.to_string();
                    if !ids.iter().any(|existing| desc.contains(existing) || existing.contains(&desc)) {
                        ids.push(desc);
                    }
                }
            }
        }
    }

    // ── Strategy 3: lspci fallback (last resort) ──
    if ids.is_empty() {
        if let Ok(output) = Command::new("lspci").arg("-nn").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Match VGA or 3D controller lines
                if line.contains("VGA") || line.contains("3D controller") {
                    // Extract the PCI slot address (first column, e.g. "01:00.0")
                    if let Some(slot) = line.split_whitespace().next() {
                        ids.push(format!("LSPCI-{}", slot));
                    }
                }
            }
        }
    }

    ids
}

/// Helper: reads a sysfs file like `/sys/class/drm/card0/device/vendor` and returns content trimmed
fn read_sysfs_hex(path: &std::path::Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A robust wrapper that runs the timing analysis `rounds` times and returns
/// the most frequent bucket value per ratio slot (per-bucket majority vote).
pub fn get_robust_cpu_timing_signature(rounds: usize) -> [u64; 6] {
    use std::collections::HashMap;

    // Pin the thread to the first CPU core to prevent the OS scheduler from migrating
    // us to an E-core or a core with different thermal/frequency characteristics mid-flight.
    if let Some(core_ids) = core_affinity::get_core_ids() {
        if let Some(first_core) = core_ids.first() {
            core_affinity::set_for_current(*first_core);
        }
    }

    // Collect all runs
    let mut all_runs: Vec<[u64; 6]> = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        all_runs.push(get_cpu_timing_signature());
    }

    // Per-bucket majority vote: for each of the 6 slots, pick the most frequent value
    let mut result = [0u64; 6];
    for slot in 0..6 {
        let mut tally: HashMap<u64, usize> = HashMap::new();
        for run in &all_runs {
            *tally.entry(run[slot]).or_insert(0) += 1;
        }
        result[slot] = tally.into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(val, _)| val)
            .unwrap_or(0);
    }

    result
}

/// # CPU Silicon Fingerprint — Per-Chip Unique via Micro-Architectural Timing
///
/// ## The Problem
/// CPUs have **no serial number**. Intel killed PSN after Pentium III (privacy backlash).
/// AMD never had it. So two identical i7-12700K chips have no distinguishing software-readable ID.
///
/// ## The Solution: RDTSC Timing Ratios (Enhanced — 6 Workloads)
/// Every chip is manufactured with microscopic transistor variations (process variation).
/// These cause different execution units (ALU, divider, FPU, AES unit, barrel shifter)
/// to run at slightly different speeds **relative to each other**, even on identical models.
///
/// We exploit this by:
/// 1. Measuring 6 different instruction workloads that stress distinct execution units
/// 2. Computing 6 pairwise **ratios** — ratios cancel frequency scaling (stable at any MHz)
/// 3. Quantizing with narrow buckets (width=40, ×100000 precision) for high granularity
/// 4. Hashing all ratio buckets with /proc/cpuinfo model hash
///
/// ## Workloads
/// - Integer multiply chain → ALU multiplier
/// - Integer division chain → hardware divider
/// - FP multiply-add chain → floating-point unit
/// - AES-NI encrypt chain  → AES execution unit
/// - Bit-rotate chain      → barrel shifter
/// - Integer XOR chain     → basic ALU XOR path
///
/// ## Stability
/// - 6 pairwise ratios cancel out frequency scaling
/// - Double-median: median of 2001 samples per workload, then median across 5 passes
/// - Bucket width 70 absorbs per-run jitter while remaining narrow enough for uniqueness
/// - ~millions of distinct bucket combinations (vs ~150 in previous 2-ratio design)
#[cfg(target_arch = "x86_64")]
pub fn get_cpu_timing_signature() -> [u64; 6] {
    use std::arch::x86_64::{__cpuid, __m128i, _mm_aesenc_si128, _mm_set_epi64x};

    const ROUNDS: usize = 2001; // Samples per workload per pass (odd for clean median)
    const CHAIN_LEN: usize = 1000; // Long dependency chains so workload >> measurement overhead
    const PASSES: usize = 5; // Full measurement passes; we take median of ratios
    const PRECISION: f64 = 100000.0; // 10× higher than before for finer fractional resolution
    const BUCKET_WIDTH: u64 = 80; // Narrower than original (100-150) but stable; 6 ratios × ~12 buckets ≈ millions of combos
    const HALF_BUCKET: u64 = BUCKET_WIDTH / 2; // Round-to-nearest offset

    // ── Warmup: bring CPU out of deep C-state ──
    for _ in 0..1000 {
        std::hint::black_box(__cpuid(0));
    }

    // 6 ratio collectors: mul/div, mul/fp, mul/aes, div/fp, div/aes, fp/aes
    let mut ratios: [Vec<u64>; 6] = std::array::from_fn(|_| Vec::with_capacity(PASSES));

    for _ in 0..PASSES {
        // ── Workload 1: Integer multiply chain (tests ALU multiplier) ──
        let t_mul = measure_median(ROUNDS, || {
            let mut x: u64 = 0xDEAD_BEEF_CAFE_BABE;
            for _ in 0..CHAIN_LEN {
                x = x.wrapping_mul(6364136223846793005);
            }
            std::hint::black_box(x);
        });

        // ── Workload 2: Integer division chain (tests hardware divider) ──
        let t_div = measure_median(ROUNDS, || {
            let mut x: u64 = 0xFFFF_FFFF_FFFF_FFFE;
            for _ in 0..CHAIN_LEN {
                x = x / 3;
                x |= 0x8000_0000_0000_0000; // keep x large for full-width division
            }
            std::hint::black_box(x);
        });

        // ── Workload 3: FP multiply-add chain (tests floating-point unit) ──
        let t_fp = measure_median(ROUNDS, || {
            let mut x: f64 = 1.0000001;
            for _ in 0..CHAIN_LEN {
                x = x * 1.0000001 + 0.0000001;
            }
            std::hint::black_box(x);
        });

        // ── Workload 4: AES-NI encrypt chain (tests dedicated AES execution unit) ──
        let t_aes = measure_median(ROUNDS, || {
            unsafe {
                let mut block: __m128i = _mm_set_epi64x(0x0123456789ABCDEF, 0xFEDCBA9876543210u64 as i64);
                let key: __m128i = _mm_set_epi64x(0x0F0E0D0C0B0A0908, 0x0706050403020100);
                for _ in 0..CHAIN_LEN {
                    block = _mm_aesenc_si128(block, key);
                }
                std::hint::black_box(block);
            }
        });

        // ── Workload 5: Bit-rotate chain (tests barrel shifter) ──
        let t_rot = measure_median(ROUNDS, || {
            let mut x: u64 = 0xA5A5_A5A5_5A5A_5A5A;
            for _ in 0..CHAIN_LEN {
                x = x.rotate_left(7);
                x = x.wrapping_add(1); // data dependency to prevent trivial optimization
            }
            std::hint::black_box(x);
        });

        // ── Workload 6: Integer XOR chain (tests basic ALU XOR path) ──
        let t_xor = measure_median(ROUNDS, || {
            let mut x: u64 = 0xCAFE_BABE_DEAD_BEEF;
            for _ in 0..CHAIN_LEN {
                x ^= 0x5555_5555_AAAA_AAAA;
                x = x.wrapping_add(x >> 17); // data dependency
            }
            std::hint::black_box(x);
        });

        // Compute 6 pairwise ratios: mul/div, mul/fp, mul/aes, div/aes, fp/aes, rot/xor
        let all_timings = [t_mul, t_div, t_fp, t_aes, t_rot, t_xor];
        let ratio_pairs = [(0,1), (0,2), (0,3), (1,3), (2,3), (4,5)];

        for (idx, &(a, b)) in ratio_pairs.iter().enumerate() {
            if all_timings[b] > 0 {
                ratios[idx].push((all_timings[a] as f64 / all_timings[b] as f64 * PRECISION) as u64);
            }
        }
    }

    // ── Take the median ratio from across all passes, then bucket ──
    let mut buckets = [0u64; 6];

    for (i, ratio_samples) in ratios.iter_mut().enumerate() {
        ratio_samples.sort();
        let median = *ratio_samples.get(PASSES / 2).unwrap_or(&0);
        buckets[i] = (median + HALF_BUCKET) / BUCKET_WIDTH;
    }

    buckets
}

/// Measures the median RDTSC cycle count for a given workload over `rounds` iterations.
/// Uses CPUID serialization to prevent out-of-order execution from skewing measurements.
#[cfg(target_arch = "x86_64")]
fn measure_median<F: Fn()>(rounds: usize, workload: F) -> u64 {
    use std::arch::x86_64::{__cpuid, _rdtsc};

    let mut timings = Vec::with_capacity(rounds);

    for _ in 0..rounds {
        unsafe {
            // CPUID forces all prior instructions to retire before we read TSC
            __cpuid(0);
            let start = _rdtsc();

            workload();

            __cpuid(0);
            let end = _rdtsc();

            timings.push(end.wrapping_sub(start));
        }
    }

    timings.sort();
    timings[rounds / 2]
}


/// Stable CPU identity fields parsed from /proc/cpuinfo.
/// These identify the CPU model (not per-chip unique) and are stored in the manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CpuInfo {
    pub model_name: String,
    pub cpu_family: String,
    pub model: String,
    pub stepping: String,
    pub microcode: String,
    pub cache_size: String,
    pub cpuid_level: String,
}

/// Reads stable CPU identity fields from /proc/cpuinfo (first logical core block).
pub fn get_cpu_info() -> CpuInfo {
    let mut info = CpuInfo {
        model_name: String::new(),
        cpu_family: String::new(),
        model: String::new(),
        stepping: String::new(),
        microcode: String::new(),
        cache_size: String::new(),
        cpuid_level: String::new(),
    };

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        // Only parse the first processor block
        let first_block = cpuinfo.split("\n\n").next().unwrap_or(&cpuinfo);

        for line in first_block.lines() {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();
                match key {
                    "model name" => info.model_name = value,
                    "cpu family" => info.cpu_family = value,
                    "model" => info.model = value,
                    "stepping" => info.stepping = value,
                    "microcode" => info.microcode = value,
                    "cache size" => info.cache_size = value,
                    "cpuid level" => info.cpuid_level = value,
                    _ => {}
                }
            }
        }
    }

    info
}

/// Measures the median AArch64 internal timer tick count for a given workload.
/// Uses Instruction Synchronization Barriers (`isb sy`) to serialize execution.
#[cfg(target_arch = "aarch64")]
fn measure_median<F: Fn()>(rounds: usize, workload: F) -> u64 {
    use std::arch::asm;

    let mut timings = Vec::with_capacity(rounds);

    for _ in 0..rounds {
        unsafe {
            let start: u64;
            let end: u64;

            // ISB guarantees that instructions preceding the ISB execute before the timer read
            asm!("isb sy");
            asm!("mrs {}, cntvct_el0", out(reg) start);

            workload();

            asm!("isb sy");
            asm!("mrs {}, cntvct_el0", out(reg) end);

            timings.push(end.wrapping_sub(start));
        }
    }

    timings.sort();
    timings[rounds / 2]
}

/// AArch64 implementation of deep silicon fingerprinting via Virtual Timers (`cntvct_el0`).
/// Enhanced with 6 workloads for higher per-chip uniqueness.
/// Works on Snapdragon, Apple M-Series (macOS/Asahi), and AWS Graviton.
#[cfg(target_arch = "aarch64")]
pub fn get_cpu_timing_signature() -> [u64; 6] {
    const ROUNDS: usize = 2001;
    const CHAIN_LEN: usize = 1000;
    const PASSES: usize = 5;
    const PRECISION: f64 = 100000.0;
    const BUCKET_WIDTH: u64 = 80;
    const HALF_BUCKET: u64 = BUCKET_WIDTH / 2;

    // Warmup processor
    for _ in 0..1000 {
        std::hint::black_box(0);
    }

    let mut ratios: [Vec<u64>; 6] = std::array::from_fn(|_| Vec::with_capacity(PASSES));

    for _ in 0..PASSES {
        // ── Workload 1: Integer multiply chain ──
        let t_mul = measure_median(ROUNDS, || {
            let mut x: u64 = 0xDEAD_BEEF_CAFE_BABE;
            for _ in 0..CHAIN_LEN {
                x = x.wrapping_mul(6364136223846793005);
            }
            std::hint::black_box(x);
        });

        // ── Workload 2: Integer division chain ──
        let t_div = measure_median(ROUNDS, || {
            let mut x: u64 = 0xFFFF_FFFF_FFFF_FFFE;
            for _ in 0..CHAIN_LEN {
                x = x / 3;
                x |= 0x8000_0000_0000_0000;
            }
            std::hint::black_box(x);
        });

        // ── Workload 3: FP multiply-add chain ──
        let t_fp = measure_median(ROUNDS, || {
            let mut x: f64 = 1.0000001;
            for _ in 0..CHAIN_LEN {
                x = x * 1.0000001 + 0.0000001;
            }
            std::hint::black_box(x);
        });

        // ── Workload 4: Bit-rotate chain ──
        let t_rot = measure_median(ROUNDS, || {
            let mut x: u64 = 0xA5A5_A5A5_5A5A_5A5A;
            for _ in 0..CHAIN_LEN {
                x = x.rotate_left(7);
                x = x.wrapping_add(1);
            }
            std::hint::black_box(x);
        });

        // ── Workload 5: Integer XOR chain ──
        let t_xor = measure_median(ROUNDS, || {
            let mut x: u64 = 0xCAFE_BABE_DEAD_BEEF;
            for _ in 0..CHAIN_LEN {
                x ^= 0x5555_5555_AAAA_AAAA;
                x = x.wrapping_add(x >> 17);
            }
            std::hint::black_box(x);
        });

        // ── Workload 6: Integer add chain (stands in for AES on non-x86) ──
        let t_add = measure_median(ROUNDS, || {
            let mut x: u64 = 0x1234_5678_9ABC_DEF0;
            for _ in 0..CHAIN_LEN {
                x = x.wrapping_add(0x9E3779B97F4A7C15);
            }
            std::hint::black_box(x);
        });

        // 6 pairwise ratios: mul/div, mul/fp, mul/rot, div/fp, div/rot, xor/add
        let all_timings = [t_mul, t_div, t_fp, t_rot, t_xor, t_add];
        let ratio_pairs = [(0,1), (0,2), (0,3), (1,2), (1,3), (4,5)];

        for (idx, &(a, b)) in ratio_pairs.iter().enumerate() {
            if all_timings[b] > 0 {
                ratios[idx].push((all_timings[a] as f64 / all_timings[b] as f64 * PRECISION) as u64);
            }
        }
    }

    let mut buckets = [0u64; 6];

    for (i, ratio_samples) in ratios.iter_mut().enumerate() {
        ratio_samples.sort();
        let median = *ratio_samples.get(PASSES / 2).unwrap_or(&0);
        buckets[i] = (median + HALF_BUCKET) / BUCKET_WIDTH;
    }

    buckets
}

/// Fallback for unknown architectures (e.g. RISC-V, older ARM32) — returns empty buckets.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn get_cpu_timing_signature() -> [u64; 6] {
    [0u64; 6]
}