pub mod scenario;
pub mod sfen;

pub use scenario::{
    AI_ID, Expectation, NamedScenario, PLAYER_ID, Scenario, ScenarioBoard, ScenarioSnake, load_scenarios_from_dir, save_scenario_to_file,
};
pub use sfen::{SFEN_PREFIX, SnakeFen, SnakeFenError};
