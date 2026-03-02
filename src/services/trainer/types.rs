use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use snake_api::ApiFlavor;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainerOptions {
    pub pop: usize,
    pub gens: usize,
    pub elite: usize,
    pub games: usize,
    pub depth: usize,
    pub width: i32,
    pub height: i32,
    pub max_turns: u32,
    pub mut_rate: f64,
    pub mut_strength: f64,
    pub tourney: usize,
    pub seed: u64,
    pub save: Option<PathBuf>,
    pub opponents: Vec<String>,
    pub only_http: bool,
    pub http_games: usize,
    pub self_play: bool,
    pub self_games: usize,
    pub self_every: usize,
    pub self_recent: usize,
    pub self_hof: usize,
    pub self_max_pool: usize,
    pub staged_eval: bool,
    pub quick_games: usize,
    pub quick_http_games: usize,
    pub quick_self_games: usize,
    pub quick_turn_ratio: f64,
    pub refine_top_frac: f64,
    pub validation_games: usize,
    pub progress: usize,
    pub progress_every: usize,
    pub verify: bool,
    pub verify_depths: Vec<usize>,
    pub verify_max_attempts: usize,
    pub http_api_mode: ApiFlavor,
    pub legacy_http: bool,
    pub resume: Option<PathBuf>,
    pub checkpoint: Option<PathBuf>,
}

impl TrainerOptions {
    pub const DEFAULT_POP: usize = 30;
    pub const DEFAULT_GENS: usize = 25;
    pub const DEFAULT_ELITE: usize = 4;
    pub const DEFAULT_GAMES: usize = 5;
    pub const DEFAULT_DEPTH: usize = 6;
    pub const DEFAULT_WIDTH: i32 = 16;
    pub const DEFAULT_HEIGHT: i32 = 9;
    pub const DEFAULT_MAX_TURNS: u32 = 500;
    pub const DEFAULT_MUT_RATE: f64 = 0.25;
    pub const DEFAULT_MUT_STRENGTH: f64 = 0.15;
    pub const DEFAULT_TOURNEY: usize = 3;
    pub const DEFAULT_SEED: u64 = 42;
    pub const DEFAULT_OPPONENT: &'static str = "local-old";
    pub const DEFAULT_ONLY_HTTP: bool = false;
    pub const DEFAULT_HTTP_GAMES_OVERRIDE: usize = 0;
    pub const DEFAULT_SELF_PLAY: bool = false;
    pub const DEFAULT_SELF_GAMES: usize = 4;
    pub const DEFAULT_SELF_EVERY: usize = 5;
    pub const DEFAULT_SELF_RECENT: usize = 5;
    pub const DEFAULT_SELF_HOF: usize = 4;
    pub const DEFAULT_SELF_MAX_POOL: usize = 8;
    pub const DEFAULT_STAGED_EVAL: bool = true;
    pub const DEFAULT_QUICK_GAMES: usize = 4;
    pub const DEFAULT_QUICK_HTTP_GAMES: usize = 2;
    pub const DEFAULT_QUICK_SELF_GAMES: usize = 2;
    pub const DEFAULT_QUICK_TURN_RATIO: f64 = 0.55;
    pub const DEFAULT_REFINE_TOP_FRAC: f64 = 0.5;
    pub const DEFAULT_VALIDATION_GAMES: usize = 10;
    pub const DEFAULT_PROGRESS: usize = 0;
    pub const DEFAULT_PROGRESS_EVERY: usize = 1;
    pub const DEFAULT_VERIFY: bool = true;
    pub const DEFAULT_VERIFY_DEPTHS_RAW: &'static str = "4,5,6";
    pub const DEFAULT_VERIFY_MAX_ATTEMPTS: usize = 800;
    pub const DEFAULT_HTTP_API_RAW: &'static str = "auto";
    pub const DEFAULT_LEGACY_HTTP: bool = false;
    pub const DEFAULT_VERIFY_DEPTHS: [usize; 3] = [4, 5, 6];
}

impl Default for TrainerOptions {
    fn default() -> Self {
        Self {
            pop: Self::DEFAULT_POP,
            gens: Self::DEFAULT_GENS,
            elite: Self::DEFAULT_ELITE,
            games: Self::DEFAULT_GAMES,
            depth: Self::DEFAULT_DEPTH,
            width: Self::DEFAULT_WIDTH,
            height: Self::DEFAULT_HEIGHT,
            max_turns: Self::DEFAULT_MAX_TURNS,
            mut_rate: Self::DEFAULT_MUT_RATE,
            mut_strength: Self::DEFAULT_MUT_STRENGTH,
            tourney: Self::DEFAULT_TOURNEY,
            seed: Self::DEFAULT_SEED,
            save: None,
            opponents: vec![Self::DEFAULT_OPPONENT.to_owned()],
            only_http: Self::DEFAULT_ONLY_HTTP,
            http_games: Self::DEFAULT_GAMES,
            self_play: Self::DEFAULT_SELF_PLAY,
            self_games: Self::DEFAULT_SELF_GAMES,
            self_every: Self::DEFAULT_SELF_EVERY,
            self_recent: Self::DEFAULT_SELF_RECENT,
            self_hof: Self::DEFAULT_SELF_HOF,
            self_max_pool: Self::DEFAULT_SELF_MAX_POOL,
            staged_eval: Self::DEFAULT_STAGED_EVAL,
            quick_games: Self::DEFAULT_QUICK_GAMES,
            quick_http_games: Self::DEFAULT_QUICK_HTTP_GAMES,
            quick_self_games: Self::DEFAULT_QUICK_SELF_GAMES,
            quick_turn_ratio: Self::DEFAULT_QUICK_TURN_RATIO,
            refine_top_frac: Self::DEFAULT_REFINE_TOP_FRAC,
            validation_games: Self::DEFAULT_VALIDATION_GAMES,
            progress: Self::DEFAULT_PROGRESS,
            progress_every: Self::DEFAULT_PROGRESS_EVERY,
            verify: Self::DEFAULT_VERIFY,
            verify_depths: Self::DEFAULT_VERIFY_DEPTHS.to_vec(),
            verify_max_attempts: Self::DEFAULT_VERIFY_MAX_ATTEMPTS,
            http_api_mode: ApiFlavor::Auto,
            legacy_http: Self::DEFAULT_LEGACY_HTTP,
            resume: None,
            checkpoint: None,
        }
    }
}

impl TrainerOptions {
    pub fn for_gui(pop: usize, gens: usize, games: usize, seed: u64, depth: usize) -> Self {
        Self {
            pop,
            gens,
            games,
            http_games: games,
            seed,
            depth,
            ..Self::default()
        }
    }

    pub(super) fn normalize(&mut self) {
        self.pop = self.pop.max(2);
        self.gens = self.gens.max(1);
        self.elite = self.elite.min(self.pop).max(1);
        self.games = self.games.max(1);
        self.http_games = self.http_games.max(1);
        self.self_games = self.self_games.max(1);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TrainerSummary {
    pub best_fitness: f64,
    pub best_generation: usize,
    pub genes: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TrainerCheckpoint {
    pub generation: usize,
    pub pop: Vec<Vec<f64>>,
    pub best_genes: Vec<f64>,
    pub best_score: f64,
    pub best_generation: usize,
}
