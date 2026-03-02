use std::path::PathBuf;

use serde::Serialize;
use snake_api::ApiFlavor;

use super::super::common::{DeathReasonCounts, MatchResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArenaFindMode {
    Shortest,
    Longest,
    ShortestSnake,
    LongestSnake,
}

impl ArenaFindMode {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "shortest" | "shortest-turns" | "st" => Some(Self::Shortest),
            "longest" | "longest-turns" | "lt" => Some(Self::Longest),
            "shortest-snake" | "ss" => Some(Self::ShortestSnake),
            "longest-snake" | "ls" => Some(Self::LongestSnake),
            _ => None,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Shortest => "Shortest Turns",
            Self::Longest => "Longest Turns",
            Self::ShortestSnake => "Shortest Local Snake",
            Self::LongestSnake => "Longest Local Snake",
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Shortest => "shortest",
            Self::Longest => "longest",
            Self::ShortestSnake => "shortest-snake",
            Self::LongestSnake => "longest-snake",
        }
    }

    fn metric(self, result: &MatchResult) -> i64 {
        match self {
            Self::Shortest | Self::Longest => result.turns as i64,
            Self::ShortestSnake | Self::LongestSnake => result.local_length as i64,
        }
    }

    pub(crate) fn metric_label(self, result: &MatchResult) -> String {
        match self {
            Self::Shortest | Self::Longest => format!("{} turns", result.turns),
            Self::ShortestSnake | Self::LongestSnake => {
                format!("{} local length", result.local_length)
            }
        }
    }

    pub(crate) fn better(self, candidate: &MatchResult, current: &MatchResult) -> bool {
        let cand = self.metric(candidate);
        let best = self.metric(current);
        match self {
            Self::Shortest | Self::ShortestSnake => cand < best,
            Self::Longest | Self::LongestSnake => cand > best,
        }
    }
}

pub fn parse_arena_find_modes(values: &[String]) -> (Vec<ArenaFindMode>, Vec<String>) {
    let mut parsed = Vec::new();
    let mut invalid = Vec::new();
    for token in values.iter().flat_map(|v| v.split(',')).map(str::trim).filter(|v| !v.is_empty()) {
        if let Some(mode) = ArenaFindMode::parse(token) {
            if !parsed.contains(&mode) {
                parsed.push(mode);
            }
        } else {
            invalid.push(token.to_owned());
        }
    }
    (parsed, invalid)
}

#[derive(Debug, Clone)]
pub struct ArenaOptions {
    pub games: usize,
    pub seed: u32,
    pub width: i32,
    pub height: i32,
    pub max_turns: u32,
    pub opponent: String,
    pub self_play: bool,
    pub api_flavor: ApiFlavor,
    pub request_timeout_ms: u64,
    pub payload_timeout_ms: u32,
    pub find_modes: Vec<ArenaFindMode>,
    pub invalid_find_modes: Vec<String>,
    pub only_loss: bool,
    pub resume: bool,
    pub snapshot_file: PathBuf,
    pub snapshot_ticks: usize,
}

impl ArenaOptions {
    pub const DEFAULT_GAMES: usize = 100;
    pub const DEFAULT_SEED: u32 = 1;
    pub const DEFAULT_WIDTH: i32 = 16;
    pub const DEFAULT_HEIGHT: i32 = 9;
    pub const DEFAULT_MAX_TURNS: u32 = 2000;
    pub const DEFAULT_OPPONENT: &'static str = "local-old";
    pub const DEFAULT_SELF_PLAY: bool = false;
    pub const DEFAULT_API_RAW: &'static str = "auto";
    pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 700;
    pub const DEFAULT_PAYLOAD_TIMEOUT_MS: u32 = 100;
    pub const DEFAULT_ONLY_LOSS: bool = false;
    pub const DEFAULT_RESUME: bool = false;
    pub const DEFAULT_SNAPSHOT_TICKS: usize = 10;
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaProgress {
    pub completed_games: usize,
    pub total_games: usize,
    pub wins_local: usize,
    pub wins_opponent: usize,
    pub draws: usize,
    pub local_win_rate: f64,
    pub opponent_win_rate: f64,
    pub draw_rate: f64,
    pub avg_turns: f64,
    pub avg_local_length: f64,
    pub avg_opponent_length: f64,
    pub elapsed_ms: u128,
    pub last_seed: u32,
    pub last_turns: u32,
    pub last_winner: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaDistributionBin {
    pub label: String,
    pub count: usize,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ArenaDeathStats {
    pub local: DeathReasonCounts,
    pub opponent: DeathReasonCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaTurnsExtreme {
    pub seed: u32,
    pub turns: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaLengthExtreme {
    pub seed: u32,
    pub length: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaFindResult {
    pub mode: ArenaFindMode,
    pub mode_title: String,
    pub seed: u32,
    pub winner: String,
    pub turns: u32,
    pub local_length: usize,
    pub metric_label: String,
    pub snapshot_file: Option<String>,
    pub reproduce_hint: String,
    pub resume_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArenaSummary {
    pub wins_local: usize,
    pub wins_opponent: usize,
    pub draws: usize,
    pub total_games: usize,
    pub local_win_rate: f64,
    pub opponent_win_rate: f64,
    pub draw_rate: f64,
    pub avg_turns: f64,
    pub avg_local_length: f64,
    pub avg_opponent_length: f64,
    pub duration_ms: u128,
    pub death_stats: ArenaDeathStats,
    pub turn_distribution: Vec<ArenaDistributionBin>,
    pub local_length_distribution: Vec<ArenaDistributionBin>,
    pub opponent_length_distribution: Vec<ArenaDistributionBin>,
    pub shortest_turn_game: Option<ArenaTurnsExtreme>,
    pub longest_turn_game: Option<ArenaTurnsExtreme>,
    pub shortest_local_length_game: Option<ArenaLengthExtreme>,
    pub longest_local_length_game: Option<ArenaLengthExtreme>,
    pub resumed_from_snapshot: bool,
    pub snapshot_file: Option<String>,
    pub invalid_find_modes: Vec<String>,
    pub find_results: Vec<ArenaFindResult>,
    pub results: Vec<MatchResult>,
}
