use std::cell::RefCell;

use snake_domain::Point;

use crate::{config::{AiConfig, FoodCurveConfig}, grid::Grid, model::AgentState, voronoi::compute_voronoi};

/// Both snakes must be at least this long for a position to count as a dense tail race.
const DENSE_MIN_LEN: usize = 16;
/// Room term: a snake with less than this many Voronoi cells per body segment is cramped; found through self-play data.
const CRAMP_THRESHOLD: f64 = 1.3;
/// Territory cells per unit of shortfall below `CRAMP_THRESHOLD`.
const CRAMP_WEIGHT: f64 = 100.0;

/// Most turns of lead the food race counts either way, and the turn count standing for a flood that never reaches food.
const FOOD_RACE_CAP: i32 = 6;
const FOOD_RACE_NEVER: i32 = 20;

#[inline(always)]
fn manhattan(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

/// Buffers (health left after reaching the closest food) with a cached panic value.
const PANIC_TABLE_LEN: usize = 128;

struct PanicTable {
    /// The curve parameters the values were computed for.
    curve: [u64; 3],
    values: [f64; PANIC_TABLE_LEN],
}

thread_local! {
    static PANIC_TABLE: RefCell<Option<PanicTable>> = const { RefCell::new(None) };
}

/// How hard the food term pushes toward food, per step of distance, when `buffer` turns of health are to spare.
fn panic_value(food: &FoodCurveConfig, buffer: i32) -> f64 {
    if buffer <= 0 {
        return food.intensity * 100.0;
    }
    let curve = |buffer: i32| food.intensity * (food.threshold / (buffer as f64 + 1.0)).powf(food.exponent);
    if buffer as usize >= PANIC_TABLE_LEN {
        return curve(buffer);
    }
    let key = [food.intensity.to_bits(), food.threshold.to_bits(), food.exponent.to_bits()];
    PANIC_TABLE.with_borrow_mut(|cache| {
        let table = match cache {
            Some(table) if table.curve == key => table,
            _ => cache.insert(PanicTable {
                curve: key,
                values: std::array::from_fn(|b| if b == 0 { 0.0 } else { curve(b as i32) }),
            }),
        };
        table.values[buffer as usize]
    })
}

/// A long-snake endgame: both snakes are at least `DENSE_MIN_LEN` long. The quiescence extension only runs in these
/// positions.
pub(crate) fn is_dense_tail_race(my_len: usize, enemy_len: usize) -> bool {
    my_len >= DENSE_MIN_LEN && enemy_len >= DENSE_MIN_LEN
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
    let cramp = |count: i32, len: usize| (CRAMP_THRESHOLD - count as f64 / len as f64).max(0.0);
    score += ((cramp(voronoi.enemy_count, enemy.body.len()) - cramp(voronoi.my_count, me.body.len())) * CRAMP_WEIGHT * unit as f64) as i32;
    score += (voronoi.my_food - voronoi.enemy_food) * unit * 3;

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

        // Path distance around bodies when the Voronoi flood found a food cell, else the straight-line guess.
        let closest_dist = if voronoi.my_food_turn > 0 { voronoi.my_food_turn } else { closest_dist };
        let buffer = me.health - closest_dist;
        score -= (closest_dist as f64 * panic_value(&scores.food, buffer)) as i32;

        // Food race: each turn sooner than the enemy my flood first reaches a food cell is worth a territory cell.
        let mine = if voronoi.my_food_turn > 0 { voronoi.my_food_turn } else { FOOD_RACE_NEVER };
        let theirs = if voronoi.en_food_turn > 0 { voronoi.en_food_turn } else { FOOD_RACE_NEVER };
        score += (theirs - mine).clamp(-FOOD_RACE_CAP, FOOD_RACE_CAP) * unit;
    }

    score
}
