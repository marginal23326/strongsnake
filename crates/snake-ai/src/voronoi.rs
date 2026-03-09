use crate::bitboard::BitBoard;
use crate::grid::Grid;
use crate::model::SearchBuffers;
use snake_domain::Point;

#[derive(Debug, Clone)]
pub struct VoronoiResult {
    pub my_count: i32,
    pub enemy_count: i32,
}

pub fn compute_voronoi<const N: usize>(grid: &Grid<N>, my_head: Point, enemy_head: Point, _buffers: &mut SearchBuffers) -> VoronoiResult
where
    [(); (N + 63) / 64]: Sized,
{
    #[cfg(feature = "profiling")]
    {
        let start = std::time::Instant::now();
        let res = compute_voronoi_inner(grid, my_head, enemy_head);
        crate::PERF_STATS.with(|s| {
            let mut st = s.borrow_mut();
            st.voronoi_calls += 1;
            st.voronoi_duration += start.elapsed();
        });
        res
    }
    #[cfg(not(feature = "profiling"))]
    {
        compute_voronoi_inner(grid, my_head, enemy_head)
    }
}

fn compute_voronoi_inner<const N: usize>(grid: &Grid<N>, my_head: Point, enemy_head: Point) -> VoronoiResult
where
    [(); (N + 63) / 64]: Sized,
{
    let mut my_front = BitBoard::<N>::with_bit(grid.idx(my_head.x, my_head.y));
    let mut en_front = BitBoard::<N>::with_bit(grid.idx(enemy_head.x, enemy_head.y));

    let mut my_territory = BitBoard::<N>::empty();
    let mut en_territory = BitBoard::<N>::empty();

    let mut visited = my_front | en_front | !grid.safe_cells();
    let ctx = &grid.ctx;

    loop {
        let unvisited = !visited;
        my_front = ctx.expand_neighbors(my_front) & unvisited;
        en_front = ctx.expand_neighbors(en_front) & unvisited;

        let ties = my_front & en_front;

        my_front ^= ties;
        en_front ^= ties;

        let active = my_front | en_front;

        if active.is_empty() {
            break;
        }

        my_territory |= my_front;
        en_territory |= en_front;

        visited |= active | ties;
    }

    VoronoiResult {
        my_count: my_territory.count_ones() as i32,
        enemy_count: en_territory.count_ones() as i32,
    }
}
