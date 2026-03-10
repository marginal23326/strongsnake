use snake_domain::Point;

use crate::{
    config::AiConfig,
    floodfill::flood_fill,
    grid::Grid,
    model::{AgentState, SearchBuffers},
    voronoi::compute_voronoi,
};

#[inline(always)]
fn manhattan(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

pub fn evaluate<const N: usize>(
    grid: &mut Grid<N>,
    me: &AgentState,
    enemy: &AgentState,
    dist_map: Option<&[i16]>,
    cfg: &AiConfig,
    buffers: &mut SearchBuffers,
) -> i32
where
    [(); (N + 63) / 64]: Sized,
{
    #[cfg(feature = "profiling")]
    {
        let start = std::time::Instant::now();
        let res = evaluate_inner(grid, me, enemy, dist_map, cfg, buffers);
        crate::PERF_STATS.with(|s| {
            let mut st = s.borrow_mut();
            st.eval_calls += 1;
            st.eval_duration += start.elapsed();
        });
        res
    }
    #[cfg(not(feature = "profiling"))]
    {
        evaluate_inner(grid, me, enemy, dist_map, cfg, buffers)
    }
}

fn evaluate_inner<const N: usize>(
    grid: &mut Grid<N>,
    me: &AgentState,
    enemy: &AgentState,
    dist_map: Option<&[i16]>,
    cfg: &AiConfig,
    buffers: &mut SearchBuffers,
) -> i32
where
    [(); (N + 63) / 64]: Sized,
{
    if me.health <= 0 || me.body.is_empty() {
        return cfg.scores.loss;
    }
    if enemy.health <= 0 || enemy.body.is_empty() {
        return cfg.scores.win;
    }

    let mut score: i32 = 0;

    let total_len = me.body.len() + enemy.body.len();
    let total_area = (grid.width * grid.height) as usize;
    let dense_tail_race =
        me.body.len() >= 20 && enemy.body.len() >= 20 && (total_len * 100) >= (cfg.dense_tail_race_occupancy * total_area);

    score += me.body.len() as i32 * cfg.scores.length;

    let my_head = me.body.head();
    let enemy_head = enemy.body.head();

    if me.health > 15 {
        let base_pin = cfg.scores.territory_control.abs() * 100; // ~326,500
        let mut my_pin_penalty = 0;

        let dx = (my_head.x - enemy_head.x).abs();
        let dy = (my_head.y - enemy_head.y).abs();

        if my_head.x == 0 || my_head.x == grid.width - 1 {
            let ex_dist = if my_head.x == 0 {
                enemy_head.x
            } else {
                grid.width - 1 - enemy_head.x
            };
            if ex_dist <= 3 && dy <= 4 {
                my_pin_penalty += base_pin * (8 - (ex_dist + dy));
            }
        }
        if my_head.y == 0 || my_head.y == grid.height - 1 {
            let ey_dist = if my_head.y == 0 {
                enemy_head.y
            } else {
                grid.height - 1 - enemy_head.y
            };
            if ey_dist <= 3 && dx <= 4 {
                my_pin_penalty += base_pin * (8 - (ey_dist + dx));
            }
        }

        let mut enemy_pin_penalty = 0;
        if enemy_head.x == 0 || enemy_head.x == grid.width - 1 {
            let mx_dist = if enemy_head.x == 0 { my_head.x } else { grid.width - 1 - my_head.x };
            if mx_dist <= 3 && dy <= 4 {
                enemy_pin_penalty += base_pin * (8 - (mx_dist + dy));
            }
        }
        if enemy_head.y == 0 || enemy_head.y == grid.height - 1 {
            let my_dist = if enemy_head.y == 0 {
                my_head.y
            } else {
                grid.height - 1 - my_head.y
            };
            if my_dist <= 3 && dx <= 4 {
                enemy_pin_penalty += base_pin * (8 - (my_dist + dx));
            }
        }

        score -= my_pin_penalty;
        score += enemy_pin_penalty;
    }

    let mut tail_is_safe = false;
    let mut original_tail_val = 0i8;
    if !me.body.is_empty() {
        let tail = me.body.last();
        if me.health < 100 {
            tail_is_safe = true;
            original_tail_val = grid.get(tail.x, tail.y);
            grid.set(tail.x, tail.y, 0);
        }
    }

    let voronoi = compute_voronoi(grid, my_head, enemy_head, buffers);
    if tail_is_safe {
        let tail = me.body.last();
        grid.set(tail.x, tail.y, original_tail_val);
    }

    score += (voronoi.my_count - voronoi.enemy_count) * cfg.scores.territory_control;

    let mut i_am_in_death_trap = false;
    let my_len = me.body.len() as i32;

    if voronoi.my_count < my_len {
        let ff = flood_fill(grid, my_head.x, my_head.y, my_len + 2, Some(&me.body), Some(&enemy.body), buffers);
        let food_mod = if ff.has_food { 1 } else { 0 };
        let escape_time = ff.min_turns_to_clear.saturating_add(food_mod);
        let future_len = my_len + food_mod;

        if ff.count < future_len && ff.count < escape_time {
            i_am_in_death_trap = true;
            let trap_score = if dense_tail_race {
                cfg.scores.trap_danger / 1000
            } else {
                cfg.scores.trap_danger
            };
            score += trap_score;
        } else if ff.count >= future_len {
            let tail = me.body.last();
            let dist_to_tail = (my_head.x - tail.x).abs() + (my_head.y - tail.y).abs();

            if dist_to_tail <= 2 || escape_time <= 2 {
                score += cfg.scores.territory_control * 5;
            } else {
                score += cfg.scores.strategic_squeeze;
            }
        } else {
            score -= escape_time * cfg.scores.territory_control * 2;
        }
    } else if (voronoi.my_count as f64) < (grid.width * grid.height) as f64 * 0.2 {
        score += cfg.scores.tight_spot;
    }

    if !i_am_in_death_trap && voronoi.enemy_count < enemy.body.len() as i32 {
        let en_head = enemy.body.head();
        let en_len = enemy.body.len() as i32;
        let ff = flood_fill(grid, en_head.x, en_head.y, en_len + 2, Some(&enemy.body), Some(&me.body), buffers);
        let food_mod = if ff.has_food { 1 } else { 0 };
        let escape_time = ff.min_turns_to_clear.saturating_add(food_mod);
        let future_len = en_len + food_mod;

        if ff.count < future_len && ff.count < escape_time {
            let trap_score = if dense_tail_race {
                cfg.scores.enemy_trapped / 1000
            } else {
                cfg.scores.enemy_trapped
            };
            score += trap_score;
        } else if ff.count >= future_len {
            let tail = enemy.body.last();
            let dist_to_tail = (en_head.x - tail.x).abs() + (en_head.y - tail.y).abs();

            if dist_to_tail <= 2 || escape_time <= 2 {
                score -= cfg.scores.territory_control * 5;
            } else {
                score -= cfg.scores.strategic_squeeze;
            }
        } else {
            score += escape_time * cfg.scores.territory_control * 2;
        }
    }

    let dist_to_opp = manhattan(my_head, enemy_head);
    if dist_to_opp == 1 && me.body.len() > enemy.body.len() {
        score += cfg.scores.kill_pressure;
    }

    if grid.food.any() {
        let closest_dist = if let Some(map) = dist_map {
            map[(my_head.y * grid.width + my_head.x) as usize] as i32
        } else {
            let mut min_dist = 9999;
            let mut temp_food = grid.food;
            while let Some(idx) = temp_food.pop_first() {
                let fx = (idx as i32) % grid.width;
                let fy = (idx as i32) / grid.width;
                min_dist = min_dist.min(manhattan(my_head, Point { x: fx, y: fy }));
            }
            min_dist
        };

        if closest_dist > me.health {
            return cfg.scores.loss;
        }

        let buffer = me.health - closest_dist;
        let panic_value = if buffer > 0 {
            cfg.scores.food.intensity * (cfg.scores.food.threshold / (buffer as f64 + 1.0)).powf(cfg.scores.food.exponent)
        } else {
            cfg.scores.food.intensity * 100.0
        };
        score -= (closest_dist as f64 * panic_value) as i32;
    }

    if me.body.len() > enemy.body.len() + 1 {
        score -= dist_to_opp * cfg.scores.aggression;
    }

    score
}
