use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub stddev: f64,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "count={} min={:.3} max={:.3} mean={:.3} p50={:.3} p95={:.3} p99={:.3} stddev={:.3}",
            self.count, self.min, self.max, self.mean, self.p50, self.p95, self.p99, self.stddev
        )
    }
}

/// Compute summary stats for a slice of samples.
///
/// - Uses standard deviation of the population (divide by N).
/// - Percentiles use linear interpolation between closest ranks.
pub fn summarize(samples: &[f64]) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }

    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let count = v.len();
    let min = v[0];
    let max = v[count - 1];

    let sum: f64 = v.iter().sum();
    let mean = sum / count as f64;

    let var: f64 = v
        .iter()
        .map(|x| {
            let d = x - mean;
            d * d
        })
        .sum::<f64>()
        / count as f64;
    let stddev = var.sqrt();

    let p50 = percentile_sorted(&v, 0.50);
    let p95 = percentile_sorted(&v, 0.95);
    let p99 = percentile_sorted(&v, 0.99);

    Some(Summary {
        count,
        min,
        max,
        mean,
        p50,
        p95,
        p99,
        stddev,
    })
}

fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    debug_assert!((0.0..=1.0).contains(&p));

    if sorted.len() == 1 {
        return sorted[0];
    }

    // Linear interpolation between ranks:
    // https://en.wikipedia.org/wiki/Percentile#Linear_interpolation_between_closest_ranks
    let n = sorted.len() as f64;
    let rank = p * (n - 1.0);
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let w = rank - lo as f64;
    sorted[lo] * (1.0 - w) + sorted[hi] * w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_edges() {
        let v = [1.0, 2.0, 3.0, 4.0];
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(percentile_sorted(&s, 0.0), 1.0);
        assert_eq!(percentile_sorted(&s, 1.0), 4.0);
    }

    #[test]
    fn percentile_interpolates() {
        // With 4 points, p50 rank is 1.5 => halfway between 2 and 3.
        let s = vec![1.0, 2.0, 3.0, 4.0];
        assert!((percentile_sorted(&s, 0.5) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn summarize_basic() {
        let s = summarize(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(s.count, 4);
        assert_eq!(s.min, 1.0);
        assert_eq!(s.max, 4.0);
        assert!((s.mean - 2.5).abs() < 1e-9);
        assert!((s.p50 - 2.5).abs() < 1e-9);
        assert!((s.p95 - 3.85).abs() < 1e-9);
        assert!((s.p99 - 3.97).abs() < 1e-9);
    }
}
