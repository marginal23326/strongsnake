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
                body: FastBody::from_vec(&me.body),
                health: me.health,
            },
            AgentState {
                body: FastBody::from_vec(&enemy.body),
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
