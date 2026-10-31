use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use snake_ai::AgentState;
use snake_ai::model::FastBody;
use snake_domain::{Direction, Point};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSnake {
    pub id: String,
    pub name: String,
    pub body: Vec<Point>,
    pub health: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioBoard {
    pub width: i32,
    pub height: i32,
    pub food: Vec<Point>,
    pub snakes: Vec<ScenarioSnake>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expectation {
    Exact { direction: Direction },
    Avoid { directions: Vec<Direction> },
}

impl Expectation {
    pub fn passes(&self, actual: Direction) -> bool {
        match self {
            Expectation::Exact { direction } => *direction == actual,
            Expectation::Avoid { directions } => !directions.contains(&actual),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub board: ScenarioBoard,
    pub you_id: String,
    pub expectation: Expectation,
}

#[derive(Debug, Clone)]
pub struct NamedScenario {
    pub file: PathBuf,
    pub scenario: Scenario,
}

/// Canonical snake ids used across the whole project:
/// the snake under test ("you", red in the GUI) and its opponent (the
/// human-controlled snake, blue in the GUI).
pub const AI_ID: &str = "ai";
pub const PLAYER_ID: &str = "player";

impl Scenario {
    /// Converts the scenario into the canonical two-snake game state used by
    /// the playground: `you_id` becomes `ai` ("AI"), the first other snake
    /// becomes `player` ("Player"), extra snakes are renamed `other_N`.
    /// Returns `None` if the `you_id` snake does not exist.
    pub fn into_game_state(&self) -> Option<snake_domain::GameState> {
        let you_index = self.board.snakes.iter().position(|s| s.id == self.you_id)?;

        let mut snakes = Vec::with_capacity(self.board.snakes.len());
        let mut other_count = 0;
        for (index, snake) in self.board.snakes.iter().enumerate() {
            let (id, name): (String, String) = if index == you_index {
                (AI_ID.to_owned(), "AI".to_owned())
            } else if other_count == 0 {
                other_count += 1;
                (PLAYER_ID.to_owned(), "Player".to_owned())
            } else {
                other_count += 1;
                let id = format!("other_{other_count}");
                let name = id.clone();
                (id, name)
            };
            snakes.push(snake_domain::Snake::new(
                id.as_str(),
                name.as_str(),
                snake.body.iter().copied().collect::<Vec<Point>>(),
                snake.health,
            ));
        }

        // Sort the opponent first so snake order matches the playground
        // render convention (player before AI).
        snakes.sort_by_key(|snake| u8::from(snake.id.0 != PLAYER_ID));

        Some(snake_domain::GameState {
            turn: 0,
            seed: 0,
            board: snake_domain::Board {
                width: self.board.width,
                height: self.board.height,
                food: self.board.food.clone(),
                snakes,
            },
        })
    }
}

impl Scenario {
    pub fn into_ai_inputs(&self) -> Option<(AgentState, AgentState, Vec<Point>, i32, i32)> {
        let me = self.board.snakes.iter().find(|s| s.id == self.you_id)?;
        let enemy = self
            .board
            .snakes
            .iter()
            .find(|s| s.id != self.you_id)
            .cloned()
            .unwrap_or(ScenarioSnake {
                id: "enemy".to_owned(),
                name: "enemy".to_owned(),
                body: Vec::new(),
                health: 0,
            });

        Some((
            AgentState {
                body: FastBody::from_points(me.body.iter().copied()),
                health: me.health,
            },
            AgentState {
                body: FastBody::from_points(enemy.body.iter().copied()),
                health: enemy.health,
            },
            self.board.food.clone(),
            self.board.width,
            self.board.height,
        ))
    }
}

pub fn load_scenarios_from_dir(path: &Path) -> Result<Vec<NamedScenario>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();

    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let raw = fs::read_to_string(&file).with_context(|| format!("failed to read scenario file {}", file.display()))?;
        let scenario: Scenario = serde_json::from_str(&raw).with_context(|| format!("failed to parse scenario {}", file.display()))?;
        out.push(NamedScenario { file, scenario });
    }
    Ok(out)
}

pub fn save_scenario_to_file(path: &Path, scenario: &Scenario) -> Result<()> {
    let raw = serde_json::to_string_pretty(scenario)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, raw)?;
    Ok(())
}
