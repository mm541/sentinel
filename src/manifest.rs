use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct HardwareManifest {
    pub machine_id: String,
    pub motherboard_serial: Option<String>,
    pub cpu_timing_signature: u64,
    pub ram_serials: Vec<String>,
    pub drive_serials: Vec<String>,
    pub gpu_uuids: Vec<String>,
    pub mac_addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffStatus<T> {
    Unchanged,
    Modified { expected: T, actual: T },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionDiff {
    pub unchanged: Vec<String>,
    pub added: Vec<String>,
    pub missing: Vec<String>,
}

impl CollectionDiff {
    pub fn is_identical(&self) -> bool {
        self.added.is_empty() && self.missing.is_empty()
    }
}

#[derive(Debug)]
pub struct ManifestDiff {
    pub is_identical: bool,
    pub motherboard: DiffStatus<Option<String>>,
    pub cpu: DiffStatus<u64>,
    pub ram: CollectionDiff,
    pub drives: CollectionDiff,
    pub gpus: CollectionDiff,
    pub macs: CollectionDiff,
}

impl HardwareManifest {
    /// Serializes the struct to pretty JSON and writes it to disk
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let json_string = serde_json::to_string_pretty(self)
            .context("Failed to serialize manifest to JSON")?;
        fs::write(path, json_string)
            .context(format!("Failed to write manifest to {}", path))?;
        Ok(())
    }

    /// Reads a JSON file from disk and parses it back into the Rust struct
    pub fn load_from_file(path: &str) -> Result<Self> {
        let file_content = fs::read_to_string(path)
            .context(format!("Failed to read manifest file from {}", path))?;
        let manifest: HardwareManifest = serde_json::from_str(&file_content)
            .context("Failed to parse manifest JSON")?;
        Ok(manifest)
    }

    /// Compares this baseline manifest with a live manifest to detail all tampering or changes
    pub fn compare(&self, live: &HardwareManifest) -> ManifestDiff {
        let mobo_diff = if self.motherboard_serial == live.motherboard_serial {
            DiffStatus::Unchanged
        } else {
            DiffStatus::Modified {
                expected: self.motherboard_serial.clone(),
                actual: live.motherboard_serial.clone(),
            }
        };

        let cpu_diff = if self.cpu_timing_signature == live.cpu_timing_signature {
            DiffStatus::Unchanged
        } else {
            DiffStatus::Modified {
                expected: self.cpu_timing_signature,
                actual: live.cpu_timing_signature,
            }
        };

        let ram_diff = Self::compare_collections(&self.ram_serials, &live.ram_serials);
        let drive_diff = Self::compare_collections(&self.drive_serials, &live.drive_serials);
        let gpu_diff = Self::compare_collections(&self.gpu_uuids, &live.gpu_uuids);
        let mac_diff = Self::compare_collections(&self.mac_addresses, &live.mac_addresses);

        let is_identical = mobo_diff == DiffStatus::Unchanged
            && cpu_diff == DiffStatus::Unchanged
            && ram_diff.is_identical()
            && drive_diff.is_identical()
            && gpu_diff.is_identical()
            && mac_diff.is_identical();

        ManifestDiff {
            is_identical,
            motherboard: mobo_diff,
            cpu: cpu_diff,
            ram: ram_diff,
            drives: drive_diff,
            gpus: gpu_diff,
            macs: mac_diff,
        }
    }

    /// Helper to compare two vectors of strings treating them as sets
    fn compare_collections(baseline: &[String], live: &[String]) -> CollectionDiff {
        let base_set: HashSet<_> = baseline.iter().collect();
        let live_set: HashSet<_> = live.iter().collect();

        CollectionDiff {
            unchanged: base_set.intersection(&live_set).map(|&s| s.clone()).collect(),
            added: live_set.difference(&base_set).map(|&s| s.clone()).collect(),
            missing: base_set.difference(&live_set).map(|&s| s.clone()).collect(),
        }
    }
}