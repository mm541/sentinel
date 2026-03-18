# Sentinel Architecture

Sentinel utilizes a unified core domain model mapped strictly to generic OS traits. Below is a high-level representation of the module topography.

```mermaid
graph TD
    classDef generic fill:#2a4,stroke:#333,stroke-width:2px,color:#fff;
    classDef specific fill:#07d,stroke:#333,stroke-width:2px,color:#fff;

    A[main.rs<br>CLI Parser & Engine] --> B(HardwareManifest<br>Domain Object)
    A --> C(ManifestDiff<br>Set Theory Engine)
    A --> D{sys_info<br>Environment Abstraction Layer}
    D -.->|Trait Interface| E[linux.rs]:::specific
    D -.->|Trait Interface| F[windows.rs<br>Planned]:::generic
    D -.->|Trait Interface| G[macos.rs<br>Planned]:::generic

    E --> H(DMI Decoders)
    E --> I(udev & Block Devices)
    E --> J(RDTSC Assembly & Timers)
```

## Security & Privacy Considerations

Manifest collections identify the hardware definitively. For environments sharing security telemetry, privacy tokens (like `SHA256`) should wrap specific serial strings before transmitting payloads containing `HardwareManifest` models.

## Detailed Diff Engine

At the core of tampered environment detection sits the `ManifestDiff` module:

```mermaid
flowchart LR
    A[Baseline Manifest] --> C{ManifestDiff.compare}
    B[Live Scan Results] --> C

    C --> D[Missing Components (-)]
    C --> E[Added Components (+)]
    C --> F[Modified Strings (M)]
    C --> G[Identical Sets (OK)]

    D --> H[Alert: Hardware Stripped]
    E --> I[Alert: Rogue USB/PCI inserted]
    F --> J[Alert: Spoofing Attempt]
```
