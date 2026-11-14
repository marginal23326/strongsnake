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

/// Walks one snake's body from the tail, a segment per turn: the segment k cells from the tail can be entered on turn
/// k + 1 (one turn later for a snake that just ate and grows). The caller already cleared both tails for turn 1.
struct Release {
    /// Turns still to wait before the first segment leaves.
    wait: usize,
    /// The next segment to leave, counted from the head. Never the head (0), so 0 means the body is used up.
    next: usize,
}

impl Release {
    fn new(len: usize, delay: usize) -> Self {
        Self {
            wait: delay,
            next: len.saturating_sub(1),
        }
    }

    /// The cell this snake frees on the next turn, if any.
    #[inline(always)]
    fn step(&mut self, body: &FastBody) -> Option<Point> {
        if self.wait > 0 {
            self.wait -= 1;
            return None;
        }
        if self.next == 0 {
            return None;
        }
        let idx = self.next;
        self.next -= 1;
        let cell = body.get(idx);
        // A fresh start stacks segments on one cell: the segment nearer the head frees it instead.
        if idx >= 2 && body.get(idx - 1) == cell {
            return None;
        }
        Some(cell)
    }
}

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
    // Both fronts start at the heads.
    let mut my_any = true;
    let mut en_any = true;

    let mut my_territory = BitBoard::<W, N>::empty();
    let mut en_territory = BitBoard::<W, N>::empty();
    // Every cell next to a cell the snake has reached so far (its head included): a cell that frees up there is its to take.
    let mut my_adj = BitBoard::<W, N>::empty();
    let mut en_adj = BitBoard::<W, N>::empty();

    let heads = my_front | en_front;
    // Cells still open to be claimed: not a wall, not a head, not claimed yet.
    let mut unvisited = grid.safe_cells() & !heads;
    let ctx = &grid.ctx;

    let my_delay = (me.health >= 100) as usize;
    let en_delay = (enemy.health >= 100) as usize;
    let horizon = (my_len + my_delay).max(en_len + en_delay).min(RELEASE_TURNS);
    let mut my_release = Release::new(my_len, my_delay);
    let mut en_release = Release::new(en_len, en_delay);

    let mut turn = 0usize;
    loop {
        turn += 1;

        // Raw bits may land off the board; every use below masks them (`unvisited` has none, and `adj` is only read at board cells).
        let my_raw = if my_any { ctx.expand_neighbors_unmasked(my_front) } else { BitBoard::<W, N>::empty() };
        let en_raw = if en_any { ctx.expand_neighbors_unmasked(en_front) } else { BitBoard::<W, N>::empty() };
        my_adj |= my_raw;
        en_adj |= en_raw;

        // Cells that open up this turn: a snake that already touches one can take it.
        let mut my_free = BitBoard::<W, N>::empty();
        let mut en_free = BitBoard::<W, N>::empty();
        if turn <= RELEASE_TURNS {
            let mut freed = BitBoard::<W, N>::empty();
            if let Some(cell) = my_release.step(&me.body) {
                freed |= BitBoard::with_bit(grid.idx(cell.x, cell.y));
            }
            if let Some(cell) = en_release.step(&enemy.body) {
                freed |= BitBoard::with_bit(grid.idx(cell.x, cell.y));
            }
            // Heads never free up.
            freed &= !heads;
            unvisited |= freed;
            my_free = freed & my_adj;
            en_free = freed & en_adj;
        }

        my_front = (my_raw & unvisited) | my_free;
        en_front = (en_raw & unvisited) | en_free;

        // A contested cell goes to the longer snake, which survives the head-on and keeps expanding from it.
        // Equal lengths both die, so the cell stays nobody's.
        if my_len > en_len {
            en_front &= !my_front;
        } else if en_len > my_len {
            my_front &= !en_front;
        } else {
            let ties = my_front & en_front;
            my_front ^= ties;
            en_front ^= ties;
            unvisited &= !ties;
        }

        my_any = my_front.any();
        en_any = en_front.any();
        if !my_any && !en_any {
            // Nothing to expand, but a body that is still leaving can open a way later.
            if turn >= horizon {
                break;
            }
            continue;
        }

        my_territory |= my_front;
        en_territory |= en_front;

        unvisited &= !(my_front | en_front);
    }

    VoronoiResult {
        my_count: my_territory.count_ones() as i32,
        enemy_count: en_territory.count_ones() as i32,
    }
}
