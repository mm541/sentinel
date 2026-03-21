# Per-Chip Silicon Fingerprinting

Sentinel relies on Deep Silicon Fingerprinting using the system's `RDTSC` (Read Time-Stamp Counter) on x86 and `cntvct_el0` on AArch64 to extract unique micro-architectural timing characteristics.

## The Theory

On standard operating systems, distinguishing between two exact identical CPUs (e.g. two physical Intel Core i9-14900Ks) is fundamentally impossible through standard software APIs like CPUID or model string comparisons. The silicon manufacturers deliberately deprecated generic hardware serial numbers on consumer silicon two decades ago.

However, no two physical chips come out of the semiconductor fab exactly identical. Due to **process variation** inside the silicon wafers, the physical timing limits of independent execution units fluctuate slightly per chip.

## Mechanism

Sentinel measures these tiny execution time discrepancies by chaining thousands of dependent instructions and timing them down to the cycle.

### 6 Micro-Architectural Workloads

Each workload is a **dependency chain** of 1000 iterations, forcing the CPU to reveal the raw latency of a specific execution unit:

| # | Workload | Execution Unit Tested | x86_64 | AArch64 |
|---|----------|-----------------------|--------|---------|
| 1 | Integer multiply chain | ALU multiplier | ✅ | ✅ |
| 2 | Integer division chain | Hardware divider | ✅ | ✅ |
| 3 | FP multiply-add chain | Floating-point unit | ✅ | ✅ |
| 4 | AES-NI encrypt chain | AES execution unit | ✅ | — (replaced by integer add chain) |
| 5 | Bit-rotate chain | Barrel shifter | ✅ | ✅ |
| 6 | Integer XOR chain | Basic ALU XOR path | ✅ | ✅ |

### 6 Pairwise Ratios Cancel Frequency Interference

Why ratios? Modern platforms continuously adjust CPU clock frequencies (idle vs boost). By timing multiple execution units synchronously and computing their relative performance ratios, Sentinel calculates values that **persist stably** across extreme frequency throttling.

From the 6 workloads, we compute 6 pairwise ratios:

```
ratio[0] = mul / div     (integer multiplier vs divider)
ratio[1] = mul / fp      (integer vs floating-point)
ratio[2] = mul / aes     (integer vs AES unit)
ratio[3] = div / aes     (divider vs AES unit)
ratio[4] = fp  / aes     (FPU vs AES unit)
ratio[5] = rot / xor     (barrel shifter vs ALU XOR)
```

Each ratio is computed at **×100,000 precision** for fine fractional resolution.

### Quantization: Coarse Bucketing

Raw ratios still have minor run-to-run jitter. We quantize each into coarse buckets (width=80, round-to-nearest) to absorb noise while preserving chip-level variation:

```
bucket = (ratio + 40) / 80
```

With 6 ratios and ~12 distinct buckets each, this yields **~millions of distinct combinations** — compared to ~150 in the original 2-ratio design.

The raw bucket values are stored directly in the manifest (e.g., `[88, 75, 33, 474, 552, 1249]`) rather than hashed into a single number. This enables **fuzzy verification** — each bucket is allowed to differ by ±1 during comparison, absorbing boundary jitter without sacrificing uniqueness.

### The 100-Run Majority Vote Filter

Because multitasking OSes constantly interrupt threads for system scheduling, raw timing metrics are occasionally disrupted.

Sentinel prevents this using a 100-run voting system:

1. **Thread Affinitization:** Pin the measuring thread strictly to physical Core 0 to prevent P-core/E-core migration or cache-invalidation mid-measurement.
2. Fire 100 consecutive measurement sweeps.
3. Inside each sweep, collect 2001 iterations per workload across 5 passes and resolve the **double-median** (median of samples, then median of ratios across passes).
4. Quantize the 6 ratios into logical bins (buckets) to absorb thermal and scheduling jitter.
5. **Per-bucket majority vote:** For each of the 6 ratio slots independently, select the bucket value that appears most frequently across the 100 runs.
6. Store the resulting 6-element bucket array as the CPU timing signature.

### Fuzzy Verification (±1 Tolerance)

During verification, each of the 6 stored buckets is compared against the live scan. A bucket is considered matching if it differs by **at most ±1** from the baseline value. This eliminates false positives from boundary jitter while still detecting genuinely different CPUs (which differ by many buckets).

### CPU Identity Fields

In addition to the timing signature, Sentinel stores stable CPU identity fields parsed from `/proc/cpuinfo`:

| Field | Example | Purpose |
|-------|---------|----------|
| `model_name` | 12th Gen Intel Core i7-12700H | Human-readable chip name |
| `cpu_family` | 6 | Architecture family |
| `model` | 154 | Specific model number |
| `stepping` | 3 | Silicon revision |
| `microcode` | 0x43b | Firmware version |
| `cache_size` | 24576 KB | L3 cache |
| `cpuid_level` | 32 | Max CPUID leaf |

These fields detect CPU swaps (different model/stepping) with exact matching, complementing the timing signature which detects per-chip differences within the same model.

### Architecture Support

| Architecture | Timer | Serialization | AES Workload |
|-------------|-------|---------------|-------------|
| x86_64 | `RDTSC` | `CPUID` barrier | `_mm_aesenc_si128` (AES-NI) |
| AArch64 | `cntvct_el0` | `ISB SY` barrier | Integer add chain (fallback) |
| Other | — | — | Fallback: returns zero buckets |

The final output is a **6-element array of bucket values** stored directly in the manifest JSON, enabling transparent inspection and fuzzy per-bucket verification.
