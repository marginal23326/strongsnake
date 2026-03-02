use crate::grid::Grid;

pub fn get_food_distance_map(grid: &Grid) -> Vec<i16> {
    let start = std::time::Instant::now();
    let res = get_food_distance_map_inner(grid);
    crate::PERF_STATS.with(|s| {
        let mut st = s.borrow_mut();
        st.distmap_calls += 1;
        st.distmap_duration += start.elapsed();
    });
    res
}

fn get_food_distance_map_inner(grid: &Grid) -> Vec<i16> {
    let size = (grid.width * grid.height) as usize;
    let mut dist_map = vec![1000i16; size];

    let mut current_front = grid.food;
    let mut visited = current_front;
    let safe_cells = grid.safe_cells();
    let mut dist = 0;

    let mut temp = current_front;
    while let Some(idx) = temp.pop_first() {
        dist_map[idx] = 0;
    }

    while current_front.any() {
        dist += 1;
        current_front = grid.ctx.expand_neighbors(current_front) & safe_cells & !visited;
        visited |= current_front;

        let mut temp = current_front;
        while let Some(idx) = temp.pop_first() {
            dist_map[idx] = dist;
        }
    }

    dist_map
}
