use eframe::egui;
use snake_domain::{LcgRng, Point, Snake};
use snake_io::{AI_ID, PLAYER_ID, SFEN_PREFIX, SnakeFen};

use super::super::state::SnakeGuiApp;

impl SnakeGuiApp {
    fn player_and_ai(&self) -> (Option<&Snake>, Option<&Snake>) {
        let player = self.sim_state.board.snakes.iter().find(|s| s.id.0 == PLAYER_ID);
        let ai = self.sim_state.board.snakes.iter().find(|s| s.id.0 == AI_ID);
        (player, ai)
    }

    fn compact_json_state(&self) -> serde_json::Value {
        let (player, ai) = self.player_and_ai();
        let p_body = player.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
        let a_body = ai.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
        serde_json::json!({
            "p": p_body,
            "a": a_body,
            "foods": self.sim_state.board.food.clone(),
            "pHealth": player.map(|s| s.health).unwrap_or(100),
            "aHealth": ai.map(|s| s.health).unwrap_or(100),
            "cols": self.sim_state.board.width,
            "rows": self.sim_state.board.height,
            "turn": self.sim_state.turn,
            "seed": self.sim_rng.state()
        })
    }

    fn compact_sfen_state(&self) -> SnakeFen {
        let (player, ai) = self.player_and_ai();
        SnakeFen {
            cols: self.sim_state.board.width,
            rows: self.sim_state.board.height,
            turn: self.sim_state.turn,
            seed: self.sim_rng.state(),
            p_health: player.map(|s| s.health).unwrap_or(100),
            a_health: ai.map(|s| s.health).unwrap_or(100),
            p_body: player.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default(),
            a_body: ai.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default(),
            foods: self.sim_state.board.food.clone(),
            opponent_moves: Vec::new(),
        }
    }

    fn apply_loaded_state(&mut self, state: snake_domain::GameState, source: impl AsRef<str>) {
        self.sim_rng = LcgRng::new(state.seed as u32);
        self.sim_state = state;
        self.pv_line.clear();
        self.pv_index = 0;
        self.load_error = None;
        self.log_line(source);
    }

    pub(in crate::gui) fn copy_state_to_clipboard(&self, ui: &mut egui::Ui) {
        ui.ctx().copy_text(self.compact_sfen_state().to_string());
    }

    pub(in crate::gui) fn copy_json_state_to_clipboard(&self, ui: &mut egui::Ui) {
        ui.ctx()
            .copy_text(serde_json::to_string_pretty(&self.compact_json_state()).unwrap());
    }

    pub(in crate::gui) fn save_scenario_to_disk(&mut self) {
        let dir = std::path::PathBuf::from(&self.scenario_dir);
        let path = dir.join(format!("{}.json", self.scenario_save_name));
        let (player, ai) = self.player_and_ai();

        let scenario = snake_io::Scenario {
            id: self.scenario_save_name.clone(),
            name: self.scenario_save_name.clone(),
            board: snake_io::ScenarioBoard {
                width: self.sim_state.board.width,
                height: self.sim_state.board.height,
                food: self.sim_state.board.food.clone(),
                snakes: vec![
                    {
                        let body = ai.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
                        let health = ai.map(|s| s.health).unwrap_or(100);
                        snake_io::ScenarioSnake {
                            id: AI_ID.to_owned(),
                            name: "AI".to_owned(),
                            body,
                            health,
                        }
                    },
                    {
                        let body = player.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
                        let health = player.map(|s| s.health).unwrap_or(100);
                        snake_io::ScenarioSnake {
                            id: PLAYER_ID.to_owned(),
                            name: "Player".to_owned(),
                            body,
                            health,
                        }
                    },
                ],
            },
            you_id: AI_ID.to_owned(),
            expectation: snake_io::Expectation::Exact {
                direction: self.scenario_expected_move,
            },
        };

        match snake_io::save_scenario_to_file(&path, &scenario) {
            Ok(_) => {
                self.save_success = Some(format!("Saved to {}", path.display()));
                self.save_error = None;
            }
            Err(e) => {
                self.save_error = Some(e.to_string());
                self.save_success = None;
            }
        }
    }

    pub(in crate::gui) fn load_scenario_from_path(&mut self) {
        self.load_error = None;
        let input = self.scenario_load_path.trim();
        if input.is_empty() {
            self.load_error = Some("Enter a scenario path, JSON blob, or SFEN string".to_owned());
            return;
        }

        if input.starts_with('{') {
            let v: serde_json::Value = match serde_json::from_str(input) {
                Ok(v) => v,
                Err(e) => {
                    self.load_error = Some(format!("JSON err: {}", e));
                    return;
                }
            };

            if let Some(state) = Self::json_to_game_state(&v) {
                self.apply_loaded_state(state, "Loaded inline JSON state.");
            } else {
                self.load_error = Some("Failed to parse scenario JSON".to_owned());
            }
            return;
        }

        let inline_sfen_error = if input.starts_with(SFEN_PREFIX) || input.contains(char::is_whitespace) {
            match SnakeFen::parse(input) {
                Ok(sfen) => {
                    self.apply_loaded_state(sfen.into_game_state(), "Loaded inline SFEN state.");
                    return;
                }
                Err(err) => Some(err.to_string()),
            }
        } else {
            None
        };

        let path = std::path::Path::new(input);
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                self.load_error = Some(inline_sfen_error.unwrap_or_else(|| format!("File error: {}", e)));
                return;
            }
        };

        if let Ok(sfen) = SnakeFen::parse(&raw) {
            self.apply_loaded_state(sfen.into_game_state(), format!("Loaded SFEN: {}", path.display()));
            return;
        }

        let v: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                self.load_error = Some(format!("JSON err: {}", e));
                return;
            }
        };

        if let Some(state) = Self::json_to_game_state(&v) {
            self.apply_loaded_state(state, format!("Loaded scenario: {}", path.display()));
        } else {
            self.load_error = Some("Failed to parse scenario JSON".to_owned());
        }
    }

    /// Parses either the canonical scenario JSON (via `snake_io::Scenario`)
    /// or the compact playground clipboard JSON (`{p, a, foods, cols, ...}`).
    fn json_to_game_state(json: &serde_json::Value) -> Option<snake_domain::GameState> {
        if json.get("cols").is_some_and(|v| v.is_i64()) && json.get("rows").is_some_and(|v| v.is_i64()) {
            return Self::parse_compact_state_to_state(json);
        }

        let scenario: snake_io::Scenario = serde_json::from_value(json.clone()).ok()?;
        scenario.into_game_state()
    }

    fn parse_compact_state_to_state(json: &serde_json::Value) -> Option<snake_domain::GameState> {
        if let (Some(cols), Some(rows)) = (json.get("cols").and_then(|v| v.as_i64()), json.get("rows").and_then(|v| v.as_i64())) {
            let width = cols as i32;
            let height = rows as i32;

            let parse_pts = |k: &str| {
                let mut pts = Vec::new();
                if let Some(arr) = json.get(k).and_then(|v| v.as_array()) {
                    for p in arr {
                        if let (Some(x), Some(y)) = (p.get("x").and_then(|v| v.as_i64()), p.get("y").and_then(|v| v.as_i64())) {
                            pts.push(Point { x: x as i32, y: y as i32 });
                        }
                    }
                }
                pts
            };

            let p_body = parse_pts("p");
            let a_body = parse_pts("a");
            let food = parse_pts("foods");

            let p_health = json.get("pHealth").and_then(|v| v.as_i64()).unwrap_or(100) as i32;
            let a_health = json.get("aHealth").and_then(|v| v.as_i64()).unwrap_or(100) as i32;
            let turn = json.get("turn").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let seed = json.get("seed").and_then(|v| v.as_u64()).unwrap_or(0);

            return Some(snake_domain::GameState {
                turn,
                seed,
                board: snake_domain::Board {
                    width,
                    height,
                    food,
                    snakes: vec![
                        snake_domain::Snake::new(PLAYER_ID, "Player", p_body, p_health),
                        snake_domain::Snake::new(AI_ID, "AI", a_body, a_health),
                    ],
                },
            });
        }

        None
    }
}
