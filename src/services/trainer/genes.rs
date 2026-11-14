use rand::{RngExt, rngs::StdRng};
use snake_ai::AiConfig;

/// The tuned weights. `territory_control` is the unit the others are measured in, so it stays fixed; the verdict sizes
/// only matter in rare endgames.
#[derive(Debug, Clone, Copy)]
enum Gene {
    Length,
    EatReward,
    FoodIntensity,
    FoodThreshold,
    FoodExponent,
}

const GENES: [Gene; 5] = [Gene::Length, Gene::EatReward, Gene::FoodIntensity, Gene::FoodThreshold, Gene::FoodExponent];

pub(super) const GENE_COUNT: usize = GENES.len();

/// A candidate is the start config with each weight scaled by `exp(offset)`, so one step size is the same relative change
/// for every weight.
pub(super) type Offsets = Vec<f64>;

/// No weight moves further than a factor of 8 from where it started.
pub(super) const MAX_OFFSET: f64 = 2.0794415416798357; // ln 8

impl Gene {
    fn name(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::EatReward => "eat_reward",
            Self::FoodIntensity => "food.intensity",
            Self::FoodThreshold => "food.threshold",
            Self::FoodExponent => "food.exponent",
        }
    }

    fn read(self, cfg: &AiConfig) -> f64 {
        match self {
            Self::Length => cfg.scores.length as f64,
            Self::EatReward => cfg.scores.eat_reward as f64,
            Self::FoodIntensity => cfg.scores.food.intensity,
            Self::FoodThreshold => cfg.scores.food.threshold,
            Self::FoodExponent => cfg.scores.food.exponent,
        }
    }

    fn write(self, cfg: &mut AiConfig, value: f64) {
        match self {
            Self::Length => cfg.scores.length = value.round() as i32,
            Self::EatReward => cfg.scores.eat_reward = value.round() as i32,
            Self::FoodIntensity => cfg.scores.food.intensity = value,
            Self::FoodThreshold => cfg.scores.food.threshold = value,
            Self::FoodExponent => cfg.scores.food.exponent = value,
        }
    }
}

/// `start` with its weights scaled by `exp(offsets)`; weights at or below zero stay put.
pub(super) fn apply(start: &AiConfig, offsets: &[f64]) -> AiConfig {
    let mut cfg = start.clone();
    for (gene, offset) in GENES.iter().zip(offsets) {
        let base = gene.read(start);
        if base > 0.0 {
            gene.write(&mut cfg, base * offset.clamp(-MAX_OFFSET, MAX_OFFSET).exp());
        }
    }
    cfg
}

/// The tuned weights by name.
pub(super) fn values(cfg: &AiConfig) -> Vec<(String, f64)> {
    GENES.iter().map(|g| (g.name().to_owned(), g.read(cfg))).collect()
}

/// One standard normal sample (Box-Muller).
pub(super) fn gauss(rng: &mut StdRng) -> f64 {
    let u1 = 1.0 - rng.random::<f64>();
    let u2 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Two candidates around `mean`, mirror images of each other: `mean` plus and minus one random step of size `sigma`.
/// Mirrored pairs keep the linear and the quadratic effects apart in the fit.
pub(super) fn sample_antithetic(mean: &[f64], sigma: f64, rng: &mut StdRng) -> (Offsets, Offsets) {
    let step: Vec<f64> = mean.iter().map(|_| sigma * gauss(rng)).collect();
    let at = |sign: f64| mean.iter().zip(&step).map(|(m, s)| (m + sign * s).clamp(-MAX_OFFSET, MAX_OFFSET)).collect();
    (at(1.0), at(-1.0))
}

/// Component-wise average.
pub(super) fn average(points: &[&[f64]]) -> Offsets {
    let n = points.len().max(1) as f64;
    (0..GENE_COUNT).map(|i| points.iter().map(|p| p[i]).sum::<f64>() / n).collect()
}
