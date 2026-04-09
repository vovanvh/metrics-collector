// Spike and trend detection for time-series metric samples.
//
// Both functions operate on ordered f64 slices and have no external dependencies.
// They are called from the aggregator after computing avg/min/max and their results
// are stored alongside those statistics in the output BSON document.

/// A field is considered spiked if its peak deviation from the mean exceeds
/// this many standard deviations.
const SPIKE_STDDEV_FACTOR: f64 = 2.0;

/// Minimum relative change between first-half and second-half averages
/// required to classify a trend as "rising" or "falling" (5%).
const TREND_RELATIVE_THRESHOLD: f64 = 0.05;

/// Absolute fallback threshold used when the overall mean is near zero.
const TREND_ABSOLUTE_THRESHOLD: f64 = 0.01;

/// Returns true if the sample window contains a spike.
///
/// A spike is detected when the maximum value deviates from the mean by more
/// than SPIKE_STDDEV_FACTOR standard deviations. Returns false for fewer than
/// 2 samples or when all samples are identical (stddev = 0).
pub fn detect_spike(samples: &[f64]) -> bool {
    if samples.len() < 2 {
        return false;
    }
    let mean = avg(samples);
    let variance = samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    let stddev = variance.sqrt();
    if stddev == 0.0 {
        return false;
    }
    let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    (max - mean) > SPIKE_STDDEV_FACTOR * stddev
}

/// Returns the trend direction across the sample window.
///
/// Compares the average of the first half of samples against the second half.
/// Returns "rising", "falling", or "stable". Returns "stable" for fewer than 2 samples.
pub fn detect_trend(samples: &[f64]) -> &'static str {
    if samples.len() < 2 {
        return "stable";
    }
    let mid = samples.len() / 2;
    let first_avg = avg(&samples[..mid]);
    let second_avg = avg(&samples[mid..]);
    let overall = avg(samples);
    let delta = second_avg - first_avg;
    let threshold = if overall.abs() > 1e-9 {
        overall.abs() * TREND_RELATIVE_THRESHOLD
    } else {
        TREND_ABSOLUTE_THRESHOLD
    };
    if delta > threshold {
        "rising"
    } else if delta < -threshold {
        "falling"
    } else {
        "stable"
    }
}

fn avg(s: &[f64]) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    s.iter().sum::<f64>() / s.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- spike detection ---

    #[test]
    fn spike_flat() {
        assert!(!detect_spike(&[1.0, 1.0, 1.0, 1.0]));
    }

    #[test]
    fn spike_gradual() {
        assert!(!detect_spike(&[1.0, 1.1, 1.2, 1.3]));
    }

    #[test]
    fn spike_burst() {
        // 5 stable samples then one sharp outlier — outlier is clearly > 2 stddevs above mean
        assert!(detect_spike(&[1.0, 1.0, 1.0, 1.0, 1.0, 20.0]));
    }

    #[test]
    fn spike_single_sample() {
        assert!(!detect_spike(&[5.0]));
    }

    #[test]
    fn spike_zero_values() {
        assert!(!detect_spike(&[0.0, 0.0, 0.0]));
    }

    // --- trend detection ---

    #[test]
    fn trend_stable() {
        assert_eq!(detect_trend(&[2.0, 2.1, 1.9, 2.0, 2.1, 1.9]), "stable");
    }

    #[test]
    fn trend_rising() {
        assert_eq!(detect_trend(&[1.0, 1.1, 1.5, 2.0, 2.5, 3.0]), "rising");
    }

    #[test]
    fn trend_falling() {
        assert_eq!(detect_trend(&[3.0, 2.5, 2.0, 1.5, 1.0, 0.8]), "falling");
    }

    #[test]
    fn trend_single_sample() {
        assert_eq!(detect_trend(&[5.0]), "stable");
    }

    #[test]
    fn trend_zero_values() {
        assert_eq!(detect_trend(&[0.0, 0.0, 0.0]), "stable");
    }
}
