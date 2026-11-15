//! Cost diagnostic: `harness cost <seed0> <games> <w> <h> <depth> <max_turns>`.
//!
//! Plays self-play games with the `base` build and asks both builds for a move at every turn, from both snakes' points
//! of view, so both see the very same positions. Search nodes are deterministic with one thread, so the node ratio is
//! exact; the time ratio divided by it is the cost per node (the eval and move generation). Prints one markdown table.
use std::time::Instant;

use snake_domain::{Direction, SimConfig, simulate_turn};

use crate::{Start, base, cand, init_state};

fn dir(code: u8) -> Direction {
    match code {
        0 => Direction::Up,
        1 => Direction::Down,
        2 => Direction::Left,
        _ => Direction::Right,
    }
}

#[derive(Default)]
struct Tally {
    nodes: u64,
    secs: f64,
}

pub fn run(a: &[String]) {
    if a.len() != 8 {
        eprintln!("usage: harness cost <seed0> <games> <w> <h> <depth> <max_turns>");
        std::process::exit(2);
    }
    let seed0: u32 = a[2].parse().unwrap();
    let games: u32 = a[3].parse().unwrap();
    let (w, h): (i32, i32) = (a[4].parse().unwrap(), a[5].parse().unwrap());
    let depth: usize = a[6].parse().unwrap();
    let max_turns: u32 = a[7].parse().unwrap();
    base::warm(w, h, depth);
    cand::warm(w, h, depth);

    let (mut tb, mut tc) = (Tally::default(), Tally::default());
    let (mut differ, mut positions) = (0usize, 0usize);
    // Per-position node ratios, to see whether the growth is everywhere or in a few positions.
    let mut ratios: Vec<f64> = Vec::new();
    for g in 0..games {
        let (mut state, mut rng) = init_state(w, h, Start::Standard, seed0 + g);
        while state.turn < max_turns && state.board.snakes.iter().filter(|s| s.alive && !s.body.is_empty()).count() == 2 {
            let food: Vec<(i32, i32)> = state.board.food.iter().map(|p| (p.x, p.y)).collect();
            let bodies: Vec<Vec<(i32, i32)>> = state.board.snakes.iter().map(|s| s.body.iter().map(|p| (p.x, p.y)).collect()).collect();
            let hp: Vec<i32> = state.board.snakes.iter().map(|s| s.health).collect();
            let mut moves = [0u8; 2];
            for (i, j) in [(0usize, 1usize), (1, 0)] {
                let t = Instant::now();
                let (mb, nb) = base::nodes(&bodies[i], hp[i], &bodies[j], hp[j], &food, w, h, depth);
                tb.secs += t.elapsed().as_secs_f64();
                tb.nodes += nb;
                let t = Instant::now();
                let (mc, nc) = cand::nodes(&bodies[i], hp[i], &bodies[j], hp[j], &food, w, h, depth);
                tc.secs += t.elapsed().as_secs_f64();
                tc.nodes += nc;
                positions += 1;
                differ += (mb != mc) as usize;
                ratios.push(nc as f64 / nb.max(1) as f64);
                moves[i] = mb;
            }
            simulate_turn(&mut state, &[dir(moves[0]), dir(moves[1])], &mut rng, SimConfig::default());
        }
    }
    ratios.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let q = |p: f64| ratios[((ratios.len() - 1) as f64 * p).round() as usize];
    let npm = |t: &Tally| t.nodes as f64 / positions as f64;
    let nsn = |t: &Tally| t.secs * 1e9 / t.nodes.max(1) as f64;
    println!("| build | positions | nodes/move | ms/move | ns/node |");
    println!("|---|---:|---:|---:|---:|");
    println!("| base | {positions} | {:.0} | {:.3} | {:.0} |", npm(&tb), tb.secs * 1e3 / positions as f64, nsn(&tb));
    println!("| cand | {positions} | {:.0} | {:.3} | {:.0} |", npm(&tc), tc.secs * 1e3 / positions as f64, nsn(&tc));
    println!(
        "\ncand / base: nodes {:.3}x, time {:.3}x, ns per node {:.3}x. Per-position node ratio: p10 {:.2}, median {:.2}, p90 {:.2}. \
         Root move differs in {:.2}% of positions ({differ} of {positions}). {games} games of `base` at depth {depth} on {w}x{h}.",
        tc.nodes as f64 / tb.nodes as f64,
        tc.secs / tb.secs,
        nsn(&tc) / nsn(&tb),
        q(0.1),
        q(0.5),
        q(0.9),
        100.0 * differ as f64 / positions as f64,
    );
}
