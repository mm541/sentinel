# Detailed Diff Engine

At the core of tampered environment detection sits the `ManifestDiff` module. Instead of simply performing a binary YES/NO hash check, Sentinel's Verification Engine computes precise Set Theory differences across every hardware vector.

```mermaid
flowchart LR
    A[Baseline Manifest] --> C{ManifestDiff.compare}
    B[Live Scan Results] --> C

    C --> D["Missing Components (-)"]
    C --> E["Added Components (+)"]
    C --> F["Modified Strings (M)"]
    C --> G["Identical Sets (OK)"]

    D --> H["Alert: Hardware Stripped"]
    E --> I["Alert: Rogue USB/PCI inserted"]
    F --> J["Alert: Spoofing Attempt"]
```

## How It Works

1. **Extraction:** Sentinel parses the persisted Baseline JSON configuration.
2. **Re-Scan:** It queries the live host environment mapping hardware components into memory.
3. **Comparison:** It diffs collections of drives, RAM DIMMs, MAC arrays, and GPU arrays tracking exactly what was inserted (rogue PCI devices) or removed (stripped RAM). CPU identity fields are compared exactly, while CPU timing signature buckets use ±1 fuzzy tolerance.
4. **Resolution:** Emits a granular incident report to stdout before cleanly exiting with status `1` upon tamper detection.
