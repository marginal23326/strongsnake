use crate::bitboard::BitBoard;
use crate::grid::Grid;
use crate::model::{AgentState, FastBody};
use snake_domain::Point;

#[derive(Debug, Clone)]
pub struct VoronoiResult {
    pub my_count: i32,
    pub enemy_count: i32,
}

pub fn compute_voronoi<const W: usize, const N: usize>(grid: &Grid<W, N>, my_head: Point, enemy_head: Point, my_len: usize, en_len: usize, me: &AgentState, enemy: &AgentState) -> VoronoiResult
{
    #[cfg(feature = "profiling")]
    {
        let start = std::time::Instant::now();
        let res = compute_voronoi_inner(grid, my_head, enemy_head, my_len, en_len, me, enemy);
        crate::PERF_STATS.with(|s| {
            let mut st = s.borrow_mut();
            st.voronoi_calls += 1;
            st.voronoi_duration += start.elapsed();
        });
        res
    }
    #[cfg(not(feature = "profiling"))]
    {
        compute_voronoi_inner(grid, my_head, enemy_head, my_len, en_len, me, enemy)
    }
}

/// Turns of body release the Voronoi expansion models; later segments stay walls.
const RELEASE_TURNS: usize = usize::MAX;

fn compute_voronoi_inner<const W: usize, const N: usize>(
    grid: &Grid<W, N>,
    my_head: Point,
    enemy_head: Point,
    my_len: usize,
    en_len: usize,
    me: &AgentState,
    enemy: &AgentState,
) -> VoronoiResult {
    let mut my_front = BitBoard::<W, N>::with_bit(grid.idx(my_head.x, my_head.y));
    let mut en_front = BitBoard::<W, N>::with_bit(grid.idx(enemy_head.x, enemy_head.y));

    let mut my_territory = BitBoard::<W, N>::empty();
    let mut en_territory = BitBoard::<W, N>::empty();
    // Every cell next to a cell the snake has reached so far (its head included): a cell that frees up there is its to take.
    let mut my_adj = BitBoard::<W, N>::empty();
    let mut en_adj = BitBoard::<W, N>::empty();

    let heads = my_front | en_front;
    let mut visited = heads | !grid.safe_cells();
    let ctx = &grid.ctx;

    // Bodies leave the board from the tail: the segment k cells from the tail can be entered on turn k + 1 (one turn
    // later for a snake that just ate and grows). The caller already cleared both tails for turn 1.
    let my_delay = (me.health >= 100) as usize;
    let en_delay = (enemy.health >= 100) as usize;
    let horizon = (my_len + my_delay).max(en_len + en_delay).min(RELEASE_TURNS);

    // The cell this snake frees on `turn`, if any (never its head).
    let freed_cell = |body: &FastBody, len: usize, delay: usize, turn: usize| -> Option<Point> {
        let k = (turn - 1).checked_sub(delay)?;
        if k + 2 > len {
            return None;
        }
        let idx = len - 1 - k;
        let cell = body.get(idx);
        // A fresh start stacks segments on one cell: the segment nearer the head frees it instead.
        if idx >= 2 && body.get(idx - 1) == cell {
            return None;
        }
        Some(cell)
    };

    let mut turn = 0usize;
    loop {
        turn += 1;

        let my_raw = if my_front.any() { ctx.expand_neighbors(my_front) } else { BitBoard::<W, N>::empty() };
        let en_raw = if en_front.any() { ctx.expand_neighbors(en_front) } else { BitBoard::<W, N>::empty() };
        my_adj |= my_raw;
        en_adj |= en_raw;

        // Cells that open up this turn: a snake that already touches one can take it.
        let mut my_free = BitBoard::<W, N>::empty();
        let mut en_free = BitBoard::<W, N>::empty();
        let freed = if turn <= RELEASE_TURNS {
            [freed_cell(&me.body, my_len, my_delay, turn), freed_cell(&enemy.body, en_len, en_delay, turn)]
        } else {
            [None, None]
        };
        for cell in freed.into_iter().flatten() {
            let i = grid.idx(cell.x, cell.y);
            if heads.get(i) {
                continue;
            }
            visited.unset(i);
            if my_adj.get(i) {
                my_free.set(i);
            }
            if en_adj.get(i) {
                en_free.set(i);
            }
        }

        let unvisited = !visited;
        my_front = (my_raw & unvisited) | my_free;
        en_front = (en_raw & unvisited) | en_free;

        let ties = my_front & en_front;

        my_front ^= ties;
        en_front ^= ties;
        // A contested cell goes to the longer snake, which survives the head-on and keeps expanding from it.
        // Equal lengths both die, so the cell stays nobody's.
        if my_len > en_len {
            my_front |= ties;
        } else if en_len > my_len {
            en_front |= ties;
        }
        visited |= ties;

        let active = my_front | en_front;

        if active.is_empty() {
            // Nothing to expand, but a body that is still leaving can open a way later.
            if turn >= horizon {
                break;
            }
            continue;
        }

        my_territory |= my_front;
        en_territory |= en_front;

        visited |= active;
    }

    VoronoiResult {
        my_count: my_territory.count_ones() as i32,
        enemy_count: en_territory.count_ones() as i32,
    }
}
