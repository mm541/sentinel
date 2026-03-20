mod manifest;
mod sys_info;

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use manifest::{DiffStatus, HardwareManifest};
use std::process::exit;

#[derive(Parser, Debug)]
#[command(
    name = "Sentinel",
    version = "0.2.0",
    about = "Hardware Fingerprint Scanner and Verification Tool",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generates a hardware manifest and saves it to a file
    Generate {
        /// Output path for the JSON manifest
        #[arg(default_value = "sentinel_manifest.json")]
        output: String,
    },
    /// Verifies live hardware against a stored baseline manifest
    Verify {
        /// Path to the stored baseline JSON manifest
        #[arg(required = true)]
        file: String,
    },
    /// Profiles the CPU fingerprint logic over 100 runs and prints the distribution percentages
    Distribution,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("{}", "\n🔍 Sentinel — Hardware Fingerprint Scanner".bold().cyan());
    println!("{}", "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n".magenta());

    match &cli.command {
        Commands::Generate { output } => {
            println!("{}", "➤ Scanning hardware...".yellow());
            let manifest = collect_hardware_manifest();
            print_manifest(&manifest);

            println!("\n💾 Saving manifest to '{}'...", output.bold());
            manifest.save_to_file(output)?;
            println!("{}", "   ✅ Saved successfully.".green().bold());
        }
        Commands::Verify { file } => {
            println!("📖 Loading baseline from '{}'...", file.bold());
            let baseline = HardwareManifest::load_from_file(file).unwrap_or_else(|e| {
                eprintln!("{} {}", "❌ Error loading baseline:".red().bold(), e);
                exit(1);
            });

            println!("{}", "➤ Scanning live hardware...".yellow());
            let live = collect_hardware_manifest();

            println!("\n🔎 {}...", "Verification Report".bold().cyan());
            let diff = baseline.compare(&live);

            print_diff_report(&diff);

            if diff.is_identical {
                println!("\n{} Hardware matches the baseline precisely. Integrity verified.\n", "✅ PASS:".green().bold());
            } else {
                eprintln!("\n{} Hardware modification detected! Integrity compromised.\n", "❌ FAIL:".red().bold());
                exit(1);
            }
        }
        Commands::Distribution => {
            use std::collections::HashMap;

            let core_ids = core_affinity::get_core_ids().unwrap_or_default();
            let num_cores = core_ids.len();

            println!("➤ {}...", format!("Per-Core CPU Fingerprint Analysis ({} cores detected)", num_cores).yellow());
            println!("{}", "━".repeat(70));

            let runs_per_core = 10;
            let mut core_results: Vec<(usize, u64)> = Vec::new();

            for (i, core_id) in core_ids.iter().enumerate() {
                // Pin thread to this specific core
                core_affinity::set_for_current(*core_id);

                let mut tally: HashMap<u64, usize> = HashMap::new();
                for _ in 0..runs_per_core {
                    let sig = sys_info::get_cpu_timing_signature();
                    *tally.entry(sig).or_insert(0) += 1;
                }

                // Get the majority signature for this core
                let (best_sig, best_count) = tally.into_iter()
                    .max_by_key(|&(_, count)| count)
                    .unwrap_or((0, 0));

                println!("  Core {:>2}  │  0x{:016X}  │  {}/{} stable",
                    i, best_sig, best_count, runs_per_core);

                core_results.push((i, best_sig));
            }

            // ── Summary: group cores by their signature ──
            println!("\n{}", "━".repeat(70));
            println!("📊 {}\n", "Cross-Core Summary:".bold().cyan());

            let mut sig_groups: HashMap<u64, Vec<usize>> = HashMap::new();
            for (core_idx, sig) in &core_results {
                sig_groups.entry(*sig).or_default().push(*core_idx);
            }

            let mut sorted_groups: Vec<_> = sig_groups.into_iter().collect();
            sorted_groups.sort_by_key(|(_, cores)| std::cmp::Reverse(cores.len()));

            for (sig, cores) in &sorted_groups {
                let core_list: Vec<String> = cores.iter().map(|c| format!("{}", c)).collect();
                let pct = (cores.len() as f64 / num_cores as f64) * 100.0;
                let label = if cores.len() == num_cores {
                    "ALL CORES".green().bold().to_string()
                } else {
                    format!("{} cores", cores.len())
                };
                println!("  0x{:016X}  │  {:>5.1}%  │  {} [{}]",
                    sig, pct, label, core_list.join(", "));
            }

            if sorted_groups.len() == 1 {
                println!("\n{} {}\n",
                    "✅".green(),
                    "PERFECT: All cores produce the same chip-level signature!".green().bold());
            } else {
                println!("\n{} {}",
                    "⚠️".yellow(),
                    "HETEROGENEOUS: Different core types detected (e.g. P-cores vs E-cores).".yellow().bold());
                println!("   This is expected on hybrid architectures like Intel 12th Gen+ (Alder Lake).\n");
            }
        }
    }

    Ok(())
}

use std::hash::{DefaultHasher, Hash, Hasher};

fn collect_hardware_manifest() -> HardwareManifest {
    let motherboard_serial = sys_info::get_motherboard_serial();
    let cpu_timing_signature = sys_info::get_robust_cpu_timing_signature(100);
    let ram_serials = sys_info::get_ram_serials();
    let drive_serials = sys_info::get_drive_serials();
    let gpu_uuids = sys_info::get_gpu_uuids();
    let mut mac_addresses = sys_info::get_mac_addresses();
    
    // Sort macs for deterministic hashing regardless of enumeration order
    mac_addresses.sort();

    // Generate a deterministic machine ID based tightly on immutable hardware
    let mut hasher = DefaultHasher::new();
    if let Some(ref serial) = motherboard_serial {
        serial.hash(&mut hasher);
    }
    cpu_timing_signature.hash(&mut hasher);
    for mac in &mac_addresses {
        mac.hash(&mut hasher);
    }
    let machine_id = format!("HW-{:016X}", hasher.finish());

    HardwareManifest {
        machine_id,
        motherboard_serial,
        cpu_timing_signature,
        ram_serials,
        drive_serials,
        gpu_uuids,
        mac_addresses,
    }
}

fn print_manifest(manifest: &HardwareManifest) {
    println!("\n📋 {}", "Collected Identifiers:".bold());

    match &manifest.motherboard_serial {
        Some(s) => println!("  Motherboard Serial : {}", s.cyan()),
        None    => println!("  Motherboard Serial : {}", "⚠ Not available".yellow()),
    }

    println!("  CPU Signature      : 0x{:016X}", manifest.cpu_timing_signature);

    if manifest.ram_serials.is_empty() {
        println!("  RAM Serials        : {}", "⚠ None found".yellow());
    } else {
        for (i, s) in manifest.ram_serials.iter().enumerate() {
            println!("  RAM Serial [{}]     : {}", i, s.cyan());
        }
    }

    if manifest.drive_serials.is_empty() {
        println!("  Drive Serials      : {}", "⚠ None found".yellow());
    } else {
        for (i, s) in manifest.drive_serials.iter().enumerate() {
            println!("  Drive Serial [{}]   : {}", i, s.cyan());
        }
    }

    if manifest.gpu_uuids.is_empty() {
        println!("  GPU UUIDs          : {}", "⚠ None found".yellow());
    } else {
        for (i, u) in manifest.gpu_uuids.iter().enumerate() {
            println!("  GPU UUID [{}]       : {}", i, u.cyan());
        }
    }

    if manifest.mac_addresses.is_empty() {
        println!("  MAC Addresses      : {}", "⚠ None found".yellow());
    } else {
        for (i, m) in manifest.mac_addresses.iter().enumerate() {
            println!("  MAC Address [{}]    : {}", i, m.cyan());
        }
    }
}

fn print_diff_report(diff: &manifest::ManifestDiff) {
    // Motherboard
    match &diff.motherboard {
         DiffStatus::Unchanged => println!("  [{}] Motherboard Serial", "OK  ".green()),
         DiffStatus::Modified { expected, actual } => {
             println!("  [{}] Motherboard Serial", "MOD ".red());
             println!("      Expected: {:?}", expected);
             println!("      Actual:   {:?}", actual);
         }
    }

    // CPU
    match &diff.cpu {
         DiffStatus::Unchanged => println!("  [{}] CPU Timing Signature", "OK  ".green()),
         DiffStatus::Modified { expected, actual } => {
             println!("  [{}] CPU Timing Signature", "MOD ".red());
             println!("      Expected: 0x{:016X}", expected);
             println!("      Actual:   0x{:016X}", actual);
         }
    }

    // RAM
    print_collection_diff("RAM Serials", &diff.ram);
    // Drives
    print_collection_diff("Storage Drives", &diff.drives);
    // GPUs
    print_collection_diff("GPU UUIDs", &diff.gpus);
    // MACs
    print_collection_diff("MAC Addresses", &diff.macs);
}

fn print_collection_diff(label: &str, collection: &manifest::CollectionDiff) {
    if collection.is_identical() {
        println!("  [{}] {}", "OK  ".green(), label);
    } else {
        println!("  [{}] {}", "MOD ".red(), label);
        for missing in &collection.missing {
            println!("      {} {}", "-".red().bold(), missing.red());
        }
        for added in &collection.added {
            println!("      {} {}", "+".green().bold(), added.green());
        }
    }
}
