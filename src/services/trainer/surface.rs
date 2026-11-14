//! Response-surface step: fits a quadratic model of "score against the reference" to every game the trainer has played
//! and moves the mean to the model's best point within a trust radius.
//!
//! Every candidate plays the same reference, so scores from different generations are comparable and all of them feed
//! one fit. That uses each game many times where "keep the best few, average them" uses it once, and the fitted
//! curvature says where to stop. No candidate is dropped early, because dropping the unlucky ones biases a fit.
//!
//!   score(u) = b0 + b.u + sum q_ii u_i^2 + sum_{i<j} q_ij u_i u_j,    u = (offset - mean) / scale
//!
//! Observations are weighted by their precision (pairs / pair variance) and by a Gaussian kernel around the mean, so the
//! model stays local; a ridge prior keeps the 21 coefficients from chasing noise.

use serde::{Deserialize, Serialize};

use super::genes::GENE_COUNT;

const D: usize = GENE_COUNT;
const TERMS: usize = 1 + 2 * D + D * (D - 1) / 2;
/// Prior standard deviation of every coefficient but the intercept, in score per unit of `u` (and per unit of `u` squared).
const PRIOR_SD: f64 = 0.05;
/// Kernel width in log-weight units: an observation this far from the mean counts with weight e^(-1/2).
const LOCALITY: f64 = 1.0;
/// Floor of the per-pair score variance, so an early all-equal sample cannot claim certainty.
const MIN_PAIR_VARIANCE: f64 = 0.01;

/// A candidate's result: where it was (offsets from the starting weights) and how it scored against the reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Observation {
    pub offsets: Vec<f64>,
    pub pairs: usize,
    pub mean: f64,
    /// Sample variance of its pair scores.
    pub variance: f64,
}

#[derive(Debug, Clone)]
pub(super) struct Step {
    /// Move of the mean in log-weight units, never longer than the trust radius.
    pub delta: Vec<f64>,
    /// Gain in mean pair score the model expects from the move (optimistic: it is fitted to noisy data).
    pub predicted_gain: f64,
    /// Length of the fitted gradient at the mean, in score per unit of `u`; small next to the noise means no direction.
    pub gradient_norm: f64,
}

fn features(u: &[f64]) -> [f64; TERMS] {
    let mut f = [0.0; TERMS];
    f[0] = 1.0;
    let mut k = 1;
    for &x in u {
        f[k] = x;
        k += 1;
    }
    for &x in u {
        f[k] = x * x;
        k += 1;
    }
    for i in 0..D {
        for j in i + 1..D {
            f[k] = u[i] * u[j];
            k += 1;
        }
    }
    f
}

/// Lower Cholesky factor of a symmetric `n` x `n` matrix (row-major), or `None` if it is not positive definite.
fn cholesky(a: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut sum = a[i * n + j];
            for k in 0..j {
                sum -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if sum <= 1e-12 {
                    return None;
                }
                l[i * n + i] = sum.sqrt();
            } else {
                l[i * n + j] = sum / l[j * n + j];
            }
        }
    }
    Some(l)
}

/// Solves `L L^T x = b`.
fn cholesky_solve(l: &[f64], n: usize, b: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut sum = b[i];
        for k in 0..i {
            sum -= l[i * n + k] * y[k];
        }
        y[i] = sum / l[i * n + i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = y[i];
        for k in i + 1..n {
            sum -= l[k * n + i] * x[k];
        }
        x[i] = sum / l[i * n + i];
    }
    x
}

/// Pooled variance of one pair's score.
fn pair_variance(observations: &[Observation]) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    for o in observations.iter().filter(|o| o.pairs > 1) {
        num += (o.pairs - 1) as f64 * o.variance;
        den += (o.pairs - 1) as f64;
    }
    if den > 0.0 { (num / den).max(MIN_PAIR_VARIANCE) } else { MIN_PAIR_VARIANCE }
}

/// The step from `center` to the best point of the fitted surface within `radius` (log-weight units); `scale` is the
/// length of one unit of `u`.
pub(super) fn plan_step(observations: &[Observation], center: &[f64], scale: f64, radius: f64) -> Option<Step> {
    let noise = pair_variance(observations);
    let mut a = vec![0.0; TERMS * TERMS];
    let mut rhs = vec![0.0; TERMS];
    for o in observations {
        let dist2: f64 = o.offsets.iter().zip(center).map(|(x, c)| (x - c).powi(2)).sum();
        let weight = (-0.5 * dist2 / (LOCALITY * LOCALITY)).exp() * o.pairs as f64 / noise;
        let u: Vec<f64> = o.offsets.iter().zip(center).map(|(x, c)| (x - c) / scale).collect();
        let phi = features(&u);
        for i in 0..TERMS {
            rhs[i] += weight * phi[i] * o.mean;
            for j in 0..=i {
                a[i * TERMS + j] += weight * phi[i] * phi[j];
            }
        }
    }
    for i in 0..TERMS {
        for j in 0..i {
            a[j * TERMS + i] = a[i * TERMS + j];
        }
        if i > 0 {
            a[i * TERMS + i] += 1.0 / (PRIOR_SD * PRIOR_SD);
        }
    }
    let beta = cholesky_solve(&cholesky(&a, TERMS)?, TERMS, &rhs);

    let gradient = &beta[1..1 + D];
    let mut hessian = vec![0.0; D * D];
    for i in 0..D {
        hessian[i * D + i] = 2.0 * beta[1 + D + i];
    }
    let mut k = 1 + 2 * D;
    for i in 0..D {
        for j in i + 1..D {
            hessian[i * D + j] = beta[k];
            hessian[j * D + i] = beta[k];
            k += 1;
        }
    }

    // Trust-region Newton step: solve (mu I - H) d = g, raising mu until the matrix is positive definite (a concave
    // model) and the step fits in the radius. Large mu is a plain gradient step.
    let radius_u = radius / scale;
    let mut mu = 0.0_f64;
    let mut best = None;
    for _ in 0..100 {
        let m: Vec<f64> = (0..D * D).map(|idx| (if idx / D == idx % D { mu } else { 0.0 }) - hessian[idx]).collect();
        if let Some(l) = cholesky(&m, D) {
            let d = cholesky_solve(&l, D, gradient);
            if d.iter().map(|x| x * x).sum::<f64>().sqrt() <= radius_u {
                best = Some(d);
                break;
            }
        }
        mu = (mu * 2.0).max(1e-3);
    }
    let d = best?;
    let linear: f64 = gradient.iter().zip(&d).map(|(g, x)| g * x).sum();
    let quadratic: f64 = (0..D).map(|i| (0..D).map(|j| d[i] * hessian[i * D + j] * d[j]).sum::<f64>()).sum();
    Some(Step {
        delta: d.iter().map(|x| x * scale).collect(),
        predicted_gain: linear + 0.5 * quadratic,
        gradient_norm: gradient.iter().map(|g| g * g).sum::<f64>().sqrt(),
    })
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::services::trainer::genes;

    /// Noisy quadratic with its best point at `optimum`: the shape of the real problem, with a known answer. Returns the
    /// trainer's answer (the average of the later means).
    fn run(optimum: [f64; D], seed: u64) -> Vec<f64> {
        let score = |x: &[f64]| surface_score(x, &optimum);
        let mut rng = StdRng::seed_from_u64(seed);
        let (pairs, scale) = (24, 0.3);
        let mut sigma = scale;
        let mut center = vec![0.0; D];
        let mut history = Vec::new();
        let mut observations = Vec::new();
        for _ in 0..30 {
            let mut points = vec![center.clone()];
            for _ in 0..8 {
                let (plus, minus) = genes::sample_antithetic(&center, sigma, &mut rng);
                points.push(plus);
                points.push(minus);
            }
            for p in points {
                // One pair scores 0.5 +- 0.33 around the surface; a candidate's mean has variance 0.33^2 / pairs.
                let noise = genes::gauss(&mut rng) * 0.33 / (pairs as f64).sqrt();
                observations.push(Observation {
                    mean: score(&p) + noise,
                    offsets: p,
                    pairs,
                    variance: 0.33 * 0.33,
                });
            }
            let step = plan_step(&observations, &center, scale, sigma).expect("a step");
            for (c, d) in center.iter_mut().zip(&step.delta) {
                *c += d;
            }
            history.push(center.clone());
            sigma *= 0.95;
        }
        let tail = &history[history.len() / 2..];
        (0..D).map(|i| tail.iter().map(|c| c[i]).sum::<f64>() / tail.len() as f64).collect()
    }

    fn surface_score(x: &[f64], optimum: &[f64; D]) -> f64 {
        0.55 - 0.08 * x.iter().zip(optimum).map(|(a, b)| (a - b).powi(2)).sum::<f64>()
    }

    #[test]
    fn recovers_most_of_the_gain_on_a_noisy_quadratic() {
        let optimum = [0.4, -0.3, 0.0, 0.2, -0.5];
        let (start, best) = (surface_score(&[0.0; D], &optimum), surface_score(&optimum, &optimum));
        let recovered: Vec<f64> = (1..=8).map(|seed| (surface_score(&run(optimum, seed), &optimum) - start) / (best - start)).collect();
        let mean = recovered.iter().sum::<f64>() / recovered.len() as f64;
        assert!(mean > 0.75, "recovered only {mean:.2} of the available gain: {recovered:?}");
        assert!(recovered.iter().all(|&r| r > 0.3), "a run recovered too little: {recovered:?}");
    }

    #[test]
    fn stays_put_on_a_flat_surface() {
        let mut rng = StdRng::seed_from_u64(3);
        let center = vec![0.0; D];
        let observations: Vec<Observation> = (0..60)
            .map(|_| Observation {
                offsets: (0..D).map(|_| genes::gauss(&mut rng) * 0.3).collect(),
                pairs: 24,
                mean: 0.5 + genes::gauss(&mut rng) * 0.33 / 24f64.sqrt(),
                variance: 0.33 * 0.33,
            })
            .collect();
        let step = plan_step(&observations, &center, 0.3, 0.3).expect("a step");
        let len: f64 = step.delta.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(len < 0.15, "moved {len:.3} on pure noise");
    }

    #[test]
    fn cholesky_round_trip() {
        let a = [4.0, 2.0, 2.0, 3.0];
        let l = cholesky(&a, 2).unwrap();
        let x = cholesky_solve(&l, 2, &[2.0, 1.0]);
        assert!((x[0] - 0.5).abs() < 1e-12 && x[1].abs() < 1e-12);
        assert!(cholesky(&[1.0, 2.0, 2.0, 1.0], 2).is_none());
    }
}
