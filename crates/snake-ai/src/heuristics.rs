use snake_domain::Point;

use crate::{config::AiConfig, floodfill::flood_fill, grid::Grid, model::AgentState, voronoi::compute_voronoi};

// Tactical terms are measured in territory cells: `scores.territory_control` is the unit.

const PIN_REACH_FROM_WALL: i32 = 3;
const PIN_REACH_ALONG_WALL: i32 = 4;
const PIN_FALLOFF: i32 = 8;
const PIN_CELLS: i32 = 100;
const PIN_MIN_HEALTH: i32 = 15;

const EXIT_NEAR_TURNS: i32 = 2;
const EXIT_NEAR_CELLS: i32 = 5;
const ESCAPE_TURN_CELLS: i32 = 2;

const TIGHT_SPOT_AREA_SHARE: f64 = 0.2;

/// Trap verdicts shrink in a dense tail race, where "sealed" pockets often reopen.
const DENSE_VERDICT_DIVISOR: i32 = 1000;
const DENSE_MIN_LEN: usize = 20;

#[inline(always)]
fn manhattan(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

/// A packed endgame: both snakes are long and together fill `dense_tail_race_occupancy` percent of the board.
pub(crate) fn is_dense_tail_race(my_len: usize, enemy_len: usize, area: usize, cfg: &AiConfig) -> bool {
    my_len >= DENSE_MIN_LEN && enemy_len >= DENSE_MIN_LEN && (my_len + enemy_len) * 100 >= cfg.dense_tail_race_occupancy * area
}

/// How hard `hunter` pins `prey` against a wall.
fn pin_pressure(prey: Point, hunter: Point, width: i32, height: i32) -> i32 {
    let dx = (prey.x - hunter.x).abs();
    let dy = (prey.y - hunter.y).abs();
    let mut pressure = 0;

    if prey.x == 0 || prey.x == width - 1 {
        let wall_gap = if prey.x == 0 { hunter.x } else { width - 1 - hunter.x };
        if wall_gap <= PIN_REACH_FROM_WALL && dy <= PIN_REACH_ALONG_WALL {
            pressure += PIN_FALLOFF - (wall_gap + dy);
        }
    }
    if prey.y == 0 || prey.y == height - 1 {
        let wall_gap = if prey.y == 0 { hunter.y } else { height - 1 - hunter.y };
        if wall_gap <= PIN_REACH_FROM_WALL && dx <= PIN_REACH_ALONG_WALL {
            pressure += PIN_FALLOFF - (wall_gap + dx);
        }
    }
    pressure
}

/// How much room a snake has, given the other snake's body.
#[derive(Debug, Clone, Copy)]
enum Confinement {
    Trapped,
    Roomy { exit_near: bool },
    Cramped { escape_turns: i32 },
}

impl Confinement {
    /// Worth to the confined snake; `trapped` is the verdict for being sealed in.
    fn value(self, trapped: i32, cfg: &AiConfig) -> i32 {
        let unit = cfg.scores.territory_control;
        match self {
            Self::Trapped => trapped,
            Self::Roomy { exit_near: true } => unit * EXIT_NEAR_CELLS,
            Self::Roomy { exit_near: false } => cfg.scores.strategic_squeeze,
            Self::Cramped { escape_turns } => -escape_turns * unit * ESCAPE_TURN_CELLS,
        }
    }
}

fn confinement<const W: usize, const N: usize>(grid: &Grid<W, N>, snake: &AgentState, rival: &AgentState) -> Confinement {
    let head = snake.body.head();
    let len = snake.body.len() as i32;

    let fill = flood_fill(grid, head.x, head.y, len + 2, Some(&snake.body), Some(&rival.body));
    let food_bonus = fill.has_food as i32;
    let escape_turns = fill.min_turns_to_clear.saturating_add(food_bonus);
    let future_len = len + food_bonus;

    if fill.count < future_len && fill.count < escape_turns {
        Confinement::Trapped
    } else if fill.count >= future_len {
        let exit_near = manhattan(head, snake.body.last()) <= EXIT_NEAR_TURNS || escape_turns <= EXIT_NEAR_TURNS;
        Confinement::Roomy { exit_near }
    } else {
        Confinement::Cramped { escape_turns }
    }
}

pub fn evaluate<const W: usize, const N: usize>(grid: &mut Grid<W, N>, me: &AgentState, enemy: &AgentState, cfg: &AiConfig) -> i32
{
    #[cfg(feature = "profiling")]
    {
        let start = std::time::Instant::now();
        let res = evaluate_inner(grid, me, enemy, cfg);
        crate::PERF_STATS.with(|s| {
            let mut st = s.borrow_mut();
            st.eval_calls += 1;
            st.eval_duration += start.elapsed();
        });
        res
    }
    #[cfg(not(feature = "profiling"))]
    {
        evaluate_inner(grid, me, enemy, cfg)
    }
}

fn evaluate_inner<const W: usize, const N: usize>(grid: &mut Grid<W, N>, me: &AgentState, enemy: &AgentState, cfg: &AiConfig) -> i32
{
    let scores = &cfg.scores;
    if me.health <= 0 || me.body.is_empty() {
        return scores.loss;
    }
    if enemy.health <= 0 || enemy.body.is_empty() {
        return scores.win;
    }

    let area = (grid.width * grid.height) as usize;
    let dense = is_dense_tail_race(me.body.len(), enemy.body.len(), area, cfg);
    let scale_verdict = |verdict: i32| if dense { verdict / DENSE_VERDICT_DIVISOR } else { verdict };
    let unit = scores.territory_control;

    let my_head = me.body.head();
    let enemy_head = enemy.body.head();

    let mut score = me.body.len() as i32 * scores.length;

    if me.health > PIN_MIN_HEALTH {
        let pin_unit = unit * PIN_CELLS;
        score -= pin_unit * pin_pressure(my_head, enemy_head, grid.width, grid.height);
        score += pin_unit * pin_pressure(enemy_head, my_head, grid.width, grid.height);
    }

    // My tail leaves its cell this turn, so it must not block my expansion.
    let leaving_tail = (me.health < 100).then(|| {
        let tail = me.body.last();
        (tail, grid.get(tail.x, tail.y))
    });
    if let Some((tail, _)) = leaving_tail {
        grid.set(tail.x, tail.y, 0);
    }
    let voronoi = compute_voronoi(grid, my_head, enemy_head);
    if let Some((tail, original)) = leaving_tail {
        grid.set(tail.x, tail.y, original);
    }

    score += (voronoi.my_count - voronoi.enemy_count) * unit;

    let mut i_am_trapped = false;
    if voronoi.my_count < me.body.len() as i32 {
        let state = confinement(grid, me, enemy);
        i_am_trapped = matches!(state, Confinement::Trapped);
        score += state.value(scale_verdict(scores.trap_danger), cfg);
    } else if (voronoi.my_count as f64) < area as f64 * TIGHT_SPOT_AREA_SHARE {
        score += scores.tight_spot;
    }

    // The enemy's side mirrors mine; if I am sealed in, that already decided things.
    if !i_am_trapped && voronoi.enemy_count < enemy.body.len() as i32 {
        let state = confinement(grid, enemy, me);
        score -= state.value(-scale_verdict(scores.enemy_trapped), cfg);
    }

    let dist_to_opp = manhattan(my_head, enemy_head);
    if dist_to_opp == 1 && me.body.len() > enemy.body.len() {
        score += scores.kill_pressure;
    }

    if grid.food.any() {
        let mut closest_dist = i32::MAX;
        let mut food = grid.food;
        while let Some(idx) = food.pop_first() {
            let cell = Point {
                x: (idx as i32) % grid.width,
                y: (idx as i32) / grid.width,
            };
            closest_dist = closest_dist.min(manhattan(my_head, cell));
        }

        if closest_dist > me.health {
            return scores.loss;
        }

        let buffer = me.health - closest_dist;
        let panic_value = if buffer > 0 {
            scores.food.intensity * (scores.food.threshold / (buffer as f64 + 1.0)).powf(scores.food.exponent)
        } else {
            scores.food.intensity * 100.0
        };
        score -= (closest_dist as f64 * panic_value) as i32;
    }

    if me.body.len() > enemy.body.len() + 1 {
        score -= dist_to_opp * scores.aggression;
    }

    score
}
