use snake_domain::Point;

use crate::{config::AiConfig, floodfill::flood_fill, grid::Grid, model::AgentState, voronoi::compute_voronoi};

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

/// Whether `snake` is sealed into a pocket it cannot get out of, given the rival's body.
fn is_sealed_in<const W: usize, const N: usize>(grid: &Grid<W, N>, snake: &AgentState, rival: &AgentState) -> bool {
    let head = snake.body.head();
    let len = snake.body.len() as i32;

    let fill = flood_fill(grid, head.x, head.y, len + 2, Some(&snake.body), Some(&rival.body));
    let food_bonus = fill.has_food as i32;
    let escape_turns = fill.min_turns_to_clear.saturating_add(food_bonus);

    fill.count < len + food_bonus && fill.count < escape_turns
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

    // Both tails leave their cells this turn (unless the snake just ate), so neither may block the other's expansion.
    let leaving_tails = [me, enemy].map(|snake| {
        (snake.health < 100).then(|| {
            let tail = snake.body.last();
            (tail, grid.get(tail.x, tail.y))
        })
    });
    for &(tail, _) in leaving_tails.iter().flatten() {
        grid.set(tail.x, tail.y, 0);
    }
    let voronoi = compute_voronoi(grid, my_head, enemy_head, me.body.len(), enemy.body.len(), me, enemy);
    for &(tail, original) in leaving_tails.iter().flatten() {
        grid.set(tail.x, tail.y, original);
    }

    score += (voronoi.my_count - voronoi.enemy_count) * unit;

    let i_am_sealed = voronoi.my_count < me.body.len() as i32 && is_sealed_in(grid, me, enemy);
    if i_am_sealed {
        score += scale_verdict(scores.trap_danger);
    } else if voronoi.enemy_count < enemy.body.len() as i32 && is_sealed_in(grid, enemy, me) {
        // If I am sealed in, that already decided things.
        score += scale_verdict(scores.enemy_trapped);
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

    score
}
