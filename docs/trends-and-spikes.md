# Trends and Spikes Detection

Every aggregated metric field that produces `avg`, `min`, and `max` also includes two additional signals: `spike` and `trend`. These are computed from the raw sample values collected during the aggregation window before they are summarized.

## Where It Applies

Detection runs on all variable numeric fields — those that are not constant within a window:

| Metric | Fields |
|--------|--------|
| LoadAverage | `load_1min`, `load_5min`, `load_15min` |
| Memory | `available_mb`, `used_percent`, `swap_used_percent` |
| Docker (per container) | `cpu_percent`, `memory_used_mb`, `memory_percent` |

Fields that do not change within a window (`cpu_cores`, `total_mb`, `swap_total_mb`) are stored as plain values and do not get spike or trend.

DiskSpace falls back to a raw document because its data is an array structure, not flat numerics — no detection runs for it.

## Output Shape

Each variable field in the stored document becomes a subdocument:

```json
"load_1min": {
  "avg": 1.5,
  "min": 1.2,
  "max": 1.8,
  "spike": true,
  "trend": "rising"
}
```

`spike` is a boolean. `trend` is one of three strings: `"rising"`, `"falling"`, or `"stable"`.

---

## Spike Detection

**Question answered:** did a sudden burst occur during this window?

### Algorithm

1. Compute the mean of all samples in the window.
2. Compute the population standard deviation.
3. Find the maximum sample value.
4. A spike is detected if:

```
max - mean  >  2.0 * stddev
```

### Why standard deviation

Standard deviation scales with the metric's natural variation. A threshold of 2σ means the peak had to be unusually high relative to how much the metric was already moving — not just high in absolute terms. This works the same way whether the field is `load_1min` (values around 1–4) or `memory_used_mb` (values in the thousands).

### Edge cases

- **Fewer than 2 samples** — returns `false`. There is nothing to compare.
- **All samples identical** — stddev is 0, so the condition can never be satisfied. Returns `false`. A perfectly flat metric cannot spike.
- **Outlier inflates the mean** — this is expected behavior. One extreme outlier raises the mean, which makes the threshold harder to reach. Reliable spike detection requires several stable background samples before the burst. With a typical `store_timeout / collect_timeout` ratio of 10–20 samples per window, this is not a concern in practice.

### Worked example

Samples: `[1.0, 1.0, 1.0, 1.0, 1.0, 20.0]`

```
mean     = (1+1+1+1+1+20) / 6  = 4.167
variance = (5 * (1 - 4.167)^2 + (20 - 4.167)^2) / 6  = 50.14
stddev   = sqrt(50.14)  = 7.08

max - mean  = 20 - 4.167  = 15.83
2 * stddev  = 14.16

15.83 > 14.16  →  spike = true
```

Contrast with gradual growth `[1.0, 1.1, 1.2, 1.3]`:

```
mean   = 1.15
stddev = 0.112
max - mean = 0.15
2 * stddev = 0.224

0.15 > 0.224  →  spike = false
```

---

## Trend Detection

**Question answered:** was the metric moving in a consistent direction during this window?

### Algorithm

1. Split the ordered sample slice at the midpoint.
2. Compute the average of the first half (`first_avg`) and second half (`second_avg`).
3. Compute the delta: `delta = second_avg - first_avg`.
4. Compute a relative threshold: `5%` of the overall window mean.
5. Classify:

```
delta >  threshold  →  "rising"
delta < -threshold  →  "falling"
otherwise           →  "stable"
```

When the overall mean is near zero (below `1e-9`), an absolute threshold of `0.01` is used instead to avoid division-by-zero and nonsensical percentages.

### Why first-half vs second-half

Comparing half-window averages smooths out noise at both ends. A single high sample at the start or end (a spike, not a trend) will not dominate the result because it is averaged with its neighbors. This makes spike and trend complementary: a sudden burst tends to register as a spike but not a trend, while a sustained directional move registers as a trend but not a spike.

### Why 5%

A 5% relative change threshold prevents noise from being classified as a trend on naturally stable metrics. For example, `load_1min` hovering around 0.5 with ±0.03 variation produces a relative delta well below 5% and is correctly reported as `"stable"`.

### Edge cases

- **Fewer than 2 samples** — returns `"stable"`.
- **2 samples** — `mid = 1`, so first half is `[sample[0]]` and second half is `[sample[1]]`. Trend detection degrades to a direct first-vs-last comparison, which is acceptable for a window this narrow.
- **Zero mean** — uses the absolute fallback threshold `0.01` to avoid scaling issues.
- **Flat values** — delta is 0, well below any threshold. Returns `"stable"`.

### Worked example

Rising: `[1.0, 1.1, 1.5, 2.0, 2.5, 3.0]`

```
first half  = [1.0, 1.1, 1.5]  →  first_avg  = 1.2
second half = [2.0, 2.5, 3.0]  →  second_avg = 2.5
overall mean = 1.85

delta     = 2.5 - 1.2   = 1.3
threshold = 1.85 * 0.05 = 0.0925

1.3 > 0.0925  →  trend = "rising"
```

Stable: `[2.0, 2.1, 1.9, 2.0, 2.1, 1.9]`

```
first half  = [2.0, 2.1, 1.9]  →  first_avg  = 2.0
second half = [2.0, 2.1, 1.9]  →  second_avg = 2.0
overall mean = 2.0

delta     = 0.0
threshold = 2.0 * 0.05 = 0.1

0.0 > 0.1  →  false
0.0 < -0.1 →  false
            →  trend = "stable"
```

---

## Configuration Constants

All thresholds live in `src/analysis.rs` as named constants:

| Constant | Value | Controls |
|----------|-------|----------|
| `SPIKE_STDDEV_FACTOR` | `2.0` | How many standard deviations above the mean the peak must be to count as a spike |
| `TREND_RELATIVE_THRESHOLD` | `0.05` | Minimum relative change (5%) between window halves to classify a trend |
| `TREND_ABSOLUTE_THRESHOLD` | `0.01` | Absolute fallback threshold when the mean is near zero |

Lowering `SPIKE_STDDEV_FACTOR` makes spike detection more sensitive (fires on smaller bursts). Lowering `TREND_RELATIVE_THRESHOLD` makes trend detection more sensitive (fires on smaller directional changes).
