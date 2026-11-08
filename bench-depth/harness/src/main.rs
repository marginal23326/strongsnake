//! Plays two builds of the engine against each other: `base` and `cand`, which are the repo's `crates/` with the
//! variants chosen in prepare.sh applied (by default `base` is unmodified and `cand` has late-move reductions off). Each seed is played twice with the sides swapped, and one `PAIR` line is printed per seed.
//!
//! harness match <seed0> <pairs> <w> <h> <max_turns> <engineA> <depthA> <engineB> <depthB> <max_secs>
use std::time::Instant;

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

            /// Returns the chosen direction (0 up, 1 down, 2 left, 3 right) and the depth reached (0 if not reported).
            #[allow(clippy::too_many_arguments)]
            pub fn decide(me: &[(i32, i32)], me_hp: i32, en: &[(i32, i32)], en_hp: i32, food: &[(i32, i32)], w: i32, h: i32, depth: usize) -> (u8, usize) {
                let m = AgentState { body: FastBody::from_points(points(me)), health: me_hp };
                let e = AgentState { body: FastBody::from_points(points(en)), health: en_hp };
                let d = decide_move(m, e, &points(food), w, h, &cfg(depth));
                let code = match d.best_move {
                    Direction::Up => 0,
                    Direction::Down => 1,
                    Direction::Left => 2,
                    Direction::Right => 3,
                };
                (code, reached!(d, $kind))
            }
        }
    };
}

engine!(base, snake_ai_base, snake_domain, none);
engine!(cand, snake_ai_cand, snake_domain_cand, none);

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
fn play(seed: u32, w: i32, h: i32, start: Start, who: [(Eng, usize); 2], max_turns: u32, stats: &mut [Side; 2]) -> (Option<usize>, u32, Ending) {
    let (mut state, mut rng) = init_state(w, h, start, seed);
    let mut ending = Ending { death: ['-'; 2], len: [3; 2] };
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
            let (eng, depth) = who[i];
            let t = Instant::now();
            let (code, reached) = match eng {
                Eng::Base => base::decide(&bodies[i], hp[i], &bodies[1 - i], hp[1 - i], &food, w, h, depth),
                Eng::Cand => cand::decide(&bodies[i], hp[i], &bodies[1 - i], hp[1 - i], &food, w, h, depth),
            };
            let el = t.elapsed().as_secs_f64();
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
        let summary = simulate_turn(&mut state, &intents, &mut rng, SimConfig::default());
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
    let (ea, da) = (parse_eng(&a[7]), a[8].parse::<usize>().unwrap());
    let (eb, db) = (parse_eng(&a[9]), a[10].parse::<usize>().unwrap());
    let max_secs: f64 = a[11].parse().unwrap();
    base::warm(w, h, da.max(db));
    cand::warm(w, h, da.max(db));

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
            let who = if a_is == 0 { [(ea, da), (eb, db)] } else { [(eb, db), (ea, da)] };
            let mut st = [Side::default(), Side::default()];
            let (winner, t, ending) = play(seed, w, h, start, who, max_turns, &mut st);
            // Report the ending from A's and B's point of view: death code and length of A, then of B.
            ends.push(format!("{}{}:{}:{}", ending.death[a_is], ending.death[1 - a_is], ending.len[a_is], ending.len[1 - a_is]));
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
