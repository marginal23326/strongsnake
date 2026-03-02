use std::{
    collections::VecDeque,
    sync::mpsc::Receiver,
    thread,
    time::{Duration, Instant},
};

use snake_ai::AiConfig;
use snake_domain::{Direction, GameState, LcgRng};
use tokio::sync::oneshot;

use crate::services::{ArenaProgress, ArenaSummary, RegressionSummary, TrainerSummary, build_playground_state, default_scenario_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Playground,
    Regression,
    Arena,
    Trainer,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditMode {
    PaintP1,
    PaintAi,
    Food,
    Erase,
}

pub(super) struct GuiServerHandle {
    pub(super) stop_tx: Option<oneshot::Sender<()>>,
    pub(super) join: Option<thread::JoinHandle<()>>,
}

pub(super) enum WorkerMessage {
    ArenaProgress(ArenaProgress),
    Regression(Result<RegressionSummary, String>),
    Arena(Box<Result<ArenaSummary, String>>),
    Trainer(Box<Result<TrainerSummary, String>>),
}

pub(super) struct SnakeGuiApp {
    pub(super) cfg: AiConfig,
    pub(super) playground_depth: usize,
    pub(super) tab: Tab,
    pub(super) logs: String,
    pub(super) sim_state: GameState,
    pub(super) sim_rng: LcgRng,
    pub(super) player_dir: Direction,
    pub(super) player_input_queue: VecDeque<Direction>,
    pub(super) auto_run: bool,
    pub(super) last_auto_tick: Instant,
    pub(super) last_move_ms: f64,
    pub(super) edit_mode: EditMode,
    pub(super) is_drawing: bool,
    pub(super) last_draw_cell: Option<(i32, i32)>,
    pub(super) pv_line: Vec<Direction>,
    pub(super) pv_index: usize,

    // Scenarios & IO
    pub(super) scenario_load_path: String,
    pub(super) load_error: Option<String>,
    pub(super) scenario_save_name: String,
    pub(super) scenario_expected_move: Direction,
    pub(super) save_error: Option<String>,
    pub(super) save_success: Option<String>,
    pub(super) scenario_dir: String,

    // Tools
    pub(super) depths: String,
    pub(super) regression_repeat: usize,
    pub(super) arena_games: usize,
    pub(super) arena_seed: u32,
    pub(super) arena_opponent: String,
    pub(super) arena_self_play: bool,
    pub(super) arena_find_modes: String,
    pub(super) arena_only_loss: bool,
    pub(super) arena_resume: bool,
    pub(super) arena_snapshot_file: String,
    pub(super) arena_snapshot_ticks: usize,
    pub(super) arena_progress: Option<ArenaProgress>,
    pub(super) arena_summary: Option<ArenaSummary>,
    pub(super) trainer_pop: usize,
    pub(super) trainer_gens: usize,
    pub(super) trainer_games: usize,
    pub(super) trainer_seed: u64,
    pub(super) server_addr: String,
    pub(super) server_handle: Option<GuiServerHandle>,

    // Workers
    pub(super) worker_rx: Option<Receiver<WorkerMessage>>,
    pub(super) worker_running: bool,
    pub(super) worker_label: String,
    pub(super) worker_poll_interval: Duration,
}

impl SnakeGuiApp {
    pub(super) fn new(cfg: AiConfig) -> Self {
        let rust_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let scenario_dir = default_scenario_dir(&rust_root);
        let (state, rng) = build_playground_state(16, 9, 1);

        Self {
            playground_depth: cfg.max_depth.max(1),
            cfg,
            tab: Tab::Playground,
            logs: "Rust Snake Lab ready.\n".to_owned(),
            sim_state: state,
            sim_rng: rng,
            player_dir: Direction::Up,
            player_input_queue: VecDeque::new(),
            auto_run: false,
            last_auto_tick: Instant::now(),
            last_move_ms: 0.0,
            edit_mode: EditMode::PaintP1,
            is_drawing: false,
            last_draw_cell: None,
            pv_line: Vec::new(),
            pv_index: 0,
            scenario_load_path: String::new(),
            load_error: None,
            scenario_save_name: "custom_scenario_1".to_owned(),
            scenario_expected_move: Direction::Up,
            save_error: None,
            save_success: None,
            scenario_dir: scenario_dir.display().to_string(),
            depths: "6".to_owned(),
            regression_repeat: 1,
            arena_games: 10,
            arena_seed: 1,
            arena_opponent: "local".to_owned(),
            arena_self_play: false,
            arena_find_modes: String::new(),
            arena_only_loss: false,
            arena_resume: false,
            arena_snapshot_file: "data/arena_snapshot.json".to_owned(),
            arena_snapshot_ticks: 10,
            arena_progress: None,
            arena_summary: None,
            trainer_pop: 20,
            trainer_gens: 10,
            trainer_games: 4,
            trainer_seed: 42,
            server_addr: "0.0.0.0:9000".to_owned(),
            server_handle: None,
            worker_rx: None,
            worker_running: false,
            worker_label: String::new(),
            worker_poll_interval: Duration::from_millis(50),
        }
    }

    pub(super) fn log_line(&mut self, line: impl AsRef<str>) {
        self.logs.push_str(line.as_ref());
        self.logs.push('\n');
    }
}
