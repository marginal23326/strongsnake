use std::time::Instant;

use eframe::egui;
use snake_ai::{AgentState, Decision, decide_move_debug};
use snake_domain::{Direction, SimConfig, Snake, simulate_turn};

use crate::services::build_playground_state;

use super::state::{EditMode, SnakeGuiApp, Tab};

mod board;
mod scenario;

impl SnakeGuiApp {
    pub(super) fn reset_playground(&mut self) {
        let (state, rng) = build_playground_state(self.sim_state.board.width, self.sim_state.board.height, 1);
        self.sim_state = state;
        self.sim_rng = rng;
        self.auto_run = false;
        self.player_input_queue.clear();
        self.player_dir = Direction::Up;
        self.last_move_ms = 0.0;
        self.pv_line.clear();
        self.pv_index = 0;
    }

    fn is_opposite(a: Direction, b: Direction) -> bool {
        matches!(
            (a, b),
            (Direction::Up, Direction::Down)
                | (Direction::Down, Direction::Up)
                | (Direction::Left, Direction::Right)
                | (Direction::Right, Direction::Left)
        )
    }

    pub(super) fn set_player_dir(&mut self, dir: Direction) {
        let Some(player) = self.sim_state.board.snakes.iter().find(|s| s.id.0 == "s1").cloned() else {
            self.player_dir = dir;
            return;
        };

        if player.body.len() > 1 {
            let head = player.body[0];
            let neck = player.body[1];
            let blocked = (head.x + dir.vector().0 == neck.x) && (head.y + dir.vector().1 == neck.y);
            if blocked {
                return;
            }
        }
        self.player_input_queue.clear();
        self.player_dir = dir;
    }

    pub(super) fn queue_player_input(&mut self, dir: Direction) {
        let last = self.player_input_queue.back().copied().unwrap_or(self.player_dir);
        if dir == last || Self::is_opposite(dir, last) {
            return;
        }
        if self.player_input_queue.len() < 2 {
            self.player_input_queue.push_back(dir);
        }
    }

    fn living_player_and_ai(&self) -> Option<(Snake, Snake)> {
        let player = self.sim_state.board.snakes.iter().find(|s| s.id.0 == "s1").cloned()?;
        let ai = self.sim_state.board.snakes.iter().find(|s| s.id.0 == "s2").cloned()?;
        if !player.alive || !ai.alive {
            return None;
        }
        Some((player, ai))
    }

    pub(super) fn playground_can_run(&self) -> bool {
        self.living_player_and_ai().is_some()
    }

    pub(super) fn set_playground_running(&mut self, playing: bool) {
        self.auto_run = playing && self.playground_can_run();
        if self.auto_run {
            self.last_auto_tick = Instant::now();
        }
    }

    pub(super) fn sync_playground_playback(&mut self) {
        if self.auto_run && !self.playground_can_run() {
            self.auto_run = false;
        }
    }

    fn decide_ai_for_playground(&mut self, player: &Snake, ai: &Snake) -> Decision {
        let mut playground_cfg = self.cfg.clone();
        playground_cfg.max_depth = self.playground_depth.max(1);

        let started = Instant::now();
        let decision = decide_move_debug(
            AgentState {
                body: snake_ai::model::FastBody::from_points(ai.body.iter().copied()),
                health: ai.health,
            },
            AgentState {
                body: snake_ai::model::FastBody::from_points(player.body.iter().copied()),
                health: player.health,
            },
            &self.sim_state.board.food,
            self.sim_state.board.width,
            self.sim_state.board.height,
            &playground_cfg,
        );
        self.last_move_ms = started.elapsed().as_secs_f64() * 1000.0;
        decision
    }

    pub(super) fn evaluate_ai(&mut self) {
        let Some((player, ai)) = self.living_player_and_ai() else {
            return;
        };
        let ai_decision = self.decide_ai_for_playground(&player, &ai);
        self.pv_line = ai_decision.pv;
        self.pv_index = 0;
    }

    pub(super) fn step_playground(&mut self) {
        self.sync_playground_playback();
        self.pv_line.clear();
        self.pv_index = 0;

        let Some((player, ai)) = self.living_player_and_ai() else {
            return;
        };

        if let Some(next) = self.player_input_queue.pop_front() {
            self.player_dir = next;
        }

        let ai_move = self.decide_ai_for_playground(&player, &ai).best_move;

        let intents = [self.player_dir, ai_move];
        let summary = simulate_turn(&mut self.sim_state, &intents, &mut self.sim_rng, SimConfig::default());
        if !summary.dead.is_empty() {
            self.log_line(format!("Turn {} deaths: {:?}", summary.turn, summary.dead));
        }
        self.sync_playground_playback();
    }

    pub(super) fn process_playground_keys(&mut self, ctx: &egui::Context) {
        if self.tab != Tab::Playground || ctx.wants_keyboard_input() {
            return;
        }

        let mut requested = None;
        let mut step_now = false;
        let mut pause_now = false;
        let mut reset_now = false;
        let mut toggle_playback = false;
        let mut next_edit_mode = None;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                pause_now = true;
            } else if i.key_pressed(egui::Key::Enter) {
                toggle_playback = true;
            } else if i.key_pressed(egui::Key::R) {
                reset_now = true;
            } else if i.key_pressed(egui::Key::Space) {
                step_now = true;
            } else if i.key_pressed(egui::Key::Num1) {
                next_edit_mode = Some(EditMode::PaintP1);
            } else if i.key_pressed(egui::Key::Num2) {
                next_edit_mode = Some(EditMode::PaintAi);
            } else if i.key_pressed(egui::Key::Num3) {
                next_edit_mode = Some(EditMode::Food);
            } else if i.key_pressed(egui::Key::Num4) {
                next_edit_mode = Some(EditMode::Erase);
            } else if i.key_pressed(egui::Key::W) || i.key_pressed(egui::Key::ArrowUp) {
                requested = Some(Direction::Up);
            } else if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::ArrowDown) {
                requested = Some(Direction::Down);
            } else if i.key_pressed(egui::Key::A) || i.key_pressed(egui::Key::ArrowLeft) {
                requested = Some(Direction::Left);
            } else if i.key_pressed(egui::Key::D) || i.key_pressed(egui::Key::ArrowRight) {
                requested = Some(Direction::Right);
            }
        });

        if pause_now {
            self.set_playground_running(false);
            return;
        }

        if reset_now {
            self.reset_playground();
            return;
        }

        if toggle_playback {
            self.set_playground_running(!self.auto_run);
            return;
        }

        if let Some(edit_mode) = next_edit_mode {
            self.edit_mode = edit_mode;
            return;
        }

        if step_now {
            self.step_playground();
            return;
        }

        if let Some(dir) = requested {
            if self.auto_run {
                self.queue_player_input(dir);
            } else {
                self.set_player_dir(dir);
            }
        }
    }
}
