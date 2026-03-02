use crate::bitboard::BitBoard;
use crate::grid::Grid;
use crate::model::{FastBody, SearchBuffers};

#[derive(Debug, Clone, Copy)]
pub struct FloodFillResult {
    pub count: i32,
    pub min_turns_to_clear: i32,
    pub has_food: bool,
}

pub fn flood_fill(
    grid: &Grid,
    start_x: i32,
    start_y: i32,
    max_depth: i32,
    my_body: Option<&FastBody>,
    enemy_body: Option<&FastBody>,
    _buffers: &mut SearchBuffers,
) -> FloodFillResult {
    let start = std::time::Instant::now();
    let res = flood_fill_inner(grid, start_x, start_y, max_depth, my_body, enemy_body);
    crate::PERF_STATS.with(|s| {
        let mut st = s.borrow_mut();
        st.floodfill_calls += 1;
        st.floodfill_duration += start.elapsed();
    });
    res
}

fn flood_fill_inner(
    grid: &Grid,
    start_x: i32,
    start_y: i32,
    max_depth: i32,
    my_body: Option<&FastBody>,
    enemy_body: Option<&FastBody>,
) -> FloodFillResult {
    if start_x < 0 || start_y < 0 || start_x >= grid.width || start_y >= grid.height {
        return FloodFillResult {
            count: 0,
            min_turns_to_clear: i32::MAX,
            has_food: false,
        };
    }

    let mut front = BitBoard::with_bit(grid.idx(start_x, start_y));
    let mut visited = front;

    let safe_cells = grid.safe_cells();
    let food_cells = grid.food;

    let mut count = 1;
    let mut min_turns_to_clear = i32::MAX;
    let mut has_food = false;

    let mut vanish_map = [0i32; 448];

    let my_mask = my_body.map_or(BitBoard::empty(), |b| {
        let mut m = BitBoard::empty();
        let len = b.len() as i32;
        for (i, p) in b.iter().enumerate() {
            if p.x >= 0 && p.x < grid.width && p.y >= 0 && p.y < grid.height {
                let idx = grid.idx(p.x, p.y);
                m.set(idx);
                vanish_map[idx] = len - i as i32;
            }
        }
        m
    });

    let en_mask = enemy_body.map_or(BitBoard::empty(), |b| {
        let mut m = BitBoard::empty();
        let len = b.len() as i32;
        for (i, p) in b.iter().enumerate() {
            if p.x >= 0 && p.x < grid.width && p.y >= 0 && p.y < grid.height {
                let idx = grid.idx(p.x, p.y);
                m.set(idx);
                // If both snakes overlap a cell (e.g. tail traces), take the max vanish time
                vanish_map[idx] = vanish_map[idx].max(len - i as i32);
            }
        }
        m
    });

    for depth in 1..=max_depth {
        if !has_food && (visited & food_cells).any() {
            has_food = true;
        }

        let expanded_all = grid.ctx.expand_neighbors(front) & !visited;

        let mut hits = expanded_all & (my_mask | en_mask);
        while let Some(idx) = hits.pop_first() {
            let escape_time = depth.max(vanish_map[idx]);
            min_turns_to_clear = min_turns_to_clear.min(escape_time);
        }

        front = expanded_all & safe_cells;
        if front.is_empty() {
            break;
        }

        visited |= front;
        count += front.count_ones() as i32;
    }

    FloodFillResult {
        count,
        min_turns_to_clear,
        has_food,
    }
}
