use std::cmp::Reverse;

use snake_domain::{Direction, Point};

use crate::{
    config::AiConfig,
    grid::Grid,
    heuristics::{evaluate, is_dense_tail_race},
    model::{AgentState, FastBody},
    tt::{TT_FLAG_EXACT, TT_FLAG_LOWER, TT_FLAG_UPPER, TranspositionTable, TtEntry},
    zobrist::Zobrist,
};

const QUIESCENCE_MAX_EXTENSIONS: usize = 5;
const SCORE_MIN: i32 = -2_000_000_000;
const SCORE_MAX: i32 = 2_000_000_000;
const PV_MAX_TURNS: usize = 32;


const DIRECTIONS: [(i32, i32, Direction, usize); 4] = [
    (0, 1, Direction::Up, 0),
    (0, -1, Direction::Down, 1),
    (-1, 0, Direction::Left, 2),
    (1, 0, Direction::Right, 3),
];

#[derive(Debug, Clone, Copy, Default)]
pub struct SearchMove {
    pub x: i32,
    pub y: i32,
    pub dir: Direction,
    pub dir_int: usize,
}

impl SearchMove {
    #[inline(always)]
    fn cell(self) -> Point {
        Point { x: self.x, y: self.y }
    }
}

#[derive(Debug, Clone)]
pub struct RootChildRecord {
    pub mv: SearchMove,
    pub score: i32,
    pub ate: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: i32,
    pub mv: Option<SearchMove>,
    pub children: Vec<RootChildRecord>,
}

impl SearchResult {
    fn leaf(score: i32) -> Self {
        Self {
            score,
            mv: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SearchFrame {
    /// Remaining depth, in turns.
    pub depth: usize,
    pub alpha: i32,
    pub beta: i32,
    pub current_hash: u64,
    pub q_depth: usize,
}

pub struct SearchContext<'a> {
    /// Depth, in turns, of the iteration being searched. `search_root` advances it.
    pub root_depth: usize,
    pub collect_root_children: bool,
    pub history_table: &'a mut [Vec<i32>; 2],
    pub cfg: &'a AiConfig,
    pub tt: &'a TranspositionTable,
    pub zobrist: &'a Zobrist,
    pub thread_id: usize,
    pub nodes_searched: u64,
}

impl SearchContext<'_> {
    /// An entry from an earlier iteration that spent more quiescence extensions on its way to the position than
    /// this search has spent only saw the position with a smaller extension budget, so its score looks less far
    /// ahead than a fresh search would. Its moves still seed the move ordering; only its score is refused.
    #[inline]
    fn score_is_reusable(&self, entry: &TtEntry, depth: usize, q_depth: usize) -> bool {
        (entry.depth as usize) >= depth && !(entry.iteration < self.root_depth.min(255) as u8 && entry.q_used as usize > q_depth)
    }
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
    fn as_slice(&self) -> &[SearchMove] {
        &self.moves[..self.count]
    }
    fn as_mut_slice(&mut self) -> &mut [SearchMove] {
        &mut self.moves[..self.count]
    }

    /// Stable sort on a key computed once per move (there are at most four moves, so no allocation).
    fn sort_by_cached_key<K: Ord + Copy>(&mut self, mut key: impl FnMut(&SearchMove) -> K) {
        let mut keys: [Option<K>; 4] = [None; 4];
        for (slot, mv) in keys.iter_mut().zip(self.as_slice()) {
            *slot = Some(key(mv));
        }
        for i in 1..self.count {
            let mut j = i;
            while j > 0 && keys[j - 1] > keys[j] {
                keys.swap(j - 1, j);
                self.moves.swap(j - 1, j);
                j -= 1;
            }
        }
    }
}

fn tail_cell_if_freed(body: &FastBody) -> Option<Point> {
    let len = body.len();
    if len == 0 {
        return None;
    }
    let tail = body.last();
    (len == 1 || body.get(len - 2) != tail).then_some(tail)
}

fn get_safe_neighbors<const W: usize, const N: usize>(grid: &Grid<W, N>, me: &AgentState, enemy: &AgentState) -> MoveList
{
    #[cfg(feature = "profiling")]
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
    #[cfg(not(feature = "profiling"))]
    {
        get_safe_neighbors_inner(grid, me, enemy)
    }
}

fn get_safe_neighbors_inner<const W: usize, const N: usize>(grid: &Grid<W, N>, me: &AgentState, enemy: &AgentState) -> MoveList
{
    let mut list = MoveList::new();
    if me.body.is_empty() {
        return list;
    }
    let head = me.body.head();
    let occupied = grid.occupied();
    let freed_tails = [tail_cell_if_freed(&me.body), tail_cell_if_freed(&enemy.body)];

    for (dx, dy, dir, dir_int) in DIRECTIONS {
        let (x, y) = (head.x + dx, head.y + dy);
        if x < 0 || y < 0 || x >= grid.width || y >= grid.height {
            continue;
        }
        if occupied.get(grid.idx(x, y)) && !freed_tails.contains(&Some(Point { x, y })) {
            continue;
        }
        list.push(SearchMove { x, y, dir, dir_int });
    }

    list
}

fn manhattan(a: Point, b: Point) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn should_extend_leaf<const W: usize, const N: usize>(grid: &Grid<W, N>, me: &AgentState, enemy: &AgentState, cfg: &AiConfig) -> bool
{
    let area = (grid.width * grid.height) as usize;
    if !is_dense_tail_race(me.body.len(), enemy.body.len(), area, cfg) {
        return false;
    }

    let my_moves = get_safe_neighbors(grid, me, enemy).count;
    if my_moves <= 2 {
        return true;
    }

    get_safe_neighbors(grid, enemy, me).count <= 2
}

#[inline(always)]
fn direction_index(dir: Direction) -> usize {
    match dir {
        Direction::Up => 0,
        Direction::Down => 1,
        Direction::Left => 2,
        Direction::Right => 3,
    }
}

#[derive(Clone, Copy)]
struct MoveUndo {
    x: i32,
    y: i32,
    cell_id: i8,
    original_head_val: i8,
    old_health: i32,
    popped_tail: Option<Point>,
    tail_restore: Option<(i32, i32, i8)>,
}

#[inline(always)]
fn make_move<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    side: usize,
    mv: SearchMove,
    zobrist: &Zobrist,
    current_hash: u64,
) -> (u64, MoveUndo)
{
    let original_head_val = grid.get(mv.x, mv.y);
    let ate_food = original_head_val == 1;
    let old_health = me.health;
    let ate_food_i32 = ate_food as i32;
    let new_health = (old_health - 1) * (1 - ate_food_i32) + (100 * ate_food_i32);
    let cell_id: i8 = 2 + side as i8;

    let mut next_hash = current_hash
        ^ zobrist.health[side][old_health.clamp(0, 100) as usize]
        ^ zobrist.health[side][new_health.clamp(0, 100) as usize]
        ^ zobrist.ends(side, &me.body);

    me.health = new_health;
    me.body.push_front(Point { x: mv.x, y: mv.y });

    if original_head_val != 0 {
        unsafe {
            next_hash = zobrist.xor_unchecked(next_hash, mv.x, mv.y, original_head_val);
        }
    }
    unsafe {
        next_hash = zobrist.xor_unchecked(next_hash, mv.x, mv.y, cell_id);
    }

    let mut popped_tail = None;
    let mut tail_restore = None;
    if !ate_food {
        let tail = me.body.pop_back();
        popped_tail = Some(tail);

        if tail.x != mv.x || tail.y != mv.y {
            let original_tail_val = grid.get(tail.x, tail.y);
            if original_tail_val == cell_id {
                unsafe {
                    grid.clear_unchecked(tail.x, tail.y, original_tail_val);
                    next_hash = zobrist.xor_unchecked(next_hash, tail.x, tail.y, cell_id);
                }
                tail_restore = Some((tail.x, tail.y, original_tail_val));
            }
        }
    }

    unsafe {
        grid.replace_unchecked(mv.x, mv.y, original_head_val, cell_id);
    }
    next_hash ^= zobrist.ends(side, &me.body);

    (
        next_hash,
        MoveUndo {
            x: mv.x,
            y: mv.y,
            cell_id,
            original_head_val,
            old_health,
            popped_tail,
            tail_restore,
        },
    )
}

#[inline(always)]
fn unmake_move<const W: usize, const N: usize>(grid: &mut Grid<W, N>, me: &mut AgentState, undo: MoveUndo) {
    unsafe {
        grid.replace_unchecked(undo.x, undo.y, undo.cell_id, undo.original_head_val);
    }
    if let Some((tx, ty, tv)) = undo.tail_restore {
        unsafe {
            grid.replace_unchecked(tx, ty, 0, tv);
        }
    }

    me.body.pop_front();
    if let Some(tail) = undo.popped_tail {
        me.body.push_back(tail);
    }
    me.health = undo.old_health;
}

fn starves<const W: usize, const N: usize>(grid: &Grid<W, N>, health: i32, mv: SearchMove) -> bool {
    health <= 1 && !grid.food.get(grid.idx(mv.x, mv.y))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnOutcome {
    Continue,
    Win,
    Loss,
    Draw,
}

fn resolve_turn<const W: usize, const N: usize>(
    grid: &Grid<W, N>,
    me: &AgentState,
    enemy: &AgentState,
    my_mv: SearchMove,
    en_mv: SearchMove,
) -> TurnOutcome
{
    let my_eats = grid.food.get(grid.idx(my_mv.x, my_mv.y));
    let en_eats = grid.food.get(grid.idx(en_mv.x, en_mv.y));

    let mut me_dead = starves(grid, me.health, my_mv);
    let mut en_dead = starves(grid, enemy.health, en_mv);

    if my_mv.cell() == en_mv.cell() {
        // Head-to-head: the longer snake (counting this turn's growth) survives, equal lengths both die.
        let my_len = me.body.len() + my_eats as usize;
        let en_len = enemy.body.len() + en_eats as usize;
        me_dead |= my_len <= en_len;
        en_dead |= en_len <= my_len;
    } else {
        // A snake that eats keeps its tail, so stepping onto it is a body collision.
        me_dead |= en_eats && my_mv.cell() == enemy.body.last();
        en_dead |= my_eats && en_mv.cell() == me.body.last();
    }

    match (me_dead, en_dead) {
        (false, false) => TurnOutcome::Continue,
        (false, true) => TurnOutcome::Win,
        (true, false) => TurnOutcome::Loss,
        (true, true) => TurnOutcome::Draw,
    }
}

fn outcome_score(outcome: TurnOutcome, depth: usize, cfg: &AiConfig) -> Option<i32> {
    match outcome {
        TurnOutcome::Continue => None,
        TurnOutcome::Win => Some(cfg.scores.win + depth as i32),
        TurnOutcome::Loss => Some(cfg.scores.loss - depth as i32),
        TurnOutcome::Draw => Some(cfg.scores.draw),
    }
}

struct TurnUndo {
    me: MoveUndo,
    enemy: MoveUndo,
}

fn make_turn<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    my_mv: SearchMove,
    en_mv: SearchMove,
    zobrist: &Zobrist,
    current_hash: u64,
) -> (u64, TurnUndo)
{
    let (hash, my_undo) = make_move(grid, me, 0, my_mv, zobrist, current_hash);
    let (hash, en_undo) = make_move(grid, enemy, 1, en_mv, zobrist, hash);
    (
        hash,
        TurnUndo {
            me: my_undo,
            enemy: en_undo,
        },
    )
}

fn unmake_turn<const W: usize, const N: usize>(grid: &mut Grid<W, N>, me: &mut AgentState, enemy: &mut AgentState, undo: TurnUndo) {
    unmake_move(grid, enemy, undo.enemy);
    unmake_move(grid, me, undo.me);
}

#[allow(clippy::too_many_arguments)]
pub fn reconstruct_pv<const W: usize, const N: usize>(
    mut grid: Grid<W, N>,
    mut me: AgentState,
    mut enemy: AgentState,
    root_hash: u64,
    root_move: SearchMove,
    max_depth: usize,
    tt: &TranspositionTable,
    zobrist: &Zobrist,
) -> Vec<Direction>
{
    // Alternates my move and the enemy's reply, one pair per turn.
    let mut pv = Vec::new();
    let mut hash = root_hash;

    for turn in 0..max_depth.min(PV_MAX_TURNS) {
        let remaining = max_depth - turn;
        let entry = tt
            .get(hash)
            .filter(|e| e.flag == TT_FLAG_EXACT && (e.depth as usize) >= remaining);

        // The root move is already decided; later turns follow the table.
        let my_dir = if turn == 0 {
            root_move.dir
        } else if let Some(dir) = entry.and_then(|e| e.get_direction()) {
            dir
        } else {
            break;
        };
        pv.push(my_dir);

        let Some(reply_dir) = entry
            .filter(|e| e.get_direction() == Some(my_dir))
            .and_then(|e| e.get_reply_direction())
        else {
            break;
        };
        pv.push(reply_dir);

        let my_moves = get_safe_neighbors(&grid, &me, &enemy);
        let en_moves = get_safe_neighbors(&grid, &enemy, &me);
        let (Some(&my_mv), Some(&en_mv)) = (
            my_moves.as_slice().iter().find(|m| m.dir == my_dir),
            en_moves.as_slice().iter().find(|m| m.dir == reply_dir),
        ) else {
            break;
        };
        if resolve_turn(&grid, &me, &enemy, my_mv, en_mv) != TurnOutcome::Continue {
            break;
        }
        hash = make_turn(&mut grid, &mut me, &mut enemy, my_mv, en_mv, zobrist, hash).0;
    }

    pv
}

fn order_moves<const W: usize, const N: usize>(moves: &mut MoveList, grid: &Grid<W, N>, head: Point, history: &[i32], hint: Option<Direction>) {
    let head_idx = grid.idx(head.x, head.y);
    // One pass over the food serves every move (the index to coordinate division is the expensive part), kept by direction.
    let mut food_dists = [1000; 4];
    let mut food = grid.food;
    while let Some(idx) = food.pop_first() {
        let food_cell = Point {
            x: (idx as i32) % grid.width,
            y: (idx as i32) / grid.width,
        };
        for m in moves.as_slice() {
            let d = &mut food_dists[m.dir_int];
            *d = (*d).min(manhattan(m.cell(), food_cell));
        }
    }
    moves.sort_by_cached_key(|m| {
        let food_dist = food_dists[m.dir_int];
        let center_dist = (2 * m.x - grid.width).abs() + (2 * m.y - grid.height).abs();
        (Some(m.dir) != hint, Reverse(history[head_idx * 4 + m.dir_int]), food_dist, center_dist)
    });
}

fn eat_bonus(my_ate: bool, en_ate: bool, cfg: &AiConfig) -> i32 {
    (my_ate as i32 - en_ate as i32) * cfg.scores.eat_reward
}

fn is_decided(score: i32, cfg: &AiConfig) -> bool {
    score.abs() >= cfg.scores.win.abs() / 10 * 9
}

fn apply_bonus(score: i32, bonus: i32, cfg: &AiConfig) -> i32 {
    if is_decided(score, cfg) { score } else { score.saturating_add(bonus) }
}

fn remove_bonus(bound: i32, bonus: i32, cfg: &AiConfig) -> i32 {
    if is_decided(bound, cfg) { bound } else { bound.saturating_sub(bonus) }
}

struct ReplySearch<'a> {
    my_mv: SearchMove,
    replies: &'a [SearchMove],
    /// Remaining depth of the node I am moving from (decides how much a decided turn is worth).
    node_depth: usize,
    alpha: i32,
    beta: i32,
    hash: u64,
    q_depth: usize,
}

fn worst_case_reply<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    query: &ReplySearch<'_>,
    ctx: &mut SearchContext<'_>,
) -> (i32, Direction)
{
    let cfg = ctx.cfg;
    let my_ate = grid.food.get(grid.idx(query.my_mv.x, query.my_mv.y));
    let mut beta = query.beta;
    let mut worst = SCORE_MAX;
    let mut worst_reply = query.replies[0].dir;

    for &reply in query.replies {
        let outcome = resolve_turn(grid, me, enemy, query.my_mv, reply);
        let score = match outcome_score(outcome, query.node_depth, cfg) {
            Some(score) => score,
            None => {
                let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), cfg);
                let (hash, undo) = make_turn(grid, me, enemy, query.my_mv, reply, ctx.zobrist, query.hash);
                let child = search(
                    grid,
                    me,
                    enemy,
                    SearchFrame {
                        depth: query.node_depth - 1,
                        alpha: remove_bonus(query.alpha, bonus, cfg),
                        beta: remove_bonus(beta, bonus, cfg),
                        current_hash: hash,
                        q_depth: query.q_depth,
                    },
                    ctx,
                );
                unmake_turn(grid, me, enemy, undo);
                apply_bonus(child.score, bonus, cfg)
            }
        };

        if score < worst {
            worst = score;
            worst_reply = reply.dir;
        }
        if worst <= query.alpha {
            break;
        }
        beta = beta.min(worst);
    }

    (worst, worst_reply)
}

fn is_proven_loss(score: i32, cfg: &AiConfig) -> bool {
    score < 0 && is_decided(score, cfg)
}

pub fn prefer_hopeful_move<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    root_hash: u64,
    root: &SearchResult,
    ctx: &mut SearchContext<'_>,
) -> Option<SearchMove>
{
    let cfg = ctx.cfg;
    let depth = ctx.root_depth;
    let preferred = root.mv?;
    if depth == 0 || !is_proven_loss(root.score, cfg) {
        return None;
    }

    let my_moves = get_safe_neighbors(grid, me, enemy);
    let replies = get_safe_neighbors(grid, enemy, me);
    if my_moves.count < 2 || replies.count < 2 {
        return None;
    }

    let mut best = (preferred, (i32::MIN, i32::MIN));
    let candidates = std::iter::once(preferred).chain(my_moves.as_slice().iter().copied().filter(|m| m.dir_int != preferred.dir_int));
    for mv in candidates {
        let my_ate = grid.food.get(grid.idx(mv.x, mv.y));
        let (mut hope, mut worst) = (0, SCORE_MAX);
        for &reply in replies.as_slice() {
            let outcome = resolve_turn(grid, me, enemy, mv, reply);
            let (points, score) = match outcome_score(outcome, depth, cfg) {
                Some(score) => (if outcome == TurnOutcome::Win { 2 } else if outcome == TurnOutcome::Draw { 1 } else { 0 }, score),
                None => {
                    let bonus = eat_bonus(my_ate, grid.food.get(grid.idx(reply.x, reply.y)), cfg);
                    let (hash, undo) = make_turn(grid, me, enemy, mv, reply, ctx.zobrist, root_hash);
                    let child = search(
                        grid,
                        me,
                        enemy,
                        SearchFrame {
                            depth: depth - 1,
                            alpha: SCORE_MIN,
                            beta: SCORE_MAX,
                            current_hash: hash,
                            q_depth: 0,
                        },
                        ctx,
                    );
                    unmake_turn(grid, me, enemy, undo);
                    let points = if is_proven_loss(child.score, cfg) {
                        0
                    } else if child.score == cfg.scores.draw {
                        1
                    } else {
                        2
                    };
                    (points, apply_bonus(child.score, bonus, cfg))
                }
            };
            hope += points;
            worst = worst.min(score);
        }
        if (hope, worst) > best.1 {
            best = (mv, (hope, worst));
        }
    }
    Some(best.0)
}

/// The result of a root search, and the depth it was searched to.
#[derive(Debug, Clone)]
pub struct RootSearch {
    pub result: SearchResult,
    /// Depth, in turns, of the iteration that produced `result`.
    pub depth: usize,
}

/// Iterative deepening: searches to depth 1, 2, ... `max_depth` (at least 1), returning the deepest result.
///
/// Every iteration leaves its best moves in the transposition table and its cutoffs in the history table, so
/// the next one starts from the previous line instead of guessing. The search stops early once the root is a
/// forced win: the first iteration to see it found the fastest one.
pub fn search_root<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    root_hash: u64,
    max_depth: usize,
    ctx: &mut SearchContext<'_>,
) -> RootSearch {
    let mut depth = 1;
    loop {
        ctx.root_depth = depth;
        let result = search(
            grid,
            me,
            enemy,
            SearchFrame {
                depth,
                alpha: SCORE_MIN,
                beta: SCORE_MAX,
                current_hash: root_hash,
                q_depth: 0,
            },
            ctx,
        );
        let forced_win = result.score > 0 && is_decided(result.score, ctx.cfg);
        if depth >= max_depth || forced_win {
            return RootSearch { result, depth };
        }
        depth += 1;
    }
}

pub fn search<const W: usize, const N: usize>(
    grid: &mut Grid<W, N>,
    me: &mut AgentState,
    enemy: &mut AgentState,
    frame: SearchFrame,
    ctx: &mut SearchContext<'_>,
) -> SearchResult
{
    let SearchFrame {
        depth,
        mut alpha,
        mut beta,
        current_hash,
        q_depth,
    } = frame;

    ctx.nodes_searched += 1;

    let original_alpha = alpha;
    let tt_entry = ctx.tt.get(current_hash);

    if depth != ctx.root_depth
        && let Some(entry) = tt_entry
        && ctx.score_is_reusable(&entry, depth, q_depth)
    {
        match entry.flag {
            TT_FLAG_EXACT => return SearchResult::leaf(entry.score),
            TT_FLAG_LOWER => alpha = alpha.max(entry.score),
            TT_FLAG_UPPER => beta = beta.min(entry.score),
            _ => {}
        }
        if alpha >= beta {
            return SearchResult::leaf(entry.score);
        }
    }

    if me.body.is_empty() || me.health <= 0 {
        return SearchResult::leaf(ctx.cfg.scores.loss - depth as i32);
    }
    if enemy.body.is_empty() || enemy.health <= 0 {
        return SearchResult::leaf(ctx.cfg.scores.win + depth as i32);
    }

    if depth == 0 {
        if ctx.root_depth >= 2 && q_depth < QUIESCENCE_MAX_EXTENSIONS && should_extend_leaf(grid, me, enemy, ctx.cfg) {
            return search(
                grid,
                me,
                enemy,
                SearchFrame {
                    depth: 1,
                    alpha,
                    beta,
                    current_hash,
                    q_depth: q_depth + 1,
                },
                ctx,
            );
        }

        let score = evaluate(grid, me, enemy, ctx.cfg);
        ctx.tt.set(
            current_hash,
            0,
            score,
            TT_FLAG_EXACT,
            255,
            255,
            255,
            255,
            ctx.root_depth.min(255) as u8,
            q_depth.min(255) as u8,
        );
        return SearchResult::leaf(score);
    }

    let is_root = depth == ctx.root_depth;
    let mut my_moves = get_safe_neighbors(grid, me, enemy);
    let mut replies = get_safe_neighbors(grid, enemy, me);

    if my_moves.count == 0 {
        let enemy_doomed = replies.as_slice().iter().all(|&reply| starves(grid, enemy.health, reply));
        let score = if enemy_doomed {
            ctx.cfg.scores.draw
        } else {
            ctx.cfg.scores.loss - depth as i32
        };
        return SearchResult::leaf(score);
    }

    let my_head = me.body.head();
    let en_head = enemy.body.head();
    order_moves(
        &mut my_moves,
        grid,
        my_head,
        &ctx.history_table[0],
        tt_entry.and_then(|e| e.get_direction()),
    );
    order_moves(
        &mut replies,
        grid,
        en_head,
        &ctx.history_table[1],
        tt_entry.and_then(|e| e.get_reply_direction()),
    );

    if replies.count == 0 {
        // The enemy is dead whatever it does; I only need a move that does not starve me too.
        let survivor = my_moves.as_slice().iter().copied().find(|&mv| !starves(grid, me.health, mv));
        return SearchResult {
            score: if survivor.is_some() { ctx.cfg.scores.win + depth as i32 } else { ctx.cfg.scores.draw },
            mv: Some(survivor.unwrap_or(my_moves.as_slice()[0])),
            children: Vec::new(),
        };
    }

    if is_root && ctx.thread_id > 0 {
        let shift = ctx.thread_id % my_moves.count;
        my_moves.as_mut_slice().rotate_left(shift);
    }

    let my_moves = my_moves.as_slice();
    let replies = replies.as_slice();

    let mut child_records = if is_root && ctx.collect_root_children {
        Vec::with_capacity(my_moves.len())
    } else {
        Vec::new()
    };
    let mut best_move = my_moves[0];
    let mut best_reply = replies[0].dir;
    let mut best_score = SCORE_MIN;

    for (i, &mv) in my_moves.iter().enumerate() {
        let ate_food = grid.food.get(grid.idx(mv.x, mv.y));
        let probe = |alpha: i32, beta: i32| ReplySearch {
            my_mv: mv,
            replies,
            node_depth: depth,
            alpha,
            beta,
            hash: current_hash,
            q_depth,
        };

        let (raw_score, reply) = if i == 0 {
            worst_case_reply(grid, me, enemy, &probe(alpha, beta), ctx)
        } else {
            let probed = worst_case_reply(grid, me, enemy, &probe(alpha, alpha + 1), ctx);
            if probed.0 > alpha && probed.0 < beta {
                worst_case_reply(grid, me, enemy, &probe(alpha, beta), ctx)
            } else {
                probed
            }
        };

        if is_root && ctx.collect_root_children {
            child_records.push(RootChildRecord {
                mv,
                score: raw_score,
                ate: ate_food,
            });
        }

        if raw_score > best_score {
            best_score = raw_score;
            best_move = mv;
            best_reply = reply;
        }
        if best_score > alpha {
            alpha = best_score;
        }
        if alpha >= beta {
            break;
        }
    }

    let history_bonus = (depth * depth) as i32;
    ctx.history_table[0][grid.idx(my_head.x, my_head.y) * 4 + best_move.dir_int] += history_bonus;
    ctx.history_table[1][grid.idx(en_head.x, en_head.y) * 4 + direction_index(best_reply)] += history_bonus;

    let tt_flag = if best_score <= original_alpha {
        TT_FLAG_UPPER
    } else if best_score >= beta {
        TT_FLAG_LOWER
    } else {
        TT_FLAG_EXACT
    };

    ctx.tt.set(
        current_hash,
        depth,
        best_score,
        tt_flag,
        best_move.x as u8,
        best_move.y as u8,
        best_move.dir_int as u8,
        direction_index(best_reply) as u8,
        ctx.root_depth.min(255) as u8,
        q_depth.min(255) as u8,
    );

    SearchResult {
        score: best_score,
        mv: Some(best_move),
        children: child_records,
    }
}
