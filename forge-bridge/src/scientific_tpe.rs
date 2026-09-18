//! Finite categorical TPE policy, version `forge-finite-tpe/v1`.
//!
//! This private capability accepts only coordinates and verified scalar feedback.
//! No domain manifest, validation records, oracle, file or process is accessible.
//! Category spelling/order has no metric meaning. All constants are versioned.

pub(crate) struct Observation {
    pub point: Vec<usize>,
    pub loss: f64,
}

/// Mixture of factorized and joint categorical Parzen densities. The joint
/// component preserves observed interactions, the factorized one recombines
/// promising coordinates. One uniform pseudo-observation keeps support positive.
fn density(point: &[usize], sizes: &[usize], rows: &[&Observation]) -> f64 {
    let prior: f64 = sizes.iter().map(|&k| 1.0 / k as f64).product();
    let kernel = |value: usize, observed: usize, k: usize| {
        0.2 / k as f64 + if value == observed { 0.8 } else { 0.0 }
    };
    let denominator = (rows.len() + 1) as f64;
    let joint = (prior
        + rows
            .iter()
            .map(|r| {
                point
                    .iter()
                    .zip(&r.point)
                    .zip(sizes)
                    .map(|((&x, &y), &k)| kernel(x, y, k))
                    .product::<f64>()
            })
            .sum::<f64>())
        / denominator;
    let independent = point
        .iter()
        .zip(sizes)
        .enumerate()
        .map(|(d, (&x, &k))| {
            (1.0 / k as f64 + rows.iter().map(|r| kernel(x, r.point[d], k)).sum::<f64>())
                / denominator
        })
        .product::<f64>();
    0.5 * joint + 0.5 * independent
}

/// `available` is a seeded permutation of admissible, untried points. Keeping
/// that order for ties and exploration makes replay independent of call count.
/// At most 4,096 points, 256 observations and 8 dimensions: no unbounded fitting.
pub(crate) fn select(
    sizes: &[usize],
    available: &[Vec<usize>],
    observations: &[Observation],
    ordinal: usize,
) -> usize {
    select_with_startup(sizes, available, observations, ordinal, 10)
}

/// Opt-in early-feedback policy, versioned separately from the historical TPE.
/// It changes only the startup threshold; all density and exploration constants
/// remain fixed. Sparse evidence can mislead it, so no default promotion follows.
pub(crate) fn select_early(
    sizes: &[usize],
    available: &[Vec<usize>],
    observations: &[Observation],
    ordinal: usize,
) -> usize {
    select_with_startup(
        sizes,
        available,
        observations,
        ordinal,
        (2 * sizes.len()).clamp(4, 10),
    )
}

fn select_with_startup(
    sizes: &[usize],
    available: &[Vec<usize>],
    observations: &[Observation],
    ordinal: usize,
    startup: usize,
) -> usize {
    // Ten successful observations before fitting; every fifth proposal explores.
    // Missing/failed measurements never become bad-score pseudo-observations.
    if observations.len() < startup || ordinal.is_multiple_of(5) {
        return 0;
    }
    let mut ranked: Vec<_> = observations.iter().collect();
    ranked.sort_by(|a, b| a.loss.total_cmp(&b.loss));
    // A flat objective provides no ranking signal.
    if ranked.first().unwrap().loss == ranked.last().unwrap().loss {
        return 0;
    }
    let elite_count = observations.len().div_ceil(5);
    let (good, bad) = ranked.split_at(elite_count);
    let mut best = 0;
    let mut best_ratio = f64::NEG_INFINITY;
    for (i, point) in available.iter().enumerate() {
        let ratio = density(point, sizes, good) / density(point, sizes, bad);
        if ratio > best_ratio {
            best_ratio = ratio;
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_policy_uses_feedback_before_legacy_startup_and_preserves_exploration() {
        let points = vec![vec![0], vec![1]];
        let rows: Vec<_> = (0..4)
            .map(|i| Observation {
                point: vec![i % 2],
                loss: (1 - i % 2) as f64,
            })
            .collect();
        assert_eq!(select(&[2], &points, &rows, 4), 0);
        assert_eq!(select_early(&[2], &points, &rows, 4), 1);
        assert_eq!(select_early(&[2], &points, &rows, 5), 0);
    }

    #[test]
    fn feedback_changes_acquisition_and_flat_feedback_explores() {
        let points = vec![vec![0, 0], vec![1, 1]];
        let mut rows: Vec<_> = (0..10)
            .map(|i| Observation {
                point: vec![i % 2, i % 2],
                loss: (i % 2) as f64,
            })
            .collect();
        assert_eq!(select(&[2, 2], &points, &rows, 11), 0);
        for r in &mut rows {
            r.loss = -r.loss;
        }
        assert_eq!(select(&[2, 2], &points, &rows, 11), 1);
        assert_eq!(select(&[2, 2], &points, &rows, 15), 0);
        for r in &mut rows {
            r.loss = 0.0;
        }
        assert_eq!(select(&[2, 2], &points, &rows, 11), 0);
    }

    #[test]
    fn densities_are_normalized_positive_and_capture_interactions() {
        let rows = [
            Observation {
                point: vec![0, 0],
                loss: 0.0,
            },
            Observation {
                point: vec![1, 1],
                loss: 0.0,
            },
        ];
        let refs: Vec<_> = rows.iter().collect();
        let mut sum = 0.0;
        for x in 0..2 {
            for y in 0..2 {
                let p = density(&[x, y], &[2, 2], &refs);
                assert!(p > 0.0);
                sum += p;
            }
        }
        assert!((sum - 1.0).abs() < 1e-12);
        assert!(density(&[0, 0], &[2, 2], &refs) > density(&[0, 1], &[2, 2], &refs));
    }
}
