use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use snake_ai::AiConfig;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainerOptions {
    /// Candidates per generation: the current mean plus mirrored pairs around it (so always odd).
    pub candidates: usize,
    pub gens: usize,
    /// Pairs each candidate plays per generation.
    pub pairs: usize,
    pub depth: usize,
    pub boards: Vec<(i32, i32)>,
    pub max_turns: u32,
    /// First step size: candidate weights differ from the mean by `exp(sigma * N(0, 1))`; it is also the trust radius of
    /// the mean's move.
    pub sigma: f64,
    /// Step size of the last generation; the size shrinks geometrically from `sigma` to this.
    pub sigma_end: f64,
    pub seed: u64,
    /// Worker processes (0 = one per core).
    pub workers: usize,
    /// Fresh pairs the final answer is judged on.
    pub validation_pairs: usize,
    /// The fixed opponent; `None` means the starting weights. Depth and threads are overridden like the candidates'.
    #[serde(skip)]
    pub reference: Option<AiConfig>,
    pub save: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub checkpoint: Option<PathBuf>,
}

impl TrainerOptions {
    pub const DEFAULT_CANDIDATES: usize = 17;
    pub const DEFAULT_GENS: usize = 40;
    pub const DEFAULT_PAIRS: usize = 48;
    pub const DEFAULT_DEPTH: usize = 5;
    pub const DEFAULT_BOARDS_RAW: &'static str = "11x11,16x9";
    pub const DEFAULT_MAX_TURNS: u32 = 1500;
    pub const DEFAULT_SIGMA: f64 = 0.3;
    pub const DEFAULT_SIGMA_END: f64 = 0.08;
    pub const DEFAULT_SEED: u64 = 42;
    pub const DEFAULT_WORKERS: usize = 0;
    pub const DEFAULT_VALIDATION_PAIRS: usize = 1000;

    /// Clamps the options to runnable values.
    pub(super) fn normalize(&mut self) {
        self.candidates = self.candidates.max(3) | 1;
        self.sigma = self.sigma.max(1e-3);
        self.sigma_end = self.sigma_end.clamp(1e-3, self.sigma);
        self.gens = self.gens.max(1);
        self.pairs = self.pairs.max(1);
        self.depth = self.depth.max(1);
        if self.boards.is_empty() {
            self.boards = parse_boards(Self::DEFAULT_BOARDS_RAW).expect("default boards parse");
        }
        if self.workers == 0 {
            self.workers = std::thread::available_parallelism().map_or(1, |n| n.get());
        }
        self.validation_pairs = self.validation_pairs.max(1);
    }

    /// What a checkpoint must match to be resumed.
    pub(super) fn fingerprint(&self) -> String {
        format!(
            "method=surface candidates={} pairs={} depth={} boards={:?} max_turns={} sigma={} end={} seed={}",
            self.candidates, self.pairs, self.depth, self.boards, self.max_turns, self.sigma, self.sigma_end, self.seed
        )
    }
}

impl Default for TrainerOptions {
    fn default() -> Self {
        Self {
            candidates: Self::DEFAULT_CANDIDATES,
            gens: Self::DEFAULT_GENS,
            pairs: Self::DEFAULT_PAIRS,
            depth: Self::DEFAULT_DEPTH,
            boards: parse_boards(Self::DEFAULT_BOARDS_RAW).expect("default boards parse"),
            max_turns: Self::DEFAULT_MAX_TURNS,
            sigma: Self::DEFAULT_SIGMA,
            sigma_end: Self::DEFAULT_SIGMA_END,
            seed: Self::DEFAULT_SEED,
            workers: Self::DEFAULT_WORKERS,
            validation_pairs: Self::DEFAULT_VALIDATION_PAIRS,
            reference: None,
            save: None,
            resume: None,
            checkpoint: None,
        }
    }
}

/// `11x11,16x9` -> `[(11, 11), (16, 9)]`.
pub fn parse_boards(raw: &str) -> Result<Vec<(i32, i32)>> {
    let mut boards = Vec::new();
    for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let Some((w, h)) = part.split_once('x') else {
            bail!("board `{part}` must look like 11x11");
        };
        let (w, h) = (w.trim().parse::<i32>()?, h.trim().parse::<i32>()?);
        if w < 4 || h < 4 {
            bail!("board {w}x{h} is too small");
        }
        boards.push((w, h));
    }
    if boards.is_empty() {
        bail!("no boards given");
    }
    Ok(boards)
}

/// How the final answer did on fresh validation pairs.
#[derive(Debug, Clone, Serialize)]
pub struct TrainerSummary {
    /// The Elo interval lies above zero.
    pub proven: bool,
    /// Mean pair score (0.5 = even).
    pub score: f64,
    pub elo: f64,
    pub elo_low: f64,
    pub elo_high: f64,
    pub validation_pairs: usize,
    /// The tuned weights by name.
    pub genes: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TrainerCheckpoint {
    pub fingerprint: String,
    /// Generations completed.
    pub generation: usize,
    pub mean: Vec<f64>,
    /// The mean after every completed generation.
    pub history: Vec<Vec<f64>>,
    /// Every candidate result so far: the fit uses all of them.
    pub observations: Vec<super::surface::Observation>,
}
