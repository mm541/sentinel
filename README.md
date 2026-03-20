# Sentinel: Hardware Fingerprint Scanner & Verification Engine

Sentinel is a zero-trust, OS-agnostic hardware fingerprinting and verification tool designed to uniquely identify machines and detect hardware tampering. It combines standard hardware serials (Motherboard, MAC, RAM, GPU, NVMe) with deep silicon fingerprinting via micro-architectural timing analysis.

## Key Features

- **Per-Chip CPU Identification:** Exploits manufacturing process variation by profiling 6 distinct execution units (ALU, divider, FPU, AES, barrel shifter, XOR) and computing 6 pairwise timing ratios via RDTSC — yielding millions of distinct fingerprint combinations per CPU model with near 100% cross-run reproducibility.
- **Set-Theory Hardware Diffing:** Granular tamper detection that identifies exactly which components were added, removed, or spoofed across boot cycles.
- **Vendor & OS Agnostic Design:** Pluggable backend architecture designed from day one to support Linux, Windows, and macOS (currently fully implemented for Linux).
- **Graceful Degradation:** Capable of running without root bounds for standard metrics, or extracting hidden DMI/SMBIOS serials when running elevated.
- **Beautiful CLI Reporting:** Powered by `clap` and `colored` for clean, actionable output.

## Installation

```bash
git clone https://github.com/your-repo/sentinel.git
cd sentinel
cargo build --release
```

## Usage

Sentinel uses a dual-command subcommand structure:

### 1. Generating a Baseline Manifest

Scan the current physical machine and save its hardware identity as a JSON manifest.
_Note: Run with `sudo` to extract hidden SMBIOS / motherboard serials._

```bash
sudo ./target/release/sentinel generate config.json
```

### 2. Verifying Integrity (Tamper Detection)

Audit the live machine against a previously generated manifest. If the hardware has changed, or if someone flashes a fake MAC address, Sentinel isolates and reports the exact vector of compromise.

```bash
sudo ./target/release/sentinel verify config.json
```

If the hardware is identical, the process exits with `0`. If hardware drifts or is tampered with, it exits with `1` and prints a detailed diff report.

## Documentation

For deep technical dives into how Sentinel's anti-cheat/anti-tamper mechanics work, see the documentation cluster:

- [System Architecture](docs/ARCHITECTURE.md) - Explains the high-level design and OS-agnostic hardware abstraction layer.
- [Verification Engine](docs/VERIFICATION_ENGINE.md) - Details the set-theory mechanics behind the ManifestDiff tamper detection system.
- [Per-Chip Silicon Fingerprinting](docs/CPU_FINGERPRINTING.md) - Explains how Sentinel breaks the "identical CPU" barrier using RDTSC timing variance.

## Contributing

Sentinel accepts PRs for Windows (`sys_info/windows.rs`) and macOS (`sys_info/macos.rs`) implementations! Please ensure all new modules conform to the generic traits established in the core engine.
