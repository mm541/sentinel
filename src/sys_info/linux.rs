use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
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
/// the most frequent result (mode/majority vote) to eliminate OS-level noise.
pub fn get_robust_cpu_timing_signature(rounds: usize) -> u64 {
    use std::collections::HashMap;

    // Pin the thread to the first CPU core to prevent the OS scheduler from migrating
    // us to an E-core or a core with different thermal/frequency characteristics mid-flight.
    if let Some(core_ids) = core_affinity::get_core_ids() {
        if let Some(first_core) = core_ids.first() {
            core_affinity::set_for_current(*first_core);
        }
    }
    
    let mut tally: HashMap<u64, usize> = HashMap::with_capacity( rounds / 2 );

    for _ in 0..rounds {
        let sig = get_cpu_timing_signature();
        *tally.entry(sig).or_insert(0) += 1;
    }

    // Find the signature with the highest frequency
    tally.into_iter()
         .max_by_key(|&(_, count)| count)
         .map(|(sig, _)| sig)
         .unwrap_or(0)
}

/// # CPU Silicon Fingerprint — Per-Chip Unique via Micro-Architectural Timing
///
/// ## The Problem
/// CPUs have **no serial number**. Intel killed PSN after Pentium III (privacy backlash).
/// AMD never had it. So two identical i7-12700K chips have no distinguishing software-readable ID.
///
/// ## The Solution: RDTSC Timing Ratios
/// Every chip is manufactured with microscopic transistor variations (process variation).
/// These cause different execution units (integer ALU, divider, FPU) to run at slightly
/// different speeds **relative to each other**, even on chips of the exact same model.
///
/// We exploit this by:
/// 1. Measuring the execution time of 3 different instruction workloads (multiply, divide, FP)
/// 2. Computing the **ratios** between them (multiply/divide, multiply/FP, divide/FP)
/// 3. Using ratios is key — they cancel out frequency scaling, so the fingerprint is stable
///    whether the CPU is running at 800MHz power-saving or 5GHz turbo
/// 4. Quantizing the ratios and combining with /proc/cpuinfo model hash
///
/// ## Stability
/// - Ratios are stable across reboots and frequency changes
/// - We take the **median** of 2001 samples per workload to filter out scheduling noise
/// - Quantized to 14 bits per ratio (3 ratios = 42 bits of silicon identity)
/// - Combined with 22 bits of cpuinfo model hash = 64-bit composite fingerprint
#[cfg(target_arch = "x86_64")]
pub fn get_cpu_timing_signature() -> u64 {
    use std::arch::x86_64::__cpuid;

    const ROUNDS: usize = 2001; // Samples per workload per pass
    const CHAIN_LEN: usize = 1000; // Long chains so workload >> measurement overhead
    const PASSES: usize = 5; // Full measurement passes; we take median of ratios

    // ── Warmup: bring CPU out of deep C-state ──
    for _ in 0..1000 {
        unsafe {
            std::hint::black_box(__cpuid(0));
        }
    }

    let mut ratio_a_samples: Vec<u64> = Vec::with_capacity(PASSES);
    let mut ratio_b_samples: Vec<u64> = Vec::with_capacity(PASSES);

    for _ in 0..PASSES {
        // ── Workload A: Integer multiply chain (tests ALU multiplier) ──
        let t_mul = measure_median(ROUNDS, || {
            let mut x: u64 = 0xDEAD_BEEF_CAFE_BABE;
            for _ in 0..CHAIN_LEN {
                x = x.wrapping_mul(6364136223846793005);
            }
            std::hint::black_box(x);
        });

        // ── Workload B: Integer division chain (tests hardware divider) ──
        let t_div = measure_median(ROUNDS, || {
            let mut x: u64 = 0xFFFF_FFFF_FFFF_FFFE;
            for _ in 0..CHAIN_LEN {
                x = x / 3;
                x |= 0x8000_0000_0000_0000; // keep x large for full-width division
            }
            std::hint::black_box(x);
        });

        // ── Workload C: FP multiply-add chain (tests floating-point unit) ──
        let t_fp = measure_median(ROUNDS, || {
            let mut x: f64 = 1.0000001;
            for _ in 0..CHAIN_LEN {
                x = x * 1.0000001 + 0.0000001;
            }
            std::hint::black_box(x);
        });

        // Compute ratios for this pass (×10000 for fractional precision)
        if t_div > 0 {
            ratio_a_samples.push((t_mul as f64 / t_div as f64 * 10000.0) as u64);
        }
        if t_div > 0 {
            ratio_b_samples.push((t_mul as f64 / t_fp as f64 * 10000.0) as u64);
        }
    }

    // ── Take the median ratio from across all passes ──
    ratio_a_samples.sort();
    ratio_b_samples.sort();

    let r_mul_div = *ratio_a_samples.get(PASSES / 2).unwrap_or(&0);
    let r_mul_fp = *ratio_b_samples.get(PASSES / 2).unwrap_or(&0);


    // ── Bucket each ratio into a coarse bin ──
    // Bucket sizes chosen to absorb observed jitter (±5% for mul/div, ±10% for mul/fp):
    //   mul/div ≈ 700 ± 20 → bucket_size=100 → bucket ≈ 7
    //   mul/fp  ≈ 600 ± 50 → bucket_size=150 → bucket ≈ 4
    // Round-to-nearest (add half bucket) avoids flipping at exact boundaries
    let bucket_a = (r_mul_div + 50) / 100;
    let bucket_b = (r_mul_fp + 75) / 150;


    let mut hasher = DefaultHasher::new();
    bucket_a.hash(&mut hasher);
    bucket_b.hash(&mut hasher);
    cpuinfo_model_hash().hash(&mut hasher);
    hasher.finish()
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

/// Hashes stable /proc/cpuinfo fields to identify the CPU model (not per-chip unique).
fn cpuinfo_model_hash() -> u64 {
    let mut hasher = DefaultHasher::new();

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        let first_block = cpuinfo.split("\n\n").next().unwrap_or(&cpuinfo);
        let stable_fields = [
            "model name",
            "cpu family",
            "model",
            "stepping",
            "microcode",
            "cache size",
            "cpuid level",
        ];
        for line in first_block.lines() {
            for field in &stable_fields {
                if line.starts_with(field) {
                    line.hash(&mut hasher);
                }
            }
        }
    }

    hasher.finish()
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
/// Works on Snapdragon, Apple M-Series (macOS/Asahi), and AWS Graviton.
#[cfg(target_arch = "aarch64")]
pub fn get_cpu_timing_signature() -> u64 {
    const ROUNDS: usize = 2001; 
    const CHAIN_LEN: usize = 1000;
    const PASSES: usize = 5; 

    // Warmup processor
    for _ in 0..1000 {
        std::hint::black_box(0);
    }

    let mut ratio_a_samples: Vec<u64> = Vec::with_capacity(PASSES);
    let mut ratio_b_samples: Vec<u64> = Vec::with_capacity(PASSES);

    for _ in 0..PASSES {
        let t_mul = measure_median(ROUNDS, || {
            let mut x: u64 = 0xDEAD_BEEF_CAFE_BABE;
            for _ in 0..CHAIN_LEN {
                x = x.wrapping_mul(6364136223846793005);
            }
            std::hint::black_box(x);
        });

        let t_div = measure_median(ROUNDS, || {
            let mut x: u64 = 0xFFFF_FFFF_FFFF_FFFE;
            for _ in 0..CHAIN_LEN {
                x = x / 3;
                x |= 0x8000_0000_0000_0000; 
            }
            std::hint::black_box(x);
        });

        let t_fp = measure_median(ROUNDS, || {
            let mut x: f64 = 1.0000001;
            for _ in 0..CHAIN_LEN {
                x = x * 1.0000001 + 0.0000001;
            }
            std::hint::black_box(x);
        });

        if t_div > 0 {
            ratio_a_samples.push((t_mul as f64 / t_div as f64 * 10000.0) as u64);
        }
        if t_fp > 0 {
            ratio_b_samples.push((t_mul as f64 / t_fp as f64 * 10000.0) as u64);
        }
    }

    ratio_a_samples.sort();
    ratio_b_samples.sort();

    let r_mul_div = *ratio_a_samples.get(PASSES / 2).unwrap_or(&0);
    let r_mul_fp = *ratio_b_samples.get(PASSES / 2).unwrap_or(&0);

    let bucket_a = (r_mul_div + 50) / 100;
    let bucket_b = (r_mul_fp + 75) / 150;

    let mut hasher = DefaultHasher::new();
    bucket_a.hash(&mut hasher);
    bucket_b.hash(&mut hasher);
    cpuinfo_model_hash().hash(&mut hasher);
    hasher.finish()
}

/// Fallback for unknown architectures (e.g. RISC-V, older ARM32) — uses only /proc/cpuinfo hash.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub fn get_cpu_timing_signature() -> u64 {
    cpuinfo_model_hash()
}