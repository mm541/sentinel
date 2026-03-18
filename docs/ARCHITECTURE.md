# Sentinel Architecture

Sentinel utilizes a unified core domain model mapped strictly to generic OS traits. Below is a high-level representation of the module topography.

![Sentinel Architecture](assets/sentinel_architecture.png)

## Security & Privacy Considerations

Manifest collections identify the hardware definitively. For environments sharing security telemetry, privacy tokens (like `SHA256`) should wrap specific serial strings before transmitting payloads containing `HardwareManifest` models.
