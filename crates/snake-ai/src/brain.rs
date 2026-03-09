use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock, RwLock, mpsc};
use std::time::Instant;

use snake_domain::{Direction, Point};

use crate::{
    config::AiConfig,
    floodfill::flood_fill,
    grid::Grid,
    model::{AgentState, SearchBuffers},
    pathfinding::get_food_distance_map,
    search::{RootChildRecord, SearchContext, SearchFrame, negamax},
    tt::TranspositionTable,
    zobrist::Zobrist,
};

#[derive(Debug, Clone)]
pub struct Decision {
    pub best_move: Direction,
    pub score: i32,
    pub log: String,
    pub root_children: Vec<RootChildRecord>,
    pub pv: Vec<Direction>,
    pub search_nodes: u64,
}

struct BrainMemory {
    zobrist: Option<Arc<Zobrist>>,
    max_grid: usize,
}

thread_local! {
    static BRAIN_MEM: RefCell<BrainMemory> = const { RefCell::new(BrainMemory { zobrist: None, max_grid: 0 }) };
}

static GLOBAL_TT: OnceLock<RwLock<TranspositionTable>> = OnceLock::new();
static WORKER_POOL: OnceLock<Mutex<WorkerPool>> = OnceLock::new();

fn get_tt() -> &'static RwLock<TranspositionTable> {
    GLOBAL_TT.get_or_init(|| RwLock::new(TranspositionTable::new(1 << 22)))
}

fn get_worker_pool() -> &'static Mutex<WorkerPool> {
    WORKER_POOL.get_or_init(|| Mutex::new(WorkerPool::new()))
}

fn depth_based_tt_entries(max_depth: usize) -> usize {
    match max_depth {
        0..=4 => 1 << 15,   // 32k entries (~2MB)
        5..=8 => 1 << 17,   // 128k entries (~8MB)
        9..=12 => 1 << 19,  // 512k entries (~32MB)
        13..=20 => 1 << 21, // 2M entries (~128MB)
        _ => 1 << 22,       // 4M entries (~256MB)
    }
}

fn resolve_tt_entries(cfg: &AiConfig) -> usize {
    if cfg.runtime.hash_mb > 0 {
        let entries = TranspositionTable::entries_for_hash_mb(cfg.runtime.hash_mb);
        if entries > 0 {
            return entries;
        }
    }
    depth_based_tt_entries(cfg.max_depth)
}

fn resolve_thread_count(cfg: &AiConfig) -> usize {
    if cfg.runtime.threads > 0 {
        return cfg.runtime.threads;
    }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

#[inline]
fn format_duration_ns(duration_ns: u128) -> String {
    let whole_ms = duration_ns / 1_000_000;
    let fractional_ns = duration_ns % 1_000_000;
    format!("{whole_ms}.{fractional_ns:06}ms")
}

#[inline]
fn get_or_init_zobrist(cols: i32, rows: i32) -> Option<Arc<Zobrist>> {
    if cols <= 0 || rows <= 0 {
        return None;
    }

    BRAIN_MEM.with(|mem_cell| {
        let mut mem_ref = mem_cell.borrow_mut();
        let size = (cols * rows) as usize;
        if size > mem_ref.max_grid || mem_ref.zobrist.as_ref().is_none_or(|z| z.width != cols || z.height != rows) {
            mem_ref.zobrist = Some(Arc::new(Zobrist::new(cols, rows)));
            mem_ref.max_grid = size;
        }
        mem_ref.zobrist.clone()
    })
}

#[inline]
fn prepare_tt_for_search(cfg: &AiConfig) {
    let tt_size = resolve_tt_entries(cfg);
    let tt_lock = get_tt();
    let mut tt_write = tt_lock.write().unwrap();
    tt_write.prepare_for_search(tt_size);
}

#[inline]
fn accumulate_perf_stats(total: &mut crate::PerfStats, stats: &crate::PerfStats) {
    total.negamax_calls += stats.negamax_calls;
    total.eval_calls += stats.eval_calls;
    total.eval_duration += stats.eval_duration;
    total.voronoi_calls += stats.voronoi_calls;
    total.voronoi_duration += stats.voronoi_duration;
    total.floodfill_calls += stats.floodfill_calls;
    total.floodfill_duration += stats.floodfill_duration;
    total.move_gen_calls += stats.move_gen_calls;
    total.move_gen_duration += stats.move_gen_duration;
    total.distmap_calls += stats.distmap_calls;
    total.distmap_duration += stats.distmap_duration;
}

#[derive(Clone)]
struct SearchTask<const N: usize>
where
    [(); (N + 63) / 64]: Sized,
{
    grid: Grid<N>,
    me: AgentState,
    enemy: AgentState,
    dist_map: Arc<[i16]>,
    cfg: Arc<AiConfig>,
    zobrist: Arc<Zobrist>,
    initial_hash: u64,
    grid_size: usize,
}

struct WorkerResult {
    thread_id: usize,
    result: crate::search::SearchResult,
    stats: crate::PerfStats,
}

enum WorkerCommand {
    Search {
        task: WorkerTask,
        result_tx: mpsc::Sender<WorkerResult>,
    },
    Shutdown,
}

struct WorkerHandle {
    tx: mpsc::Sender<WorkerCommand>,
    join: Option<std::thread::JoinHandle<()>>,
}

struct WorkerPool {
    workers: Vec<WorkerHandle>,
}

impl WorkerPool {
    fn new() -> Self {
        Self { workers: Vec::new() }
    }

    fn ensure_workers(&mut self, num_threads: usize) {
        while self.workers.len() < num_threads {
            let thread_id = self.workers.len();
            let (tx, rx) = mpsc::channel();
            let join = std::thread::Builder::new()
                .name(format!("snake-search-{thread_id}"))
                .spawn(move || worker_loop(thread_id, rx))
                .expect("search worker spawn");
            self.workers.push(WorkerHandle { tx, join: Some(join) });
        }
    }

    fn run(&mut self, num_threads: usize, task: &WorkerTask) -> (crate::search::SearchResult, crate::PerfStats) {
        self.ensure_workers(num_threads);

        let (result_tx, result_rx) = mpsc::channel();
        for worker in self.workers.iter().take(num_threads) {
            let task_clone = task.clone();

            worker
                .tx
                .send(WorkerCommand::Search {
                    task: task_clone,
                    result_tx: result_tx.clone(),
                })
                .expect("search worker send");
        }
        drop(result_tx);

        let mut primary_res = None;
        let mut total_stats = crate::PerfStats::default();
        for _ in 0..num_threads {
            let worker_result = result_rx.recv().expect("search worker result");
            accumulate_perf_stats(&mut total_stats, &worker_result.stats);
            if worker_result.thread_id == 0 {
                primary_res = Some(worker_result.result);
            }
        }

        (primary_res.expect("missing primary search result"), total_stats)
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        for worker in &self.workers {
            let _ = worker.tx.send(WorkerCommand::Shutdown);
        }
        for worker in &mut self.workers {
            if let Some(join) = worker.join.take() {
                let _ = join.join();
            }
        }
    }
}

fn reset_worker_state(history: &mut [Vec<i32>; 2], grid_size: usize) {
    let used_history = grid_size * 4;
    for side_history in history {
        if side_history.len() != used_history {
            *side_history = vec![0; used_history];
        } else {
            side_history.fill(0);
        }
    }
}

fn execute_search_task<const N: usize>(
    thread_id: usize,
    mut task: SearchTask<N>,
    history: &mut [Vec<i32>; 2],
    buffers: &mut SearchBuffers,
) -> WorkerResult
where
    [(); (N + 63) / 64]: Sized,
{
    crate::PERF_STATS.with(|s| *s.borrow_mut() = crate::PerfStats::default());

    reset_worker_state(history, task.grid_size);
    *buffers = SearchBuffers::new(task.grid_size);

    let tt_guard = get_tt().read().unwrap();
    let mut search_ctx = SearchContext {
        root_depth: task.cfg.max_depth,
        history_table: history,
        cfg: &task.cfg,
        tt: &tt_guard,
        zobrist: task.zobrist.as_ref(),
        buffers,
        thread_id,
    };

    let result = negamax(
        &mut task.grid,
        &mut task.me,
        &mut task.enemy,
        SearchFrame {
            dist_map: Some(task.dist_map.as_ref()),
            depth: task.cfg.max_depth,
            alpha: -2_000_000_000,
            beta: 2_000_000_000,
            side: 0,
            current_hash: task.initial_hash,
            q_depth: 0,
        },
        &mut search_ctx,
    );

    let stats = crate::PERF_STATS.with(|s| s.take());
    WorkerResult { thread_id, result, stats }
}

fn worker_loop(thread_id: usize, rx: mpsc::Receiver<WorkerCommand>) {
    let mut history = [Vec::new(), Vec::new()];
    let mut buffers = SearchBuffers::new(0);

    while let Ok(command) = rx.recv() {
        match command {
            WorkerCommand::Search { task, result_tx } => {
                let result = match task {
                    // WorkerTask::Task64(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    WorkerTask::Task128(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    WorkerTask::Task192(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    // WorkerTask::Task256(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    // WorkerTask::Task320(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    // WorkerTask::Task384(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                    WorkerTask::Task448(t) => execute_search_task(thread_id, t, &mut history, &mut buffers),
                };
                let _ = result_tx.send(result);
            }
            WorkerCommand::Shutdown => break,
        }
    }
}

fn fallback_move<const N: usize>(grid: &Grid<N>, me: &AgentState, buffers: &mut SearchBuffers) -> Direction
where
    [(); (N + 63) / 64]: Sized,
{
    if me.body.is_empty() {
        return Direction::Up;
    }
    let head = me.body.head();
    let neighbors = [
        (head.x, head.y + 1, Direction::Up),
        (head.x, head.y - 1, Direction::Down),
        (head.x - 1, head.y, Direction::Left),
        (head.x + 1, head.y, Direction::Right),
    ];

    let mut candidates = Vec::new();
    for (x, y, dir) in neighbors {
        if grid.is_safe(x, y) {
            let ff = flood_fill(grid, x, y, 100, Some(&me.body), None, buffers);
            candidates.push((ff.count, dir));
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    candidates.first().map(|c| c.1).unwrap_or(Direction::Up)
}

pub fn warm_up_runtime(cols: i32, rows: i32, cfg: &AiConfig) {
    let _ = get_or_init_zobrist(cols, rows);
    prepare_tt_for_search(cfg);
}

trait IntoWorkerTask {
    fn into_worker_task(self) -> WorkerTask;
}

macro_rules! build_worker_tasks {
    ($( $name:ident => $n:literal ),* $(,)?) => {
        enum WorkerTask {
            $( $name(SearchTask<$n>), )*
        }

        impl Clone for WorkerTask {
            fn clone(&self) -> Self {
                match self {
                    $( Self::$name(t) => Self::$name(t.clone()), )*
                }
            }
        }

        $(
            impl IntoWorkerTask for SearchTask<$n> {
                fn into_worker_task(self) -> WorkerTask {
                    WorkerTask::$name(self)
                }
            }
        )*
    };
}

build_worker_tasks! {
    // Task64 => 64,
    Task128 => 128,
    Task192 => 192,
    // Task256 => 256,
    // Task320 => 320,
    // Task384 => 384,
    Task448 => 448,
}

pub fn decide_move_debug(me: AgentState, enemy: AgentState, foods: Vec<Point>, cols: i32, rows: i32, cfg: &AiConfig) -> Decision {
    let area = (cols * rows) as usize;
    let required_words = (area + 63) / 64;

    match required_words {
        0..=2 => decide_move_debug_inner::<128>(me, enemy, foods, cols, rows, cfg),
        3 => decide_move_debug_inner::<192>(me, enemy, foods, cols, rows, cfg),
        _ => decide_move_debug_inner::<448>(me, enemy, foods, cols, rows, cfg),
    }
}

fn decide_move_debug_inner<const N: usize>(
    me: AgentState,
    enemy: AgentState,
    foods: Vec<Point>,
    cols: i32,
    rows: i32,
    cfg: &AiConfig,
) -> Decision
where
    [(); (N + 63) / 64]: Sized,
    SearchTask<N>: IntoWorkerTask,
{
    crate::PERF_STATS.with(|s| *s.borrow_mut() = crate::PerfStats::default());

    let started = Instant::now();
    let grid = Grid::<N>::from_state(cols, rows, &foods, &me.body, &enemy.body);
    let dist_map = Arc::<[i16]>::from(get_food_distance_map(&grid));
    
    let pre_worker_stats = crate::PERF_STATS.with(|s| *s.borrow());

    let grid_size = (cols * rows) as usize;

    let zobrist = get_or_init_zobrist(cols, rows).unwrap();
    let initial_hash = zobrist.compute_hash(&grid, me.health, enemy.health);

    prepare_tt_for_search(cfg);
    let num_threads = resolve_thread_count(cfg);
    let base_task = SearchTask {
        grid: grid.clone(),
        me: me.clone(),
        enemy: enemy.clone(),
        dist_map,
        cfg: Arc::new(cfg.clone()),
        zobrist,
        initial_hash,
        grid_size,
    };

    let (primary_res, mut aggregated_stats) = if num_threads <= 1 {
        let mut history = [Vec::new(), Vec::new()];
        let mut buffers = SearchBuffers::new(grid_size);
        let worker_result = execute_search_task(0, base_task, &mut history, &mut buffers);
        (worker_result.result, worker_result.stats)
    } else {
        let mut pool = get_worker_pool().lock().unwrap();
        let worker_task = base_task.clone().into_worker_task();
        pool.run(num_threads, &worker_task)
    };

    accumulate_perf_stats(&mut aggregated_stats, &pre_worker_stats);
    crate::PERF_STATS.with(|s| *s.borrow_mut() = aggregated_stats);

    let mut fallback_buffers = SearchBuffers::new(grid_size);
    let selected = primary_res
        .mv
        .map(|m| m.dir)
        .unwrap_or_else(|| fallback_move(&grid, &me, &mut fallback_buffers));
    let score = primary_res.score;
    let root_children = primary_res.children;
    let pv = primary_res.pv.moves[0..primary_res.pv.len].to_vec();

    let mut log = format!("Score: {}", score);
    if root_children.is_empty() {
        log.push_str(" | FAILSAFE");
    }
    let elapsed = started.elapsed();
    log.push_str(" | ");
    log.push_str(&format_duration_ns(elapsed.as_nanos()));

    if cfg.debug_logging {
        let secs = elapsed.as_secs_f64();
        let nps = if secs > 0.0 {
            (aggregated_stats.negamax_calls as f64 / secs) as u64
        } else {
            0
        };

        crate::PERF_STATS.with(|s| {
            let st = s.borrow();
            println!("PROFILING:");
            println!("Total time: {}", format_duration_ns(elapsed.as_nanos()));
            println!("Nodes ({} Threads): {} ({} NPS)", num_threads, st.negamax_calls, nps);
            println!("Eval: {:>8} calls, {:?}", st.eval_calls, st.eval_duration);
            println!("Voronoi: {:>8} calls, {:?}", st.voronoi_calls, st.voronoi_duration);
            println!("Floodfill: {:>8} calls, {:?}", st.floodfill_calls, st.floodfill_duration);
            println!("MoveGen: {:>8} calls, {:?}", st.move_gen_calls, st.move_gen_duration);
            println!("DistMap: {:>8} calls, {:?}", st.distmap_calls, st.distmap_duration);
            println!("======\n");
        });
    }

    Decision {
        best_move: selected,
        score,
        log,
        root_children,
        pv,
        search_nodes: aggregated_stats.negamax_calls,
    }
}
