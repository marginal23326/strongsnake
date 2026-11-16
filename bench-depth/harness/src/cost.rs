//! Cost diagnostic: `harness cost <seed0> <games> <w> <h> <depth> <max_turns>`.
//!
//! Plays self-play games with the `base` build and asks both builds for a move at every turn, from both snakes' points
//! of view, so both see the very same positions. Search nodes are deterministic with one thread, so the node ratio is
//! exact; the time ratio divided by it is the cost per node (the eval and move generation). Prints one markdown table,
//! then how the `base` self-play grows (mean length and food on the board at a few turns) and how often a snake goes for food
//! that is close and uncontested (a diagnostic for growth variants: with `cand` = `none` and `base` = the variant).
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
    // Mean snake length (both snakes, games still running) at a few turns of the `base` self-play: how fast the engine grows.
    const MARKS: [u32; 5] = [10, 25, 50, 100, 200];
    let mut grow = [(0f64, 0u32); MARKS.len()];
    // Uncontested nearby food: food within 3 steps of a snake whose enemy is 3+ steps farther from it, at 70+ health.
    // Does the chosen move get closer to it? (Counted on the `base` snake's own choice.)
    let (mut near_total, mut near_toward, mut near_ate) = (0u64, 0u64, 0u64);
    let mut by_dist = [(0u64, 0u64, 0u64); 4];
    let mut food_on_board = [(0f64, 0u32); MARKS.len()];
    for g in 0..games {
        let (mut state, mut rng) = init_state(w, h, Start::Standard, seed0 + g);
        while state.turn < max_turns && state.board.snakes.iter().filter(|s| s.alive && !s.body.is_empty()).count() == 2 {
            let food: Vec<(i32, i32)> = state.board.food.iter().map(|p| (p.x, p.y)).collect();
            let bodies: Vec<Vec<(i32, i32)>> = state.board.snakes.iter().map(|s| s.body.iter().map(|p| (p.x, p.y)).collect()).collect();
            let hp: Vec<i32> = state.board.snakes.iter().map(|s| s.health).collect();
            let mut moves = [0u8; 2];
            let mut near: Vec<(usize, (i32, i32), i32)> = Vec::new();
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
                if hp[i] >= 70 {
                    let md = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() + (a.1 - b.1).abs();
                    if let Some(&f) = food.iter().min_by_key(|f| md(**f, bodies[i][0])) {
                        let (dm, de) = (md(f, bodies[i][0]), md(f, bodies[j][0]));
                        if dm <= 3 && de >= dm + 3 {
                            near.push((i, f, dm));
                        }
                    }
                }
            }
            for &(i, f, dm) in &near {
                let h = bodies[i][0];
                let nh = match moves[i] { 0 => (h.0, h.1 + 1), 1 => (h.0, h.1 - 1), 2 => (h.0 - 1, h.1), _ => (h.0 + 1, h.1) };
                let nd = (f.0 - nh.0).abs() + (f.1 - nh.1).abs();
                near_total += 1;
                near_toward += (nd < dm) as u64;
                near_ate += (nd == 0) as u64;
                by_dist[dm as usize].0 += 1;
                by_dist[dm as usize].1 += (nd < dm) as u64;
                by_dist[dm as usize].2 += (nd == 0) as u64;
            }
            if let Some(k) = MARKS.iter().position(|&m| m == state.turn) {
                food_on_board[k].0 += state.board.food.len() as f64;
                food_on_board[k].1 += 1;
                for sn in &state.board.snakes {
                    grow[k].0 += sn.body.len() as f64;
                    grow[k].1 += 1;
                }
            }
            for &(i, f, dm) in &near {
                let h = bodies[i][0];
                let nh = match moves[i] { 0 => (h.0, h.1 + 1), 1 => (h.0, h.1 - 1), 2 => (h.0 - 1, h.1), _ => (h.0 + 1, h.1) };
                let nd = (f.0 - nh.0).abs() + (f.1 - nh.1).abs();
                near_total += 1;
                near_toward += (nd < dm) as u64;
                near_ate += (nd == 0) as u64;
                by_dist[dm as usize].0 += 1;
                by_dist[dm as usize].1 += (nd < dm) as u64;
                by_dist[dm as usize].2 += (nd == 0) as u64;
            }
            if let Some(k) = MARKS.iter().position(|&m| m == state.turn) {
                food_on_board[k].0 += state.board.food.len() as f64;
                food_on_board[k].1 += 1;
                for sn in &state.board.snakes {
                    grow[k].0 += sn.body.len() as f64;
                    grow[k].1 += 1;
                }
            }
            if std::env::var("TRACE").is_ok() && g == 0 && state.turn <= 120 && state.turn % 4 == 0 {
                let nd = |i: usize| food.iter().map(|f| (f.0 - bodies[i][0].0).abs() + (f.1 - bodies[i][0].1).abs()).min().unwrap_or(-1);
                eprintln!("t{} A head {:?} len {} hp {} nearest food {} | B head {:?} len {} hp {} nearest {} | food {}", state.turn, bodies[0][0], bodies[0].len(), hp[0], nd(0), bodies[1][0], bodies[1].len(), hp[1], nd(1), food.len());
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
    let marks: Vec<String> = MARKS.iter().zip(grow).map(|(m, (sum, n))| format!("turn {m}: {:.2}", sum / n.max(1) as f64)).collect();
    let fobs: Vec<String> = MARKS.iter().zip(food_on_board).map(|(m, (sum, n))| format!("turn {m}: {:.1}", sum / n.max(1) as f64)).collect();
    println!("\nMean food items on the board: {}.", fobs.join(", "));
    println!("\nMean snake length in the `base` self-play: {}.", marks.join(", "));
    println!(
        "\nUncontested food within 3 steps at 70+ health: {near_total} snake-turns; the chosen move gets closer in {:.1}%, eats it in {:.1}%.",
        100.0 * near_toward as f64 / near_total.max(1) as f64,
        100.0 * near_ate as f64 / near_total.max(1) as f64
    );
    for d in 1..4 {
        let (n, tw, at) = by_dist[d];
        println!("  food {d} step(s) away: {n} snake-turns, closer {:.1}%, eats {:.1}%", 100.0 * tw as f64 / n.max(1) as f64, 100.0 * at as f64 / n.max(1) as f64);
    }
}
