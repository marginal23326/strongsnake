use snake_domain::{Direction, Point};

use crate::{
    config::AiConfig,
    grid::Grid,
    heuristics::evaluate,
    model::{AgentState, SearchBuffers},
    tt::{TranspositionTable, TtFlag},
    zobrist::Zobrist,
};

const QUIESCENCE_MAX_EXTENSIONS: usize = 8;
const SCORE_MIN: i32 = -2_000_000_000;

#[derive(Debug, Clone)]
pub struct PvLine {
    pub moves: [Direction; 64],
    pub len: usize,
}

impl Default for PvLine {
    fn default() -> Self {
        Self {
            moves: [Direction::Up; 64],
            len: 0,
        }
    }
}

impl PvLine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, dir: Direction) {
        if self.len < 64 {
            self.moves[self.len] = dir;
            self.len += 1;
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SearchMove {
    pub x: i32,
    pub y: i32,
    pub dir: Direction,
    pub dir_int: usize,
}

#[derive(Debug, Clone)]
pub struct RootChildRecord {
    pub mv: SearchMove,
    pub raw_recursion_score: i32,
    pub collision_penalty: i32,
    pub ate: bool,
    pub modified_score: i32,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: i32,
    pub mv: Option<SearchMove>,
    pub children: Vec<RootChildRecord>,
    pub pv: PvLine,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchFrame<'a> {
    pub dist_map: Option<&'a [i16]>,
    pub depth: usize,
    pub alpha: i32,
    pub beta: i32,
    pub side: usize,
    pub current_hash: u64,
    pub q_depth: usize,
}

pub struct SearchContext<'a> {
    pub root_depth: usize,
    pub history_table: &'a mut [Vec<i32>; 2],
    pub cfg: &'a AiConfig,
    pub tt: &'a TranspositionTable,
    pub zobrist: &'a Zobrist,
    pub buffers: &'a mut SearchBuffers,
    pub thread_id: usize,
}

struct MoveList {
    moves: [SearchMove; 4],
    count: usize,
}
impl MoveList {
    fn new() -> Self {
        Self {
            moves: [SearchMove::default(); 4],
            count: 0,
        }
    }
    fn push(&mut self, m: SearchMove) {
        self.moves[self.count] = m;
        self.count += 1;
    }
    fn as_mut_slice(&mut self) -> &mut [SearchMove] {
        &mut self.moves[0..self.count]
    }
}

fn get_safe_neighbors<const N: usize>(grid: &Grid<N>, me: &AgentState, enemy: &AgentState) -> MoveList
where
    [(); (N + 63) / 64]: Sized,
{
    let start = std::time::Instant::now();
    let res = get_safe_neighbors_inner(grid, me, enemy);
    crate::PERF_STATS.with(|s| {
        let mut st = s.borrow_mut();
        st.move_gen_calls += 1;
        st.move_gen_duration += start.elapsed();
    });
    res
}

fn get_safe_neighbors_inner<const N: usize>(grid: &Grid<N>, me: &AgentState, enemy: &AgentState) -> MoveList
where
    [(); (N + 63) / 64]: Sized,
{
    let mut list = MoveList::new();
    let my_body = &me.body;
    if my_body.is_empty() {
        return list;
    }
    let opp_body = &enemy.body;
    let head = my_body.head();

    let mut my_tail = None;
    let mut my_tail_stacked = false;
    if my_body.len() > 1 {
        let tail = my_body.last();
        let prev = my_body.get(my_body.len() - 2);
        my_tail = Some(tail);
        my_tail_stacked = tail == prev;
    }

    let mut opp_tail = None;
    let mut opp_tail_stacked = false;
    let mut enemy_can_eat = false;
    let occ = grid.occupied();
    let width = grid.width;
    let height = grid.height;
    let food = grid.food;

    if opp_body.len() > 1 {
        let tail = opp_body.last();
        let prev = opp_body.get(opp_body.len() - 2);
        opp_tail = Some(tail);
        opp_tail_stacked = tail == prev;

        let eh = opp_body.head();
        let eat_check = [(eh.x, eh.y), (eh.x, eh.y + 1), (eh.x, eh.y - 1), (eh.x - 1, eh.y), (eh.x + 1, eh.y)];
        for (x, y) in eat_check {
            if x >= 0 && y >= 0 && x < width && y < height {
                let idx = (y * width + x) as usize;
                if food.get(idx) {
                    enemy_can_eat = true;
                    break;
                }
            }
        }
    }

    let dirs = [
        (0, 1, Direction::Up, 0usize),
        (0, -1, Direction::Down, 1usize),
        (-1, 0, Direction::Left, 2usize),
        (1, 0, Direction::Right, 3usize),
    ];

    for (dx, dy, dir, dir_int) in dirs {
        let nx = head.x + dx;
        let ny = head.y + dy;
        let mut is_safe = false;
        if nx >= 0 && ny >= 0 && nx < width && ny < height {
            let idx = (ny * width + nx) as usize;
            is_safe = !occ.get(idx);
        }

        if !is_safe {
            if let Some(my_tail_pos) = my_tail
                && nx == my_tail_pos.x
                && ny == my_tail_pos.y
                && !my_tail_stacked
            {
                is_safe = true;
            }
            if !is_safe
                && let Some(opp_tail_pos) = opp_tail
                && nx == opp_tail_pos.x
                && ny == opp_tail_pos.y
                && !opp_tail_stacked
                && !enemy_can_eat
            {
                is_safe = true;
            }
        }

        if is_safe {
            list.push(SearchMove {
                x: nx,
                y: ny,
                dir,
                dir_int,
            });
        }
    }

    list
}

fn root_tie_breaker(me: &AgentState, enemy: &AgentState, cols: i32, rows: i32, mv: SearchMove) -> i32 {
    if me.body.is_empty() {
        return 0;
    }

    let my_len = me.body.len();
    let enemy_len = enemy.body.len();

    let mut move_into_enemy_tail_penalty = 0;
    if !enemy.body.is_empty() {
        let enemy_tail = enemy.body.last();
        if mv.x == enemy_tail.x && mv.y == enemy_tail.y {
            move_into_enemy_tail_penalty = 5;
        }
    }

    let mut head_contact_bias = 0;
    if !enemy.body.is_empty() {
        let enemy_head = enemy.body.head();
        let dist = (mv.x - enemy_head.x).abs() + (mv.y - enemy_head.y).abs();
        if dist == 1 {
            if my_len > enemy_len {
                head_contact_bias += 20;
            } else if my_len < enemy_len {
                head_contact_bias -= 10000;
            } else {
                head_contact_bias -= 5000;
            }
        }
    }

    let mut tail_bias = 0;
    if cols > 0 && rows > 0 && my_len >= 20 && enemy_len >= 20 {
        let total_len = my_len + enemy_len;
        let total_area = (cols * rows) as usize;
        if (total_len * 100) / total_area >= 40 {
            let my_tail = me.body.last();
            let tail_dist = (mv.x - my_tail.x).abs() + (mv.y - my_tail.y).abs();
            tail_bias = -(tail_dist * 10);
        }
    }

    tail_bias + head_contact_bias - move_into_enemy_tail_penalty
}

fn should_extend_leaf<const N: usize>(grid: &Grid<N>, me: &AgentState, enemy: &AgentState, cfg: &AiConfig) -> bool
where
    [(); (N + 63) / 64]: Sized,
{
    let my_len = me.body.len();
    let enemy_len = enemy.body.len();
    if my_len < 20 || enemy_len < 20 {
        return false;
    }

    let total_len = my_len + enemy_len;
    let total_area = (grid.width * grid.height) as usize;
    if (total_len * 100) / total_area < cfg.dense_tail_race_occupancy {
        return false;
    }

    let my_moves = get_safe_neighbors(grid, me, enemy).count;
    if my_moves <= 2 {
        return true;
    }

    get_safe_neighbors(grid, enemy, me).count <= 2
}

pub fn negamax<const N: usize>(
    grid: &mut Grid<N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    frame: SearchFrame<'_>,
    ctx: &mut SearchContext<'_>,
) -> SearchResult
where
    [(); (N + 63) / 64]: Sized,
{
    let SearchFrame {
        dist_map,
        depth,
        mut alpha,
        mut beta,
        side,
        current_hash,
        q_depth,
    } = frame;

    crate::PERF_STATS.with(|s| s.borrow_mut().negamax_calls += 1);

    let original_alpha = alpha;
    let tt_entry = ctx.tt.get(current_hash);

    if depth != ctx.root_depth
        && let Some(entry) = tt_entry
        && (entry.depth as usize) >= depth
    {
        match entry.flag {
            TtFlag::Exact => {
                let mut pv = PvLine::new();
                if let Some(d) = entry.get_direction() {
                    pv.push(d);
                }
                return SearchResult {
                    score: entry.score,
                    mv: None,
                    children: Vec::new(),
                    pv,
                };
            }
            TtFlag::LowerBound => alpha = alpha.max(entry.score),
            TtFlag::UpperBound => beta = beta.min(entry.score),
        }
        if alpha >= beta {
            let mut pv = PvLine::new();
            if let Some(d) = entry.get_direction() {
                pv.push(d);
            }
            return SearchResult {
                score: entry.score,
                mv: None,
                children: Vec::new(),
                pv,
            };
        }
    }

    if me.body.is_empty() || me.health <= 0 {
        return SearchResult {
            score: ctx.cfg.scores.loss - depth as i32,
            mv: None,
            children: Vec::new(),
            pv: PvLine::new(),
        };
    }
    if enemy.body.is_empty() || enemy.health <= 0 {
        return SearchResult {
            score: ctx.cfg.scores.win + depth as i32,
            mv: None,
            children: Vec::new(),
            pv: PvLine::new(),
        };
    }

    if depth == 0 {
        if ctx.root_depth >= 3 && q_depth < QUIESCENCE_MAX_EXTENSIONS && should_extend_leaf(grid, me, enemy, ctx.cfg) {
            return negamax(
                grid,
                me,
                enemy,
                SearchFrame {
                    dist_map,
                    depth: 1,
                    alpha,
                    beta,
                    side,
                    current_hash,
                    q_depth: q_depth + 1,
                },
                ctx,
            );
        }

        let score = if side == 0 {
            evaluate(grid, me, enemy, dist_map, ctx.cfg, ctx.buffers)
        } else {
            -evaluate(grid, enemy, me, dist_map, ctx.cfg, ctx.buffers)
        };

        ctx.tt.set(current_hash, 0, score, TtFlag::Exact, 255, 255, 255);

        return SearchResult {
            score,
            mv: None,
            children: Vec::new(),
            pv: PvLine::new(),
        };
    }

    let is_root = ctx.root_depth == depth && side == 0;
    let head = me.body.head();
    let head_idx = (head.y * grid.width + head.x) as usize;
    let mut move_list = get_safe_neighbors(grid, me, enemy);

    if move_list.count == 0 {
        return SearchResult {
            score: ctx.cfg.scores.loss - depth as i32,
            mv: None,
            children: Vec::new(),
            pv: PvLine::new(),
        };
    }

    let moves = move_list.as_mut_slice();
    let (pv_x, pv_y) = if let Some(e) = tt_entry {
        if e.mv_x != 255 {
            (Some(e.mv_x as i32), Some(e.mv_y as i32))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    if moves.len() > 1 {
        let history_table = &*ctx.history_table;
        moves.sort_unstable_by(|a, b| {
            if let (Some(px), Some(py)) = (pv_x, pv_y) {
                if a.x == px && a.y == py {
                    return std::cmp::Ordering::Less;
                }
                if b.x == px && b.y == py {
                    return std::cmp::Ordering::Greater;
                }
            }

            let hist_a = history_table[side][head_idx * 4 + a.dir_int];
            let hist_b = history_table[side][head_idx * 4 + b.dir_int];
            if hist_a != hist_b {
                return hist_b.cmp(&hist_a);
            }

            let mut min_a = 1000;
            let mut min_b = 1000;
            
            let mut temp_food = grid.food;
            while let Some(idx) = temp_food.pop_first() {
                let fx = (idx as i32) % grid.width;
                let fy = (idx as i32) / grid.width;
                let da = (a.x - fx).abs() + (a.y - fy).abs();
                let db = (b.x - fx).abs() + (b.y - fy).abs();
                min_a = min_a.min(da);
                min_b = min_b.min(db);
            }

            if min_a == min_b {
                let cx = grid.width as f64 / 2.0;
                let cy = grid.height as f64 / 2.0;
                let ca = (a.x as f64 - cx).abs() + (a.y as f64 - cy).abs();
                let cb = (b.x as f64 - cx).abs() + (b.y as f64 - cy).abs();
                return ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal);
            }
            min_a.cmp(&min_b)
        });

        if is_root && ctx.thread_id > 0 {
            let shift = ctx.thread_id % moves.len();
            moves.rotate_left(shift);
        }
    }

    let mut child_records = Vec::new();
    let mut best_move = moves[0];
    let mut best_score = SCORE_MIN;
    let mut best_tie_break = SCORE_MIN;
    let mut best_pv = PvLine::new();

    for (i, &mv) in moves.iter().enumerate() {
        let mut collision_penalty = 0;
        let mut kill_threat_bonus = 0;
        if side == 0 && !enemy.body.is_empty() {
            let opp_head = enemy.body.head();
            let dist = (mv.x - opp_head.x).abs() + (mv.y - opp_head.y).abs();
            if dist == 1 {
                let my_len = me.body.len();
                let opp_len = enemy.body.len();
                if opp_len > my_len {
                    collision_penalty = ctx.cfg.scores.head_on_collision as i32;
                } else if opp_len == my_len {
                    collision_penalty = ctx.cfg.scores.draw as i32;
                } else {
                    kill_threat_bonus = ctx.cfg.scores.kill_pressure as i32;
                }
            }
        }

        let tie_break = if is_root {
            root_tie_breaker(me, enemy, grid.width, grid.height, mv)
        } else {
            0
        };

        let original_head_val = grid.get(mv.x, mv.y);
        let ate_food = original_head_val == 1;

        let mut tail_restore: Option<(i32, i32, i8)> = None;
        let mut popped_tail = None;

        let mut next_hash = current_hash;
        let old_health = me.health;
        let new_health = if ate_food { 100 } else { old_health - 1 };

        me.health = new_health;
        me.body.push_front(Point { x: mv.x, y: mv.y });

        let cell_id: i8 = 2 + side as i8;

        next_hash = ctx.zobrist.xor_health(next_hash, old_health, new_health, side == 0);
        if original_head_val != 0 {
            unsafe {
                next_hash = ctx.zobrist.xor_unchecked(next_hash, mv.x, mv.y, original_head_val);
            }
        }
        unsafe {
            next_hash = ctx.zobrist.xor_unchecked(next_hash, mv.x, mv.y, cell_id);
        }

        if !ate_food {
            let tail = me.body.pop_back();
            popped_tail = Some(tail);

            if tail.x != mv.x || tail.y != mv.y {
                let original_tail_val = grid.get(tail.x, tail.y);
                if original_tail_val == cell_id {
                    unsafe {
                        grid.clear_unchecked(tail.x, tail.y, original_tail_val);
                    }
                    tail_restore = Some((tail.x, tail.y, original_tail_val));
                }
                unsafe {
                    next_hash = ctx.zobrist.xor_unchecked(next_hash, tail.x, tail.y, cell_id);
                }
            }
        }

        unsafe {
            grid.replace_unchecked(mv.x, mv.y, original_head_val, cell_id);
        }

        // Evaluate root bonuses while the move is applied and board is accurately updated
        let mut root_bonus = 0;
        if is_root {
            let continuation_moves = get_safe_neighbors(grid, me, enemy).count;
            if continuation_moves == 0 {
                root_bonus += ctx.cfg.scores.trap_danger as i32;
            }

            let my_len = me.body.len();
            let enemy_len = enemy.body.len();
            let total_len = my_len + enemy_len;
            let total_area = (grid.width * grid.height) as usize;
            let dense_tail_race = my_len >= 20 && enemy_len >= 20 && (total_len * 100) / total_area >= ctx.cfg.dense_tail_race_occupancy;

            if dense_tail_race && !enemy.body.is_empty() {
                let enemy_head = enemy.body.head();
                let enemy_head_dist = (mv.x - enemy_head.x).abs() + (mv.y - enemy_head.y).abs();
                if continuation_moves == 1 && enemy_head_dist <= 5 {
                    root_bonus -= ctx.cfg.scores.territory_control.abs() * 120;
                }
                let enemy_tail = enemy.body.last();
                if mv.x == enemy_tail.x && mv.y == enemy_tail.y {
                    root_bonus -= ctx.cfg.scores.territory_control.abs() * 2;
                }
            }
        }

        let calc_mod_score = |c_score: i32| -> i32 {
            let mut ms = -c_score;
            if collision_penalty < 0 {
                ms = ms.min(collision_penalty);
            }
            let terminal_band = ms.abs() >= (ctx.cfg.scores.win.abs() / 10) * 9;
            if !terminal_band {
                if kill_threat_bonus > 0 {
                    ms = ms.saturating_add(kill_threat_bonus);
                }
                if ate_food && ms > -50_000_000 {
                    ms = ms.saturating_add(ctx.cfg.scores.eat_reward as i32);
                }
            }
            ms.saturating_add(root_bonus)
        };

        let mut child_score = 0;
        let mut child_pv = PvLine::new();

        if i == 0 {
            // PV Move
            let child = negamax(
                grid,
                enemy,
                me,
                SearchFrame {
                    dist_map: None,
                    depth: depth - 1,
                    alpha: -beta,
                    beta: -alpha,
                    side: 1 - side,
                    current_hash: next_hash,
                    q_depth,
                },
                ctx,
            );
            child_score = child.score;
            child_pv = child.pv;
        } else {
            let mut needs_full_search = true;

            // LMR
            if depth >= 5 && !ate_food && collision_penalty == 0 && kill_threat_bonus == 0 {
                let r = 1;

                let child_lmr = negamax(
                    grid,
                    enemy,
                    me,
                    SearchFrame {
                        dist_map: None,
                        depth: depth - 1 - r,
                        alpha: -alpha - 1,
                        beta: -alpha,
                        side: 1 - side,
                        current_hash: next_hash,
                        q_depth,
                    },
                    ctx,
                );

                let temp_mod_lmr = calc_mod_score(child_lmr.score);

                let is_massive_win = temp_mod_lmr > 50_000_000;

                if temp_mod_lmr < alpha && !is_massive_win {
                    child_score = child_lmr.score;
                    child_pv = child_lmr.pv;
                    needs_full_search = false;
                }
            }

            // PVS ZERO-WINDOW SEARCH
            if needs_full_search {
                let child = negamax(
                    grid,
                    enemy,
                    me,
                    SearchFrame {
                        dist_map: None,
                        depth: depth - 1,
                        alpha: -alpha - 1,
                        beta: -alpha,
                        side: 1 - side,
                        current_hash: next_hash,
                        q_depth,
                    },
                    ctx,
                );
                child_score = child.score;
                child_pv = child.pv;

                let temp_mod = calc_mod_score(child_score);
                if temp_mod > alpha && temp_mod < beta {
                    let child_re = negamax(
                        grid,
                        enemy,
                        me,
                        SearchFrame {
                            dist_map: None,
                            depth: depth - 1,
                            alpha: -beta,
                            beta: -alpha,
                            side: 1 - side,
                            current_hash: next_hash,
                            q_depth,
                        },
                        ctx,
                    );
                    child_score = child_re.score;
                    child_pv = child_re.pv;
                }
            }
        }

        let modified_score = calc_mod_score(child_score);

        unsafe {
            grid.replace_unchecked(mv.x, mv.y, cell_id, original_head_val);
        }
        if let Some((tx, ty, tv)) = tail_restore {
            unsafe {
                grid.replace_unchecked(tx, ty, 0, tv);
            }
        }

        me.body.pop_front();
        if let Some(tail) = popped_tail {
            me.body.push_back(tail);
        }
        me.health = old_health;

        if is_root {
            child_records.push(RootChildRecord {
                mv,
                raw_recursion_score: child_score,
                collision_penalty,
                ate: ate_food,
                modified_score,
            });
        }

        if modified_score > best_score || (modified_score == best_score && tie_break > best_tie_break) {
            best_score = modified_score;
            best_move = mv;
            best_tie_break = tie_break;
            best_pv = child_pv;
        }
        if best_score > alpha {
            alpha = best_score;
        }
        if alpha >= beta {
            break;
        }
    }

    ctx.history_table[side][head_idx * 4 + best_move.dir_int] += (depth * depth) as i32;
    let tt_flag = if best_score <= original_alpha {
        TtFlag::UpperBound
    } else if best_score >= beta {
        TtFlag::LowerBound
    } else {
        TtFlag::Exact
    };

    let dir_byte = match best_move.dir {
        Direction::Up => 0,
        Direction::Down => 1,
        Direction::Left => 2,
        Direction::Right => 3,
    };

    ctx.tt.set(
        current_hash,
        depth,
        best_score,
        tt_flag,
        best_move.x as u8,
        best_move.y as u8,
        dir_byte,
    );

    let mut final_pv = PvLine::new();
    final_pv.push(best_move.dir);
    for i in 0..best_pv.len {
        final_pv.push(best_pv.moves[i]);
    }

    SearchResult {
        score: best_score,
        mv: Some(best_move),
        children: child_records,
        pv: final_pv,
    }
}
