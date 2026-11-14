mod arena;
mod common;
mod regression;
mod trainer;
mod util;

// ArenaProgress, ArenaSummary, RegressionSummary and build_playground_state are only used by the GUI.
#[cfg_attr(not(feature = "gui"), allow(unused_imports))]
pub use arena::{
    ArenaOptions, ArenaProgress, ArenaSummary, format_arena_progress_line, format_arena_summary_report, parse_arena_find_modes,
    run_arena_with_progress,
};
#[cfg_attr(not(feature = "gui"), allow(unused_imports))]
pub use common::{build_playground_state, format_opponent_roster};
#[cfg_attr(not(feature = "gui"), allow(unused_imports))]
pub use regression::{RegressionOptions, RegressionOutput, RegressionSummary, run_regression_suite};
pub use trainer::{TrainerOptions, parse_boards, run_trainer, run_worker as run_trainer_worker};
pub use util::{default_scenario_dir, parse_depths};
