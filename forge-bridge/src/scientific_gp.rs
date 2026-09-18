//! Forge-owned categorical acquisition over the pinned SciRust numerical GP.
//! Input capability is identical to TPE: coordinates and eligible measurements.

use crate::scientific_tpe::Observation;
use scirust_gp::{GaussianProcess, Kernel};

/// Positive-definite sum of an additive categorical kernel and a product kernel.
/// Only equality matters: integer category identifiers are not numeric geometry.
#[derive(Clone, Copy)]
struct CategoricalKernel;

impl Kernel for CategoricalKernel {
    fn eval(&self, a: &[f64], b: &[f64]) -> f64 {
        let matches = a.iter().zip(b).filter(|(x, y)| x == y).count();
        let mismatches = a.len() - matches;
        0.5 * matches as f64 / a.len() as f64 + 0.5 * (-2.0 * mismatches as f64).exp()
    }
}

/// Ten successful startup observations, LCB mean - 2*stddev, one global
/// exploration every five proposals. Fit once per acquisition, reuse Cholesky
/// for all available points. No opaque fitted state survives checkpoint replay.
pub(crate) fn select(
    available: &[Vec<usize>],
    observations: &[Observation],
    ordinal: usize,
) -> Result<usize, String> {
    if observations.len() < 10 || ordinal % 5 == 0 {
        return Ok(0);
    }
    // Scale before centering to avoid overflow even for finite +/- f64::MAX.
    let scale = observations
        .iter()
        .map(|r| r.loss.abs())
        .fold(0.0_f64, f64::max);
    if scale == 0.0 {
        return Ok(0);
    }
    let ys: Vec<_> = observations.iter().map(|r| r.loss / scale).collect();
    let center = ys.iter().sum::<f64>() / ys.len() as f64;
    let spread = ys.iter().map(|y| (y - center).powi(2)).sum::<f64>() / ys.len() as f64;
    if spread == 0.0 {
        return Ok(0);
    }
    let targets: Vec<_> = ys.iter().map(|y| (y - center) / spread.sqrt()).collect();
    if targets.iter().any(|y| !y.is_finite()) {
        return Err("nonfinite GP normalization".into());
    }
    let encode = |p: &[usize]| -> Vec<f64> { p.iter().map(|&v| v as f64).collect() };
    let xs: Vec<_> = observations.iter().map(|r| encode(&r.point)).collect();
    let gp = GaussianProcess::fit(&xs, &targets, CategoricalKernel, 1e-6)
        .map_err(|e| format!("SciRust GP fit failed: {e}"))?;
    if !gp.log_marginal_likelihood().is_finite() {
        return Err("nonfinite GP fit".into());
    }
    let mut best = 0;
    let mut best_lcb = f64::INFINITY;
    for (i, point) in available.iter().enumerate() {
        let (mean, variance) = gp.predict(&encode(point));
        let lcb = mean - 2.0 * variance.sqrt();
        if !mean.is_finite() || !variance.is_finite() || variance < 0.0 || !lcb.is_finite() {
            return Err("nonfinite GP prediction".into());
        }
        if lcb < best_lcb {
            best = i;
            best_lcb = lcb;
        }
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorical_labels_have_no_distance_and_posterior_matches_scalar_oracle() {
        assert_eq!(
            CategoricalKernel.eval(&[0.0, 1.0], &[1.0, 1.0]),
            CategoricalKernel.eval(&[0.0, 1.0], &[15.0, 1.0])
        );
        let gp = GaussianProcess::fit(&[vec![0.0, 0.0]], &[2.0], CategoricalKernel, 0.1).unwrap();
        let k = CategoricalKernel.eval(&[0.0, 0.0], &[1.0, 0.0]);
        let (mean, variance) = gp.predict(&[1.0, 0.0]);
        assert!((mean - 2.0 * k / 1.1).abs() < 1e-12);
        assert!((variance - (1.0 - k * k / 1.1)).abs() < 1e-12);
    }

    #[test]
    fn extreme_finite_feedback_and_flat_objectives_are_safe() {
        let points = vec![vec![0], vec![1]];
        let mut rows: Vec<_> = (0..10)
            .map(|i| Observation {
                point: vec![i],
                loss: if i % 2 == 0 { f64::MAX } else { -f64::MAX },
            })
            .collect();
        assert!(select(&points, &rows, 11).is_ok());
        for r in &mut rows {
            r.loss = f64::MAX;
        }
        assert_eq!(select(&points, &rows, 11), Ok(0));
    }
}
