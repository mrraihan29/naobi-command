/// Nearest-rank percentile (`p` in 1..=100). `None` for empty input or `p` out of range.
pub fn percentile(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() || !(p > 0.0 && p <= 100.0) {
        return None;
    }
    let mut v = samples.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = (p / 100.0 * v.len() as f64).ceil() as usize;
    Some(v[rank.max(1) - 1])
}

#[cfg(test)]
mod tests {
    use super::percentile;

    // @R-006
    #[test]
    fn r006_percentile_function() {
        let v: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        assert_eq!(percentile(&v, 95.0), Some(95.0));
        assert_eq!(percentile(&v, 100.0), Some(100.0));
        assert_eq!(percentile(&v, 1.0), Some(1.0));
        assert_eq!(percentile(&[3.0, 1.0, 2.0], 50.0), Some(2.0));
        assert_eq!(percentile(&[7.0], 95.0), Some(7.0));
        assert_eq!(percentile(&[], 95.0), None);
        assert_eq!(percentile(&v, 0.0), None);
        assert_eq!(percentile(&v, 101.0), None);
    }
}
