use eframe::egui;
use snake_domain::{LcgRng, Point, Snake};
use snake_io::{SFEN_PREFIX, SnakeFen};

use super::super::state::SnakeGuiApp;

impl SnakeGuiApp {
    fn player_and_ai(&self) -> (Option<&Snake>, Option<&Snake>) {
        let player = self.sim_state.board.snakes.iter().find(|s| s.id.0 == "s1");
        let ai = self.sim_state.board.snakes.iter().find(|s| s.id.0 == "s2");
        (player, ai)
    }

    fn compact_json_state(&self) -> serde_json::Value {
        let (s1, s2) = self.player_and_ai();
        let p_body = s1.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
        let a_body = s2.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
        serde_json::json!({
            "p": p_body,
            "a": a_body,
            "foods": self.sim_state.board.food.clone(),
            "pHealth": s1.map(|s| s.health).unwrap_or(100),
            "aHealth": s2.map(|s| s.health).unwrap_or(100),
            "cols": self.sim_state.board.width,
            "rows": self.sim_state.board.height,
            "turn": self.sim_state.turn,
            "seed": self.sim_rng.state()
        })
    }

    fn compact_sfen_state(&self) -> SnakeFen {
        let (s1, s2) = self.player_and_ai();
        SnakeFen {
            cols: self.sim_state.board.width,
            rows: self.sim_state.board.height,
            turn: self.sim_state.turn,
            seed: self.sim_rng.state(),
            p_health: s1.map(|s| s.health).unwrap_or(100),
            a_health: s2.map(|s| s.health).unwrap_or(100),
            p_body: s1.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default(),
            a_body: s2.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default(),
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
        let path = std::path::PathBuf::from(&self.scenario_dir).join(format!("{}.json", self.scenario_save_name));
        let (s1, s2) = self.player_and_ai();
        let s2_body = s2.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();
        let s1_body = s1.map(|s| s.body.iter().copied().collect::<Vec<_>>()).unwrap_or_default();

        let json = serde_json::json!({
            "id": format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
            "name": self.scenario_save_name,
            "you_id": "s2",
            "expectation": {
                "kind": "exact",
                "direction": self.scenario_expected_move.as_lower()
            },
            "board": {
                "width": self.sim_state.board.width,
                "height": self.sim_state.board.height,
                "food": self.sim_state.board.food,
                "snakes": [
                    {
                        "id": "s2",
                        "name": "AI",
                        "body": s2_body,
                        "health": s2.map(|s| s.health).unwrap_or(100)
                    },
                    {
                        "id": "s1",
                        "name": "Player",
                        "body": s1_body,
                        "health": s1.map(|s| s.health).unwrap_or(100)
                    }
                ]
            }
        });

        let _ = std::fs::create_dir_all(&self.scenario_dir);
        match std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()) {
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

            if let Some(state) = Self::parse_scenario_to_state(&v) {
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

        if let Some(state) = Self::parse_scenario_to_state(&v) {
            self.apply_loaded_state(state, format!("Loaded scenario: {}", path.display()));
        } else {
            self.load_error = Some("Failed to parse scenario JSON".to_owned());
        }
    }

    fn parse_scenario_to_state(json: &serde_json::Value) -> Option<snake_domain::GameState> {
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
                        snake_domain::Snake::new("s1", "s1", p_body, p_health),
                        snake_domain::Snake::new("s2", "s2", a_body, a_health),
                    ],
                },
            });
        }

        let board = json.get("board")?;
        let width = board.get("width")?.as_i64()? as i32;
        let height = board.get("height")?.as_i64()? as i32;

        let mut food = Vec::new();
        if let Some(f_arr) = board.get("food").and_then(|v| v.as_array()) {
            for f in f_arr {
                let x = f.get("x")?.as_i64()? as i32;
                let y = f.get("y")?.as_i64()? as i32;
                food.push(Point { x, y });
            }
        }

        let mut snakes = Vec::new();
        let you_id = json.get("you_id").and_then(|v| v.as_str()).unwrap_or("");

        if let Some(s_arr) = board.get("snakes").and_then(|v| v.as_array()) {
            // Find which one is the AI. Default to the second snake if not found.
            let mut ai_index = 1.min(s_arr.len().saturating_sub(1));
            for (i, s) in s_arr.iter().enumerate() {
                let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if (!you_id.is_empty() && id == you_id) || id == "ai-snake-for-test" {
                    ai_index = i;
                    break;
                }
            }

            let mut non_ai_count = 1;
            for (i, s) in s_arr.iter().enumerate() {
                let is_ai = i == ai_index;
                let new_id = if is_ai {
                    "s2".to_string()
                } else {
                    let id = if non_ai_count == 1 {
                        "s1".to_string()
                    } else {
                        format!("s_other_{}", non_ai_count)
                    };
                    non_ai_count += 1;
                    id
                };

                let health = s.get("health").and_then(|v| v.as_i64()).unwrap_or(100) as i32;
                let mut body = Vec::new();
                if let Some(b_arr) = s.get("body").and_then(|v| v.as_array()) {
                    for b in b_arr {
                        let x = b.get("x")?.as_i64()? as i32;
                        let y = b.get("y")?.as_i64()? as i32;
                        body.push(Point { x, y });
                    }
                }
                snakes.push(snake_domain::Snake::new(new_id.as_str(), new_id.as_str(), body, health));
            }
        }

        Some(snake_domain::GameState {
            turn: 0,
            seed: 0,
            board: snake_domain::Board {
                width,
                height,
                food,
                snakes,
            },
        })
    }
}
