//! Eval-symmetry diagnostic: `harness asym <seed0> <games> <w> <h> <depth> <every> <max_turns>`.
//!
//! Plays self-play games with the `base` build (so both builds see the very same positions) and, every `every` turns,
//! scores the position with both builds' eval from each snake's point of view. A symmetric eval gives
//! `eval(a, b) + eval(b, a) == 0`; the residual says how much the eval favours whoever happens to be "me".
//! It also asks both builds for a move at the same depth and counts how often they differ, which bounds how much a
//! variant can change in the arena at all.
//!
//! Prints a markdown table (one row per build) for the job summary.
use snake_domain::{Direction, SimConfig, simulate_turn};

use crate::{Start, base, cand, init_state};

macro_rules! evals {
    ($m:ident, $ai:ident, $dom:ident) => {
        mod $m {
            use $ai::{AgentState, AiConfig, grid::Grid, heuristics::evaluate, model::FastBody};
            use $dom::Point;

            fn points(v: &[(i32, i32)]) -> Vec<Point> {
                v.iter().map(|&(x, y)| Point { x, y }).collect()
            }

            #[allow(dead_code)]
            pub fn unit() -> i32 {
                AiConfig::default().scores.territory_control
            }

            /// `(eval(a, b), eval(b, a))` for the same position, each from its own snake's point of view.
            pub fn both(a: &[(i32, i32)], a_hp: i32, b: &[(i32, i32)], b_hp: i32, food: &[(i32, i32)], w: i32, h: i32) -> (i32, i32) {
                let cfg = AiConfig::default();
                let sa = AgentState { body: FastBody::from_points(points(a)), health: a_hp };
                let sb = AgentState { body: FastBody::from_points(points(b)), health: b_hp };
                let mut grid = Grid::<7, 448>::from_state(w, h, &points(food), &sa.body, &sb.body);
                let x = evaluate(&mut grid, &sa, &sb, &cfg);
                let y = evaluate(&mut grid, &sb, &sa, &cfg);
                (x, y)
            }
        }
    };
}

evals!(ev_base, snake_ai_base, snake_domain);
evals!(ev_cand, snake_ai_cand, snake_domain_cand);

/// Scores at or beyond this are verdicts (win, loss, trap, ...), not the smooth heuristic part.
const VERDICT: i64 = 10_000_000;

#[derive(Default)]
struct Stats {
    samples: usize,
    verdict: usize,
    exact: usize,
    smooth_abs: Vec<i64>,
    smooth_sum: i64,
    both_ahead: usize,
    both_behind: usize,
}

impl Stats {
    fn add(&mut self, x: i32, y: i32) {
        let (x, y) = (x as i64, y as i64);
        self.samples += 1;
        let res = x + y;
        if res == 0 {
            self.exact += 1;
        }
        if x > 0 && y > 0 {
            self.both_ahead += 1;
        }
        if x < 0 && y < 0 {
            self.both_behind += 1;
        }
        if x.abs() >= VERDICT || y.abs() >= VERDICT {
            self.verdict += 1;
        } else {
            self.smooth_abs.push(res.abs());
            self.smooth_sum += res;
        }
    }

    fn row(&mut self, name: &str, unit: i32, differs: Option<(usize, usize)>) -> String {
        self.smooth_abs.sort_unstable();
        let n = self.samples.max(1) as f64;
        let pct = |c: usize| format!("{:.1}%", 100.0 * c as f64 / n);
        let q = |p: f64| {
            if self.smooth_abs.is_empty() {
                "-".to_owned()
            } else {
                let i = ((self.smooth_abs.len() - 1) as f64 * p).round() as usize;
                format!("{:.1}", self.smooth_abs[i] as f64 / unit as f64)
            }
        };
        let mean = if self.smooth_abs.is_empty() {
            "-".to_owned()
        } else {
            format!("{:+.2}", self.smooth_sum as f64 / self.smooth_abs.len() as f64 / unit as f64)
        };
        let diff = match differs {
            Some((d, t)) => format!("{:.2}% ({d} of {t})", 100.0 * d as f64 / t.max(1) as f64),
            None => "-".to_owned(),
        };
        format!(
            "| {name} | {} | {} | {} | {mean} | {} | {} | {} | {} | {diff} |",
            self.samples,
            pct(self.exact),
            pct(self.verdict),
            q(0.5),
            q(0.9),
            pct(self.both_ahead),
            pct(self.both_behind),
        )
    }
}

fn dir(code: u8) -> Direction {
    match code {
        0 => Direction::Up,
        1 => Direction::Down,
        2 => Direction::Left,
        _ => Direction::Right,
    }
}

pub fn run(a: &[String]) {
    if a.len() != 9 {
        eprintln!("usage: harness asym <seed0> <games> <w> <h> <depth> <every> <max_turns>");
        std::process::exit(2);
    }
    let seed0: u32 = a[2].parse().unwrap();
    let games: u32 = a[3].parse().unwrap();
    let (w, h): (i32, i32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
    let depth: usize = a[6].parse().unwrap();
    let every: u32 = a[7].parse::<u32>().unwrap().max(1);
    let max_turns: u32 = a[8].parse().unwrap();
    base::warm(w, h, depth);
    cand::warm(w, h, depth);

    let (mut sb, mut sc) = (Stats::default(), Stats::default());
    let (mut differ, mut compared) = (0usize, 0usize);
    for g in 0..games {
        let (mut state, mut rng) = init_state(w, h, Start::Standard, seed0 + g);
        while state.turn < max_turns && state.board.snakes.iter().filter(|s| s.alive && !s.body.is_empty()).count() == 2 {
            let food: Vec<(i32, i32)> = state.board.food.iter().map(|p| (p.x, p.y)).collect();
            let bodies: Vec<Vec<(i32, i32)>> = state.board.snakes.iter().map(|s| s.body.iter().map(|p| (p.x, p.y)).collect()).collect();
            let hp: Vec<i32> = state.board.snakes.iter().map(|s| s.health).collect();
            let m0 = base::decide(&bodies[0], hp[0], &bodies[1], hp[1], &food, w, h, depth).0;
            let m1 = base::decide(&bodies[1], hp[1], &bodies[0], hp[0], &food, w, h, depth).0;
            if state.turn % every == 0 {
                let (x, y) = ev_base::both(&bodies[0], hp[0], &bodies[1], hp[1], &food, w, h);
                sb.add(x, y);
                let (x, y) = ev_cand::both(&bodies[0], hp[0], &bodies[1], hp[1], &food, w, h);
                sc.add(x, y);
                compared += 1;
                differ += (cand::decide(&bodies[0], hp[0], &bodies[1], hp[1], &food, w, h, depth).0 != m0) as usize;
            }
            simulate_turn(&mut state, &[dir(m0), dir(m1)], &mut rng, SimConfig::default());
        }
    }

    let unit = ev_base::unit();
    println!("| build | positions | exactly antisymmetric | verdict-level | mean eval(a,b)+eval(b,a) | median abs | p90 abs | both sides ahead | both sides behind | root move differs from base |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|---|");
    println!("{}", sb.row("base", unit, None));
    println!("{}", sc.row("cand", unit, Some((differ, compared))));
    println!(
        "\n{games} self-play games of `base` at depth {depth} on {w}x{h}, a position every {every} turns. The residual \
         `eval(a,b)+eval(b,a)` is 0 for a symmetric eval; mean/median/p90 are in territory cells (the `territory_control` weight) over \
         the positions that are not verdict-level (|score| < 1e7). \"Both sides ahead\" counts positions where each snake's own eval \
         is positive."
    );
}
