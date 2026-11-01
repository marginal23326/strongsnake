use rand::{RngExt, rngs::StdRng};
use snake_ai::AiConfig;

#[derive(Debug, Clone, Copy)]
pub(super) struct GeneSpec {
    pub key: GeneKey,
    pub min: f64,
    pub max: f64,
    pub is_int: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum GeneKey {
    TightSpot,
    Length,
    EatReward,
    KillPressure,
    FoodIntensity,
    FoodThreshold,
    FoodExponent,
    Aggression,
}

impl GeneKey {
    fn read(self, cfg: &AiConfig) -> f64 {
        match self {
            Self::TightSpot => cfg.scores.tight_spot as f64,
            Self::Length => cfg.scores.length as f64,
            Self::EatReward => cfg.scores.eat_reward as f64,
            Self::KillPressure => cfg.scores.kill_pressure as f64,
            Self::FoodIntensity => cfg.scores.food.intensity,
            Self::FoodThreshold => cfg.scores.food.threshold,
            Self::FoodExponent => cfg.scores.food.exponent,
            Self::Aggression => cfg.scores.aggression as f64,
        }
    }

    fn write(self, cfg: &mut AiConfig, value: f64) {
        match self {
            Self::TightSpot => cfg.scores.tight_spot = value.round() as i32,
            Self::Length => cfg.scores.length = value.round() as i32,
            Self::EatReward => cfg.scores.eat_reward = value.round() as i32,
            Self::KillPressure => cfg.scores.kill_pressure = value.round() as i32,
            Self::FoodIntensity => cfg.scores.food.intensity = value,
            Self::FoodThreshold => cfg.scores.food.threshold = value,
            Self::FoodExponent => cfg.scores.food.exponent = value,
            Self::Aggression => cfg.scores.aggression = value.round() as i32,
        }
    }
}

const GENE_SPECS: [GeneSpec; 8] = [
    GeneSpec {
        key: GeneKey::TightSpot,
        min: -100_000.0,
        max: -10_000.0,
        is_int: true,
    },
    GeneSpec {
        key: GeneKey::Length,
        min: 0.0,
        max: 10_000.0,
        is_int: true,
    },
    GeneSpec {
        key: GeneKey::EatReward,
        min: 100.0,
        max: 10_000.0,
        is_int: true,
    },
    GeneSpec {
        key: GeneKey::KillPressure,
        min: 50_000.0,
        max: 500_000.0,
        is_int: true,
    },
    GeneSpec {
        key: GeneKey::FoodIntensity,
        min: 100.0,
        max: 4_000.0,
        is_int: false,
    },
    GeneSpec {
        key: GeneKey::FoodThreshold,
        min: 3.0,
        max: 20.0,
        is_int: false,
    },
    GeneSpec {
        key: GeneKey::FoodExponent,
        min: 1.0,
        max: 3.0,
        is_int: false,
    },
    GeneSpec {
        key: GeneKey::Aggression,
        min: 100.0,
        max: 10_000.0,
        is_int: true,
    },
];

pub(super) fn read_genes(cfg: &AiConfig) -> Vec<f64> {
    GENE_SPECS.iter().map(|spec| spec.key.read(cfg)).collect()
}

pub(super) fn apply_genes(mut cfg: AiConfig, genes: &[f64]) -> AiConfig {
    for (spec, value) in GENE_SPECS.iter().zip(genes.iter().copied()) {
        spec.key.write(&mut cfg, value);
    }
    cfg
}

pub(super) fn random_chromosome(rng: &mut StdRng) -> Vec<f64> {
    GENE_SPECS
        .iter()
        .map(|spec| {
            let mut v = rng.random_range(spec.min..=spec.max);
            if spec.is_int {
                v = v.round();
            }
            v
        })
        .collect()
}

pub(super) fn crossover(a: &[f64], b: &[f64], rng: &mut StdRng) -> Vec<f64> {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| if rng.random::<bool>() { *x } else { *y })
        .collect()
}

pub(super) fn mutate(chromosome: &mut [f64], rng: &mut StdRng, rate: f64, strength: f64) {
    for (gene, spec) in chromosome.iter_mut().zip(GENE_SPECS.iter()) {
        if rng.random::<f64>() > rate {
            continue;
        }
        let span = spec.max - spec.min;
        let delta = (rng.random::<f64>() * 2.0 - 1.0) * span * strength;
        *gene = (*gene + delta).clamp(spec.min, spec.max);
    }
}
