//! Plays two builds of the engine against each other: `base` and `cand`, which are the repo's `crates/` with the
//! variants chosen in prepare.sh applied (by default `base` is unmodified and `cand` has late-move reductions off). Each seed is played twice with the sides swapped, and one `PAIR` line is printed per seed.
//!
//! harness match <seed0> <pairs> <w> <h> <max_turns> <engineA> <depthA> <engineB> <depthB> <max_secs>
//! (a depth is a number, or `7~8` for a mix of two depths that keeps the side's total thinking time level with its opponent's)
use std::time::Instant;

mod asym;

use snake_domain::{
    Board, Direction, FoodSettings, GameState, LcgRng, Point, SNAKE_START_SIZE, SimConfig, Snake, place_initial_standard_food, simulate_turn,
    standard_start_points,
};

macro_rules! reached {
    ($d:ident, none) => {
        0usize
    };
}

/// One module per engine build. Positions cross the boundary as plain coordinates because each build has its own types.
macro_rules! engine {
    ($m:ident, $ai:ident, $dom:ident, $kind:ident) => {
        mod $m {
            use $ai::{AgentState, AiConfig, decide_move, model::FastBody, warm_up_runtime};
            use $dom::{Direction, Point};

            fn points(v: &[(i32, i32)]) -> Vec<Point> {
                v.iter().map(|&(x, y)| Point { x, y }).collect()
            }

            fn cfg(depth: usize) -> AiConfig {
                let mut c = AiConfig::default();
                c.max_depth = depth;
                c.runtime.threads = 1;
                c
            }

            pub fn warm(w: i32, h: i32, depth: usize) {
                warm_up_runtime(w, h, &cfg(depth));
            }

            /// Returns the chosen direction (0 up, 1 down, 2 left, 3 right), the depth reached (0 if not reported) and whether
            /// the root score is a proven win (1), a proven loss (-1) or neither (0), and how far the score is below the top score.
            #[allow(clippy::too_many_arguments)]
            pub fn decide(me: &[(i32, i32)], me_hp: i32, en: &[(i32, i32)], en_hp: i32, food: &[(i32, i32)], w: i32, h: i32, depth: usize) -> (u8, usize, i8, i32) {
                let c = cfg(depth);
                let m = AgentState { body: FastBody::from_points(points(me)), health: me_hp };
                let e = AgentState { body: FastBody::from_points(points(en)), health: en_hp };
                let d = decide_move(m, e, &points(food), w, h, &c);
                let decided = c.scores.win.abs() / 10 * 9; // the engine's own threshold for a proven result
                let class = if d.score >= decided { 1 } else if d.score <= -decided { -1 } else { 0 };
                let code = match d.best_move {
                    Direction::Up => 0,
                    Direction::Down => 1,
                    Direction::Left => 2,
                    Direction::Right => 3,
                };
                (code, reached!(d, $kind), class, c.scores.win - d.score)
            }
        }
    };
}

engine!(base, snake_ai_base, snake_domain, none);
engine!(cand, snake_ai_cand, snake_domain_cand, none);

/// A fixed depth (`8`) or a time-matched mix of two depths (`7~8`): the engine searches `hi` whenever its thinking time so
/// far (summed over the whole run of this worker) is at or below its opponent's, and `lo` otherwise, so both sides end up
/// spending the same total time even though depths are whole numbers.
#[derive(Clone, Copy)]
struct DepthSpec {
    lo: usize,
    hi: usize,
}

fn parse_depth(s: &str) -> DepthSpec {
    match s.split_once('~') {
        Some((lo, hi)) => DepthSpec { lo: lo.parse().unwrap(), hi: hi.parse().unwrap() },
        None => DepthSpec { lo: s.parse().unwrap(), hi: s.parse().unwrap() },
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Eng {
    Base,
    Cand,
}

fn parse_eng(s: &str) -> Eng {
    match s {
        "base" => Eng::Base,
        "cand" => Eng::Cand,
        _ => panic!("engine must be base or cand"),
    }
}

/// How the snakes are placed. `Standard` is the "even head distance, bodies stacked on one square" rule.
/// `Fixed` is the harness's older start, (2,2) and (w-3,h-3) with straight bodies, whose head distance is odd on some
/// boards (16x9) and even on others (11x11); `FixedFlipped` moves the second snake one cell left to flip that parity.
#[derive(Clone, Copy, PartialEq)]
enum Start {
    Standard,
    Fixed,
    FixedFlipped,
}

fn init_state(width: i32, height: i32, start: Start, seed: u32) -> (GameState, LcgRng) {
    let mut rng = LcgRng::new(seed);
    let pad = 2;
    let x2 = width - pad - 1 - (start == Start::FixedFlipped) as i32;
    let bodies: [Vec<Point>; 2] = if start == Start::Standard {
        let p = standard_start_points(&mut rng, width, height, 2).expect("board too small for two snakes");
        [vec![p[0]; SNAKE_START_SIZE], vec![p[1]; SNAKE_START_SIZE]]
    } else {
        [
            vec![Point { x: pad, y: pad }, Point { x: pad, y: pad - 1 }, Point { x: pad, y: pad - 2 }],
            vec![Point { x: x2, y: height - pad - 1 }, Point { x: x2, y: height - pad }, Point { x: x2, y: height - pad + 1 }],
        ]
    };
    let [b0, b1] = bodies;
    let mut state = GameState {
        turn: 0,
        seed: seed as u64,
        board: Board {
            width,
            height,
            food: Vec::new(),
            snakes: vec![Snake::new("local", "local", b0, 100), Snake::new("opponent", "opponent", b1, 100)],
        },
    };
    let (w, h) = (state.board.width, state.board.height);
    let snakes = state.board.snakes.clone();
    place_initial_standard_food(&mut rng, w, h, &snakes, &mut state.board.food, FoodSettings::default());
    (state, rng)
}

#[derive(Default, Clone)]
struct Side {
    moves: u64,
    secs: f64,
    depth_sum: u64,
    max_secs: f64,
}

/// How a game ended, per snake: why it died (`-` if it did not) and how long it was when the last turn began.
#[derive(Clone, Copy)]
struct Ending {
    death: [char; 2],
    len: [usize; 2],
    /// First proven result each snake's own search reported: `W`, `L` or `-` (never), the turn it first did, and how many
    /// moves it reported a proven win / loss.
    first: [char; 2],
    first_turn: [i64; 2],
    wins: [u32; 2],
    losses: [u32; 2],
    /// Distance below the top score of each snake's first proven win.
    first_gap: [i32; 2],
    /// How the game would have gone from each snake's first proven win if no food had spawned from then on (`W`, `D`,
    /// `L`, or `-` when it had no proven win). A proven win is a proof against every reply, but the search cannot see
    /// food that has not spawned yet, so this separates "the proof was wrong" from "the food changed the position".
    no_spawn: [char; 2],
}

fn death_code(reason: &str) -> char {
    match reason {
        "Wall" => 'W',
        "Starvation" => 'S',
        "Body" => 'B',
        "Head" => 'H',
        _ => '?',
    }
}

/// Plays one game; `who[i]` is the engine and depth controlling snake `i`. Returns the winning snake, if any.
fn play(
    seed: u32, w: i32, h: i32, start: Start, who: [(Eng, DepthSpec, usize); 2], max_turns: u32, stats: &mut [Side; 2], clock: &mut [f64; 2],
) -> (Option<usize>, u32, Ending) {
    let (state, rng) = init_state(w, h, start, seed);
    let mut snaps: [Option<(GameState, LcgRng)>; 2] = [None, None];
    let (winner, turn, mut ending) = run(state, rng, who, max_turns, stats, clock, SimConfig::default(), Some(&mut snaps));
    // Replay each snake's first proven win with food spawning switched off.
    for i in 0..2 {
        if let Some((state, rng)) = snaps[i].take() {
            let no_food = SimConfig { food: FoodSettings { minimum_food: 0, food_spawn_chance: 0, ..FoodSettings::default() }, ..SimConfig::default() };
            let (mut scratch, mut c2) = ([Side::default(), Side::default()], *clock);
            let (res, ..) = run(state, rng, who, max_turns, &mut scratch, &mut c2, no_food, None);
            ending.no_spawn[i] = match res {
                Some(x) if x == i => 'W',
                Some(_) => 'L',
                None => 'D',
            };
        }
    }
    (winner, turn, ending)
}

/// Plays a game from `state` to its end. With `snaps`, saves the state at each snake's first proven win.
#[allow(clippy::too_many_arguments)]
fn run(
    mut state: GameState, mut rng: LcgRng, who: [(Eng, DepthSpec, usize); 2], max_turns: u32, stats: &mut [Side; 2], clock: &mut [f64; 2], sim: SimConfig,
    mut snaps: Option<&mut [Option<(GameState, LcgRng)>; 2]>,
) -> (Option<usize>, u32, Ending) {
    let w = state.board.width;
    let h = state.board.height;
    let mut ending = Ending { death: ['-'; 2], len: [3; 2], first: ['-'; 2], first_turn: [-1; 2], wins: [0; 2], losses: [0; 2], first_gap: [0; 2], no_spawn: ['-'; 2] };
    while state.turn < max_turns {
        if state.board.snakes.iter().filter(|s| s.alive && !s.body.is_empty()).count() < 2 {
            break;
        }
        let food: Vec<(i32, i32)> = state.board.food.iter().map(|p| (p.x, p.y)).collect();
        let bodies: Vec<Vec<(i32, i32)>> = state.board.snakes.iter().map(|s| s.body.iter().map(|p| (p.x, p.y)).collect()).collect();
        let hp: Vec<i32> = state.board.snakes.iter().map(|s| s.health).collect();
        ending.len = [bodies[0].len(), bodies[1].len()];
        let mut intents = [Direction::Up; 2];
        for i in 0..2 {
            let (eng, spec, side) = who[i];
            let depth = if spec.lo != spec.hi && clock[side] > clock[1 - side] { spec.lo } else { spec.hi };
            let t = Instant::now();
            let (code, reached, class, gap) = match eng {
                Eng::Base => base::decide(&bodies[i], hp[i], &bodies[1 - i], hp[1 - i], &food, w, h, depth),
                Eng::Cand => cand::decide(&bodies[i], hp[i], &bodies[1 - i], hp[1 - i], &food, w, h, depth),
            };
            let el = t.elapsed().as_secs_f64();
            clock[side] += el;
            if class != 0 {
                if class > 0 { ending.wins[i] += 1 } else { ending.losses[i] += 1 }
                if ending.first[i] == '-' {
                    ending.first[i] = if class > 0 { 'W' } else { 'L' };
                    ending.first_turn[i] = state.turn as i64;
                    if class > 0 {
                        ending.first_gap[i] = gap;
                        if let Some(snaps) = snaps.as_deref_mut() {
                            snaps[i] = Some((state.clone(), rng.clone()));
                        }
                    }
                }
            }
            let s = &mut stats[i];
            s.moves += 1;
            s.secs += el;
            s.max_secs = s.max_secs.max(el);
            s.depth_sum += if reached == 0 { depth as u64 } else { reached as u64 };
            intents[i] = match code {
                0 => Direction::Up,
                1 => Direction::Down,
                2 => Direction::Left,
                _ => Direction::Right,
            };
        }
        let summary = simulate_turn(&mut state, &intents, &mut rng, sim);
        for death in &summary.dead {
            let i = state.board.snakes.iter().position(|s| s.id.0 == death.snake_id.0).unwrap();
            ending.death[i] = death_code(&death.reason);
        }
    }
    for i in 0..2 {
        if ending.death[i] == '-' {
            ending.len[i] = state.board.snakes[i].body.len();
        }
    }
    let alive = |i: usize| state.board.snakes[i].alive && !state.board.snakes[i].body.is_empty();
    let len = |i: usize| state.board.snakes[i].body.len();
    let winner = match (alive(0), alive(1)) {
        (true, false) => Some(0),
        (false, true) => Some(1),
        (true, true) if len(0) > len(1) => Some(0),
        (true, true) if len(1) > len(0) => Some(1),
        _ => None,
    };
    (winner, state.turn, ending)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("asym") {
        return asym::run(&a);
    }
    if a.len() != 12 || a[1] != "match" {
        eprintln!("usage: harness match <seed0> <pairs> <w> <h>[p] <max_turns> <engineA> <depthA> <engineB> <depthB> <max_secs>");
        std::process::exit(2);
    }
    let seed0: u32 = a[2].parse().unwrap();
    let pairs: u32 = a[3].parse().unwrap();
    let w: i32 = a[4].parse().unwrap();
    // `11` = new starts, `11f` = the older fixed starts, `11p` = the older fixed starts with flipped parity.
    let start = match a[5].chars().last() {
        Some('f') => Start::Fixed,
        Some('p') => Start::FixedFlipped,
        _ => Start::Standard,
    };
    let h: i32 = a[5].trim_end_matches(['p', 'f']).parse().unwrap();
    let h_label = &a[5];
    let max_turns: u32 = a[6].parse().unwrap();
    let (ea, da) = (parse_eng(&a[7]), parse_depth(&a[8]));
    let (eb, db) = (parse_eng(&a[9]), parse_depth(&a[10]));
    let max_secs: f64 = a[11].parse().unwrap();
    base::warm(w, h, da.hi.max(db.hi));
    cand::warm(w, h, da.hi.max(db.hi));
    let mut clock = [0.0f64; 2]; // thinking time so far of A and B, for the time-matched depth mix

    let started = Instant::now();
    for i in 0..pairs {
        // Stop between pairs, so every finished seed is a complete, side-swapped pair.
        if started.elapsed().as_secs_f64() > max_secs {
            break;
        }
        let seed = seed0 + i;
        let mut stats = [Side::default(), Side::default()]; // [A, B]
        let mut score_a = [0.0f64; 2];
        let mut turns = [0u32; 2];
        let mut ends = Vec::new();
        for (g, a_is) in [0usize, 1].into_iter().enumerate() {
            let who = if a_is == 0 { [(ea, da, 0), (eb, db, 1)] } else { [(eb, db, 1), (ea, da, 0)] };
            let mut st = [Side::default(), Side::default()];
            let (winner, t, ending) = play(seed, w, h, start, who, max_turns, &mut st, &mut clock);
            // Report the ending from A's and B's point of view: death codes and lengths, the first proven result each side's
            // search reported (and the turn), then how many moves each side reported a proven win / loss.
            ends.push(format!(
                "{}{}:{}:{}:{}{}:{}:{}:{}:{}:{}:{}:{}{}:{}:{}",
                ending.death[a_is], ending.death[1 - a_is], ending.len[a_is], ending.len[1 - a_is],
                ending.first[a_is], ending.first[1 - a_is], ending.first_turn[a_is], ending.first_turn[1 - a_is],
                ending.wins[a_is], ending.losses[a_is], ending.wins[1 - a_is], ending.losses[1 - a_is],
                ending.no_spawn[a_is], ending.no_spawn[1 - a_is], ending.first_gap[a_is], ending.first_gap[1 - a_is],
            ));
            score_a[g] = match winner {
                Some(x) if x == a_is => 1.0,
                Some(_) => 0.0,
                None => 0.5,
            };
            turns[g] = t;
            for (side, snake) in [(0usize, a_is), (1usize, 1 - a_is)] {
                let s = &mut stats[side];
                s.moves += st[snake].moves;
                s.secs += st[snake].secs;
                s.depth_sum += st[snake].depth_sum;
                s.max_secs = s.max_secs.max(st[snake].max_secs);
            }
        }
        println!(
            "PAIR {seed} {w} {h_label} {} {} {} {} | {} {:.6} {} {:.6} | {} {:.6} {} {:.6} | {} {}",
            score_a[0], score_a[1], turns[0], turns[1],
            stats[0].moves, stats[0].secs, stats[0].depth_sum, stats[0].max_secs,
            stats[1].moves, stats[1].secs, stats[1].depth_sum, stats[1].max_secs,
            ends[0], ends[1],
        );
    }
}
