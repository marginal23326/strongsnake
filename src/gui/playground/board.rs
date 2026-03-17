use eframe::egui::{self, Color32, Rect, Stroke, pos2, vec2};
use snake_domain::{Direction, Point, SimConfig, simulate_turn};

use super::super::state::{EditMode, SnakeGuiApp};

impl SnakeGuiApp {
    fn get_projected_state(&self) -> snake_domain::GameState {
        let mut state = self.sim_state.clone();
        let mut rng = self.sim_rng.clone();
        let max_turns = self.pv_line.len() / 2;
        let turns_to_simulate = self.pv_index.min(max_turns);

        let mut p_idx = 0;
        for _ in 0..turns_to_simulate {
            let m_s2 = self.pv_line.get(p_idx).copied().unwrap_or(Direction::Up);
            let m_s1 = self.pv_line.get(p_idx + 1).copied().unwrap_or(Direction::Up);
            p_idx += 2;

            let intents = [m_s1, m_s2];
            let _ = simulate_turn(&mut state, &intents, &mut rng, SimConfig::default());
        }
        state
    }

    fn get_line_path(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
        let mut path = vec![from];
        let (mut cx, mut cy) = from;
        while cx != to.0 || cy != to.1 {
            let dx = (to.0 - cx).signum();
            let dy = (to.1 - cy).signum();
            if dx != 0 {
                cx += dx;
            } else if dy != 0 {
                cy += dy;
            }
            path.push((cx, cy));
        }
        path
    }

    fn sync_snake_state_after_edit(&mut self) {
        for snake in &mut self.sim_state.board.snakes {
            snake.alive = !snake.body.is_empty();
            if snake.alive && snake.health <= 0 {
                snake.health = 100;
            }
            if !snake.alive {
                snake.health = 0;
            }
        }
    }

    fn apply_edit_cell(&mut self, x: i32, y: i32, is_new_stroke: bool) {
        match self.edit_mode {
            EditMode::Erase => {
                self.sim_state.board.food.retain(|f| f.x != x || f.y != y);
                for snake in &mut self.sim_state.board.snakes {
                    snake.body.retain(|p| p.x != x || p.y != y);
                }
            }
            EditMode::Food => {
                if !self.sim_state.board.food.iter().any(|f| f.x == x && f.y == y) {
                    self.sim_state.board.food.push(Point { x, y });
                }
            }
            EditMode::PaintP1 | EditMode::PaintAi => {
                let target_id = if self.edit_mode == EditMode::PaintP1 { "s1" } else { "s2" };
                if let Some(snake) = self.sim_state.board.snakes.iter_mut().find(|s| s.id.0 == target_id) {
                    if is_new_stroke {
                        snake.body.clear();
                    }
                    if let Some(idx) = snake.body.iter().position(|p| p.x == x && p.y == y) {
                        snake.body.truncate(idx);
                    }
                    snake.body.push_back(Point { x, y });
                    snake.health = 100;
                    snake.alive = true;
                }
            }
        }
        self.sync_snake_state_after_edit();
    }

    fn pos_to_cell(pos: egui::Pos2, rect: egui::Rect, width: i32, height: i32, cell_size: f32) -> Option<(i32, i32)> {
        let x = ((pos.x - rect.left()) / cell_size).floor() as i32;
        let y_top = ((pos.y - rect.top()) / cell_size).floor() as i32;
        let y = height - 1 - y_top;
        if x < 0 || y < 0 || x >= width || y >= height {
            return None;
        }
        Some((x, y))
    }

    fn handle_board_input(&mut self, response: &egui::Response, rect: egui::Rect, width: i32, height: i32, cell_size: f32) {
        if response.clicked() || response.drag_started() {
            self.is_drawing = true;
            self.last_draw_cell = None;
        }

        let pointer_down = response.dragged() || response.is_pointer_button_down_on();
        if !pointer_down {
            self.is_drawing = false;
            self.last_draw_cell = None;
            return;
        }
        if !self.is_drawing {
            return;
        }

        let Some(pos) = response.interact_pointer_pos() else {
            return;
        };
        let Some((x, y)) = Self::pos_to_cell(pos, rect, width, height, cell_size) else {
            return;
        };

        let cells = if let Some(prev) = self.last_draw_cell {
            if prev == (x, y) {
                Vec::new()
            } else {
                Self::get_line_path(prev, (x, y))
            }
        } else {
            vec![(x, y)]
        };

        if cells.is_empty() {
            return;
        }
        let is_new_stroke = self.last_draw_cell.is_none();
        for (idx, (cx, cy)) in cells.into_iter().enumerate() {
            self.apply_edit_cell(cx, cy, is_new_stroke && idx == 0);
        }
        self.last_draw_cell = Some((x, y));
    }

    pub(in crate::gui) fn draw_playground_board(&mut self, ui: &mut egui::Ui) {
        let projected_state = (self.pv_index > 0).then(|| self.get_projected_state());
        let board = projected_state.as_ref().map(|state| &state.board).unwrap_or(&self.sim_state.board);
        let width = board.width;
        let height = board.height;

        let available = ui.available_size();
        let aspect_ratio = width as f32 / height as f32;

        // Ensure perfectly square cells by constraining the rect to the exact aspect ratio.
        let mut desired = available;
        if desired.x / desired.y > aspect_ratio {
            desired.x = desired.y * aspect_ratio;
        } else {
            desired.y = desired.x / aspect_ratio;
        }

        let (outer_rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

        // Center the board inside the available space.
        let rect = Rect::from_center_size(outer_rect.center(), desired);
        let cell_size = rect.width() / width as f32;

        let painter = ui.painter_at(outer_rect);

        // Background.
        painter.rect_filled(rect, 4.0, Color32::from_rgb(9, 13, 18));
        painter.rect_stroke(rect, 4.0, Stroke::new(2.0, Color32::from_rgb(48, 54, 61)), egui::StrokeKind::Inside);

        // Grid lines.
        let grid_stroke = Stroke::new(1.0, Color32::from_rgb(33, 38, 45));
        for x in 1..width {
            let px = rect.left() + x as f32 * cell_size;
            painter.line_segment([pos2(px, rect.top()), pos2(px, rect.bottom())], grid_stroke);
        }
        for y in 1..height {
            let py = rect.top() + y as f32 * cell_size;
            painter.line_segment([pos2(rect.left(), py), pos2(rect.right(), py)], grid_stroke);
        }

        // Cell Center Helper.
        let cell_center = |x: i32, y: i32| -> egui::Pos2 {
            pos2(
                rect.left() + (x as f32 + 0.5) * cell_size,
                rect.bottom() - (y as f32 + 0.5) * cell_size,
            )
        };

        // Hover Effect.
        if let Some(hover_pos) = response.hover_pos() {
            if let Some((hx, hy)) = Self::pos_to_cell(hover_pos, rect, width, height, cell_size) {
                let hover_color = match self.edit_mode {
                    EditMode::PaintP1 => Color32::from_rgba_unmultiplied(88, 166, 255, 60),
                    EditMode::PaintAi => Color32::from_rgba_unmultiplied(255, 123, 114, 60),
                    EditMode::Food => Color32::from_rgba_unmultiplied(126, 231, 135, 60),
                    EditMode::Erase => Color32::from_rgba_unmultiplied(255, 255, 255, 20),
                };
                let h_rect = Rect::from_min_size(
                    pos2(rect.left() + hx as f32 * cell_size, rect.bottom() - (hy as f32 + 1.0) * cell_size),
                    vec2(cell_size, cell_size),
                );
                painter.rect_filled(h_rect, 0.0, hover_color);
            }
        }

        {
            // Food.
            for food in &board.food {
                let center = cell_center(food.x, food.y);
                let radius = cell_size * 0.35;
                painter.circle_filled(center, radius, Color32::from_rgb(63, 185, 80));
                painter.circle_stroke(center, radius, Stroke::new(1.5, Color32::from_rgb(126, 231, 135)));
            }

            // Snakes.
            for snake in &board.snakes {
                if snake.body.is_empty() {
                    continue;
                }

                let (body_col, head_col) = if snake.id.0 == "s1" {
                    (Color32::from_rgb(31, 111, 235), Color32::from_rgb(88, 166, 255))
                } else {
                    (Color32::from_rgb(215, 58, 73), Color32::from_rgb(255, 123, 114))
                };

                let stroke_width = cell_size * 0.55;
                let radius = stroke_width / 2.0;

                // Draw Body connections as thick continuous lines + circle joints.
                if snake.body.len() > 1 {
                    let mut points = Vec::with_capacity(snake.body.len());
                    for p in &snake.body {
                        points.push(cell_center(p.x, p.y));
                    }

                    let stroke = Stroke::new(stroke_width, body_col);
                    for i in 0..(points.len() - 1) {
                        painter.line_segment([points[i], points[i + 1]], stroke);
                    }

                    // Draw circles at all inner joints and the tail to make it a perfect continuous path.
                    for point in points.iter().skip(1) {
                        painter.circle_filled(*point, radius, body_col);
                    }
                }

                // Draw Head over the first point.
                let head_p = snake.body[0];
                let head_rect = Rect::from_center_size(cell_center(head_p.x, head_p.y), vec2(cell_size * 0.8, cell_size * 0.8));
                painter.rect_filled(head_rect, cell_size * 0.25, head_col);

                // Draw small eyes for a nice visual touch based on movement direction.
                if snake.body.len() > 1 {
                    let neck = snake.body[1];
                    let dx = head_p.x - neck.x;
                    let dy = head_p.y - neck.y;

                    let (ex1, ey1, ex2, ey2) = if dx > 0 {
                        (0.2, 0.2, 0.2, -0.2)
                    } else if dx < 0 {
                        (-0.2, 0.2, -0.2, -0.2)
                    } else if dy > 0 {
                        (0.2, 0.2, -0.2, 0.2)
                    } else {
                        (0.2, -0.2, -0.2, -0.2)
                    };

                    let c = cell_center(head_p.x, head_p.y);
                    let eye_radius = cell_size * 0.12;
                    painter.circle_filled(
                        c + vec2(ex1 * cell_size, -ey1 * cell_size),
                        eye_radius,
                        Color32::from_rgb(13, 17, 23),
                    );
                    painter.circle_filled(
                        c + vec2(ex2 * cell_size, -ey2 * cell_size),
                        eye_radius,
                        Color32::from_rgb(13, 17, 23),
                    );
                } else {
                    // Default eyes if length 1.
                    let c = cell_center(head_p.x, head_p.y);
                    let eye_radius = cell_size * 0.12;
                    painter.circle_filled(
                        c + vec2(0.2 * cell_size, -0.2 * cell_size),
                        eye_radius,
                        Color32::from_rgb(13, 17, 23),
                    );
                    painter.circle_filled(
                        c + vec2(-0.2 * cell_size, -0.2 * cell_size),
                        eye_radius,
                        Color32::from_rgb(13, 17, 23),
                    );
                }
            }
        }

        self.handle_board_input(&response, rect, width, height, cell_size);
    }
}
