# Sentinel Roadmap & Feature Wishlist

This document outlines planned features and improvements for Sentinel. **Contributors are welcome to pick up any of these!** If you're interested in working on one, open an issue first so we can coordinate.

> Difficulty ratings: 🟢 Beginner-friendly · 🟡 Intermediate · 🔴 Advanced

---

## 🔒 Security Hardening

### HMAC Manifest Signing 🟡
**Problem:** The manifest is plain JSON — an attacker who swaps hardware can simply edit the manifest to match.
**Solution:** Add an HMAC-SHA256 signature field to the manifest, keyed with a user-supplied secret. `sentinel generate --key <secret>` signs on creation. `sentinel verify --key <secret>` validates the signature before comparing hardware.
**Files:** `manifest.rs`, `main.rs`

### Encrypted Manifest Storage 🟡
**Problem:** A readable manifest reveals exactly which hardware serials to spoof.
**Solution:** Encrypt the manifest at rest using AES-256-GCM with a passphrase-derived key (Argon2). Decrypt transparently during `verify`.
**Files:** `manifest.rs`, new `crypto.rs` module

---

## 🖥️ Additional Hardware Vectors

### BIOS/UEFI Version Tracking 🟢
**Source:** `/sys/class/dmi/id/bios_version`, `/sys/class/dmi/id/bios_date`, `/sys/class/dmi/id/bios_vendor`
**Value:** Detects firmware tampering or unauthorized BIOS updates.
**Files:** `sys_info/linux.rs`, `manifest.rs`, `main.rs`

### TPM Endorsement Key 🟡
**Source:** `/dev/tpm0` via `tpm2-tools` or the `tss2` crate
**Value:** Factory-burned, truly unique per-chip identifier. The gold standard for hardware identity — complements the RDTSC timing approach.
**Files:** `sys_info/linux.rs`, `manifest.rs`

### USB Controller Fingerprinting 🟢
**Source:** `/sys/bus/usb/devices/` — track root hub controllers (not removable devices)
**Value:** Detects rogue USB controller insertions. Relevant for kiosk/ATM/POS environments.
**Files:** `sys_info/linux.rs`, `manifest.rs`, `main.rs`

### Battery Serial (Laptops) 🟢
**Source:** `/sys/class/power_supply/BAT*/serial_number`
**Value:** Additional unique identifier on laptop hardware.
**Files:** `sys_info/linux.rs`, `manifest.rs`

---

## 📊 Operational Features

### Daemon Mode (`sentinel watch`) 🟡
**What:** Run as a background service that periodically re-checks hardware and fires alerts on change.
**Output:** Syslog, webhook, or custom command execution on tamper detection.
**Implementation:** New subcommand with configurable interval, alert targets, and optional systemd unit file.

### Offline Manifest Diff (`sentinel diff`) 🟢
**What:** Compare two manifest JSON files without live hardware access.
**Usage:** `sentinel diff baseline.json updated.json`
**Value:** Audit trail analysis, fleet inventory management.
**Files:** `main.rs` (new subcommand), reuses existing `ManifestDiff::compare()`

### Machine-Readable Output (`--json` / `--format`) 🟢
**What:** Add `--format json` flag to `generate` and `verify` commands.
**Value:** Enables integration with CI/CD pipelines, monitoring dashboards, and SIEM systems.
**Files:** `main.rs`

### Manifest Versioning 🟢
**What:** Add a `"version": 2` field to the JSON manifest schema.
**Value:** Enables graceful migration when the manifest format changes, instead of crashing on parse errors.
**Files:** `manifest.rs`

---

## 🏗️ Platform Expansion

### Windows Backend 🔴
**Module:** `sys_info/windows.rs`
**Approach:**
- Motherboard/RAM/Drive serials via WMI (`wmic` or `windows-rs` crate)
- GPU UUIDs via DXGI adapter enumeration
- MAC addresses via `GetAdaptersAddresses` Win32 API
- CPU timing via `__rdtsc` intrinsic (same RDTSC approach, different serialization)
**Note:** The `sys_info/mod.rs` architecture already supports conditional compilation per OS.

### macOS Backend 🔴
**Module:** `sys_info/macos.rs`
**Approach:**
- Hardware serials via `system_profiler` / IOKit
- MAC addresses via `ifconfig` or IOKit
- CPU timing via `mach_absolute_time` or `cntvct_el0` on Apple Silicon
**Note:** Apple Silicon (M-series) uses AArch64, so the existing `aarch64` timing code works directly.

---

## Contributing

1. **Pick a feature** from this list
2. **Open an issue** to claim it and discuss approach
3. **Fork & implement** following the existing patterns in `sys_info/linux.rs`
4. **Test** with `cargo build --release` and manual verification
5. **Submit a PR** with updated docs if applicable

All new hardware vectors should follow the established pattern:
- Collection function in `sys_info/linux.rs` (or platform-specific module)
- Field in `HardwareManifest` struct in `manifest.rs`
- Display + diff reporting in `main.rs`
- Documentation update if adding a new category
