#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

pub mod bitboard;
pub mod brain;
pub mod config;
pub mod floodfill;
pub mod grid;
pub mod heuristics;
pub mod model;
pub mod pathfinding;
pub mod search;
pub mod tt;
pub mod voronoi;
pub mod zobrist;

pub use brain::{Decision, decide_move_debug, warm_up_runtime};
pub use config::{AiConfig, RuntimeConfig, ScoreConfig};
pub use model::AgentState;

use std::cell::RefCell;
#[cfg(feature = "profiling")]
use std::time::Duration;

#[derive(Default, Debug, Clone, Copy)]
pub struct PerfStats {
    pub negamax_calls: u64,

    #[cfg(feature = "profiling")]
    pub eval_calls: u64,
    #[cfg(feature = "profiling")]
    pub eval_duration: Duration,

    #[cfg(feature = "profiling")]
    pub voronoi_calls: u64,
    #[cfg(feature = "profiling")]
    pub voronoi_duration: Duration,

    #[cfg(feature = "profiling")]
    pub floodfill_calls: u64,
    #[cfg(feature = "profiling")]
    pub floodfill_duration: Duration,

    #[cfg(feature = "profiling")]
    pub move_gen_calls: u64,
    #[cfg(feature = "profiling")]
    pub move_gen_duration: Duration,

    #[cfg(feature = "profiling")]
    pub distmap_calls: u64,
    #[cfg(feature = "profiling")]
    pub distmap_duration: Duration,
}

thread_local! {
    pub static PERF_STATS: RefCell<PerfStats> = RefCell::new(PerfStats::default());
}
