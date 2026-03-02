use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use snake_api::normalize_move_name;
use snake_domain::{Direction, GameState, Snake};

use super::super::common::MatchTraceFrame;
use super::types::ArenaFindMode;

#[derive(Debug, Clone)]
pub(crate) struct LoadedSnapshot {
    pub(crate) state: GameState,
    pub(crate) rng_seed: u32,
    pub(crate) opponent_moves: VecDeque<Direction>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ArenaSnapshotFile {
    #[serde(rename = "p")]
    opponent_body: Vec<snake_domain::Point>,
    #[serde(rename = "a")]
    local_body: Vec<snake_domain::Point>,
    #[serde(rename = "foods")]
    foods: Vec<snake_domain::Point>,
    #[serde(rename = "pHealth")]
    opponent_health: i32,
    #[serde(rename = "aHealth")]
    local_health: i32,
    #[serde(rename = "cols")]
    cols: i32,
    #[serde(rename = "rows")]
    rows: i32,
    #[serde(rename = "turn")]
    turn: u32,
    #[serde(rename = "seed")]
    rng_seed: u32,
    #[serde(rename = "opponentMoves", default)]
    opponent_moves: Vec<String>,
}

pub(crate) fn snapshot_path_for_mode(base: &Path, mode: ArenaFindMode, with_suffix: bool) -> PathBuf {
    if !with_suffix {
        return base.to_path_buf();
    }

    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "arena_snapshot".to_owned());
    let ext = base
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_else(|| ".json".to_owned());
    let file_name = format!("{stem}_{}{}", mode.slug(), ext);
    base.with_file_name(file_name)
}

pub(crate) fn build_snapshot(trace: &[MatchTraceFrame], snapshot_ticks: usize, width: i32, height: i32) -> Option<ArenaSnapshotFile> {
    if trace.is_empty() {
        return None;
    }
    let ticks = snapshot_ticks.max(1);
    let idx = trace.len().saturating_sub(ticks);
    let frame = &trace[idx];

    let s1 = frame.state.board.snakes.iter().find(|s| s.id.0 == "s1");
    let s2 = frame.state.board.snakes.iter().find(|s| s.id.0 == "s2");
    let local = s1.cloned().unwrap_or_else(|| Snake::new("s1", "local", Vec::new(), 0));
    let opponent = s2.cloned().unwrap_or_else(|| Snake::new("s2", "opponent", Vec::new(), 0));
    let moves = trace[idx..].iter().map(|turn| turn.opponent_move.as_lower().to_owned()).collect();

    Some(ArenaSnapshotFile {
        opponent_body: opponent.body,
        local_body: local.body,
        foods: frame.state.board.food.clone(),
        opponent_health: opponent.health,
        local_health: local.health,
        cols: width,
        rows: height,
        turn: frame.state.turn,
        rng_seed: frame.rng_seed,
        opponent_moves: moves,
    })
}

pub(crate) fn write_snapshot(path: &Path, snapshot: &ArenaSnapshotFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(snapshot)?;
    fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

pub(crate) fn load_snapshot(path: &Path, width: i32, height: i32) -> Result<LoadedSnapshot> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let snapshot: ArenaSnapshotFile =
        serde_json::from_str(&text).with_context(|| format!("invalid snapshot format in {}", path.display()))?;
    if snapshot.cols != width || snapshot.rows != height {
        bail!(
            "snapshot board {}x{} does not match arena board {}x{}",
            snapshot.cols,
            snapshot.rows,
            width,
            height
        );
    }

    let mut opponent_moves = VecDeque::new();
    for m in snapshot.opponent_moves {
        let Some(parsed) = normalize_move_name(&m) else {
            continue;
        };
        opponent_moves.push_back(parsed);
    }

    Ok(LoadedSnapshot {
        state: GameState {
            turn: snapshot.turn,
            seed: snapshot.rng_seed as u64,
            board: snake_domain::Board {
                width,
                height,
                food: snapshot.foods,
                snakes: vec![
                    Snake::new("s1", "local", snapshot.local_body, snapshot.local_health),
                    Snake::new("s2", "opponent", snapshot.opponent_body, snapshot.opponent_health),
                ],
            },
        },
        rng_seed: snapshot.rng_seed,
        opponent_moves,
    })
}

pub(crate) fn display_path(path: &Path) -> String {
    let cwd = std::env::current_dir().ok();
    if let Some(cwd) = cwd
        && let Ok(relative) = path.strip_prefix(cwd)
    {
        return relative.display().to_string();
    }
    path.display().to_string()
}

pub(crate) fn quote_for_cli(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}
