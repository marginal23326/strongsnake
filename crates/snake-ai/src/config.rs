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

/// Evaluation scores. Outcomes and verdicts must stay ordered loss < trap_danger < draw < 0 < enemy_trapped < win,
/// and far above the weights. The trainer tunes the weights; `territory_control` is the unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreConfig {
    // Outcomes.
    pub win: i32,
    pub loss: i32,
    pub draw: i32,

    // Verdicts.
    pub trap_danger: i32,
    pub enemy_trapped: i32,

    // Weights.
    pub length: i32,
    pub eat_reward: i32,
    pub territory_control: i32,
    pub food: FoodCurveConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub max_depth: usize,
    pub dense_tail_race_occupancy: usize,
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
            dense_tail_race_occupancy: 40,
            debug_logging: false,
            runtime: RuntimeConfig::default(),
            scores: ScoreConfig {
                win: 1_000_000_000,
                loss: -1_000_000_000,
                draw: -100_000_000,
                trap_danger: -413_704_270,
                enemy_trapped: 320_798_923,
                length: 1_000,
                eat_reward: 2_000,
                territory_control: 3_265,
                food: FoodCurveConfig {
                    intensity: 3_303.092,
                    threshold: 19.357,
                    exponent: 1.968,
                },
            },
        }
    }
}
