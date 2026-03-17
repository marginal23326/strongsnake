use serde::{Deserialize, Serialize};

use crate::{Direction, FoodSettings, GameState, Point, RngSource, SnakeId, apply_standard_food_spawning};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SimConfig {
    pub max_health: i32,
    pub food: FoodSettings,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            max_health: 100,
            food: FoodSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathEvent {
    pub snake_id: SnakeId,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSummary {
    pub turn: u32,
    pub dead: Vec<DeathEvent>,
    pub alive_ids: Vec<SnakeId>,
}

pub fn simulate_turn<R: RngSource>(state: &mut GameState, intents: &[(SnakeId, Direction)], rng: &mut R, cfg: SimConfig) -> TurnSummary {
    let width = state.board.width;
    let height = state.board.height;

    debug_assert!((width * height) as usize <= 448, "Board size too large for engine capacity");

    for snake in &mut state.board.snakes {
        if !snake.alive || snake.body.is_empty() {
            snake.alive = false;
            continue;
        }

        let dir = intents
            .iter()
            .find(|(id, _)| id.0 == snake.id.0)
            .map(|(_, d)| *d)
            .unwrap_or(Direction::Up);

        let Some(current_head) = snake.body.front().copied() else {
            snake.alive = false;
            continue;
        };
        let head = current_head.moved(dir);
        snake.body.push_front(head);

        if let Some(food_idx) = state.board.food.iter().position(|f| f.x == head.x && f.y == head.y) {
            state.board.food.remove(food_idx);
            snake.health = cfg.max_health;
        } else {
            snake.body.pop_back();
            snake.health -= 1;
        }
    }

    state.turn += 1;

    let mut dead = Vec::new();
    let mut body_grid = [false; 448];

    for snake in &state.board.snakes {
        if !snake.alive || snake.body.is_empty() {
            continue;
        }
        for part in snake.body.iter().skip(1) {
            if part.x >= 0 && part.x < width && part.y >= 0 && part.y < height {
                body_grid[(part.y * width + part.x) as usize] = true;
            }
        }
    }

    for (idx, snake) in state.board.snakes.iter().enumerate() {
        if !snake.alive || snake.body.is_empty() {
            continue;
        }
        let Some(head) = snake.body.front().copied() else {
            continue;
        };

        let out_of_bounds = head.x < 0 || head.y < 0 || head.x >= width || head.y >= height;
        if out_of_bounds {
            dead.push(DeathEvent {
                snake_id: snake.id.clone(),
                reason: "Wall".to_owned(),
            });
            continue;
        }

        if snake.health <= 0 {
            dead.push(DeathEvent {
                snake_id: snake.id.clone(),
                reason: "Starvation".to_owned(),
            });
            continue;
        }

        let head_idx = (head.y * width + head.x) as usize;
        if body_grid[head_idx] {
            dead.push(DeathEvent {
                snake_id: snake.id.clone(),
                reason: "Body".to_owned(),
            });
            continue;
        }

        let mut head_hit = false;
        for (other_idx, other) in state.board.snakes.iter().enumerate() {
            if idx == other_idx || other.body.is_empty() {
                continue;
            }
            let Some(other_head) = other.body.front().copied() else {
                continue;
            };
            if other_head.x == head.x && other_head.y == head.y && snake.body.len() <= other.body.len() {
                head_hit = true;
                break;
            }
        }

        if head_hit {
            dead.push(DeathEvent {
                snake_id: snake.id.clone(),
                reason: "Head".to_owned(),
            });
        }
    }

    if !dead.is_empty() {
        for snake in &mut state.board.snakes {
            if dead.iter().any(|d| d.snake_id.0 == snake.id.0) {
                snake.alive = false;
                snake.body.clear();
            }
        }
    }

    apply_standard_food_spawning(
        rng,
        state.board.width,
        state.board.height,
        &state.board.snakes,
        &mut state.board.food,
        cfg.food,
    );

    let alive_ids = state
        .board
        .snakes
        .iter()
        .filter(|s| s.alive && !s.body.is_empty())
        .map(|s| s.id.clone())
        .collect();

    TurnSummary {
        turn: state.turn,
        dead,
        alive_ids,
    }
}

pub fn snake_head_direction(body: &std::collections::VecDeque<Point>) -> Direction {
    if body.len() < 2 {
        return Direction::Up;
    }
    let head = body[0];
    let neck = body[1];
    if head.x > neck.x {
        Direction::Right
    } else if head.x < neck.x {
        Direction::Left
    } else if head.y > neck.y {
        Direction::Up
    } else {
        Direction::Down
    }
}
