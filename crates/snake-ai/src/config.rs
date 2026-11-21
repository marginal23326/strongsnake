use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Search worker threads. `0` means auto (uses available CPU parallelism).
    pub threads: usize,
    /// Transposition table size in MiB. `0` means auto (depth-based sizing).
    pub hash_mb: usize,
}

impl RuntimeConfig {
    pub const DEFAULT_THREADS: usize = 0;
    pub const DEFAULT_HASH_MB: usize = 0;
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            threads: Self::DEFAULT_THREADS,
            hash_mb: Self::DEFAULT_HASH_MB,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodCurveConfig {
    pub intensity: f64,
    pub threshold: f64,
    pub exponent: f64,
}

/// Evaluation scores. Outcomes must stay ordered loss < draw < 0 < win, and far above the weights. The trainer tunes the weights; `territory_control` is the unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreConfig {
    // Outcomes.
    pub win: i32,
    pub loss: i32,
    pub draw: i32,

    // Weights.
    pub length: i32,
    pub eat_reward: i32,
    /// An eat counts this many percent of `eat_reward` more for every ply the search still has to go below it, so eating now
    /// beats eating later (without it, food nobody contests is put off for ever).
    pub eat_urgency: i32,
    pub territory_control: i32,
    pub food: FoodCurveConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub max_depth: usize,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default)]
    pub runtime: RuntimeConfig,
    pub scores: ScoreConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            max_depth: 8,
            debug_logging: false,
            runtime: RuntimeConfig::default(),
            scores: ScoreConfig {
                win: 1_000_000_000,
                loss: -1_000_000_000,
                draw: -100_000_000,
                length: 826,
                eat_reward: 14_193,
                eat_urgency: 32,
                territory_control: 3_265,
                food: FoodCurveConfig {
                    intensity: 5_370.1582,
                    threshold: 46.612,
                    exponent: 0.4783,
                },
            },
        }
    }
}
