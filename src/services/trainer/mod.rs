//! Weight trainer: tunes eval weights against a fixed reference and reports whether the result is statistically real.
//!
//! Candidates play side-swapped pairs against the reference on shared seeds. Each generation:
//!  1. sample candidates around the mean in log space (every weight takes a relative step), in mirrored pairs;
//!  2. all of them play the same fresh seeds, and each result joins the pool of every result so far;
//!  3. a quadratic model of score against weights is fitted to the whole pool (surface.rs) and the mean moves to its
//!     best point within a trust radius that shrinks over the run.
//!
//! The answer (the average of the later means) is then validated on fresh pairs no fit ever saw, and counts as proven
//! only if its 95% Elo interval lies above zero. That is a one-sided test: about 2.5% of runs that found nothing still
//! report "proven", so confirm a win with the bench-depth workflow.

mod genes;
mod pairs;
mod pool;
mod stats;
mod surface;
mod types;

use std::path::Path;

use anyhow::{Context, Result, bail};
use rand::{SeedableRng, rngs::StdRng};
use serde_json::json;
use snake_ai::AiConfig;

use self::{
    genes::{GENE_COUNT, Offsets},
    pairs::{PairJob, PairSpec},
    stats::Tally,
    surface::Observation,
    types::TrainerCheckpoint,
};

pub use self::{
    pairs::run_worker,
    types::{TrainerOptions, TrainerSummary, parse_boards},
};

/// Pairs per worker process: small enough to keep every core busy late in a race.
const CHUNK_PAIRS: usize = 4;
/// Training seeds stay below this and validation seeds above, so validation games are never seen in training.
const VALIDATION_SEED_BASE: u32 = 0x8000_0000;

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn training_seed(seed: u64, generation: usize, round: usize, k: usize) -> u32 {
    let key = ((generation as u64) << 40) ^ ((round as u64) << 32) ^ k as u64;
    (splitmix(seed ^ splitmix(key)) & 0x7fff_ffff) as u32
}

fn validation_seed(seed: u64, k: usize) -> u32 {
    VALIDATION_SEED_BASE | (splitmix(seed ^ splitmix(0xA11D_A7E0_0000_0000 ^ k as u64)) & 0x7fff_ffff) as u32
}

fn specs(seeds: impl Iterator<Item = u32>, boards: &[(i32, i32)]) -> Vec<PairSpec> {
    seeds
        .enumerate()
        .map(|(k, seed)| {
            let (width, height) = boards[k % boards.len()];
            PairSpec { seed, width, height }
        })
        .collect()
}

/// Plays candidates against the reference in worker processes.
struct Evaluator<'a> {
    reference: &'a AiConfig,
    options: &'a TrainerOptions,
}

impl Evaluator<'_> {
    /// One tally per candidate; all play the same `specs`.
    fn evaluate(&self, candidates: &[AiConfig], specs: &[PairSpec]) -> Result<Vec<Tally>> {
        let mut jobs = Vec::new();
        let mut owner = Vec::new();
        for (i, candidate) in candidates.iter().enumerate() {
            for chunk in specs.chunks(CHUNK_PAIRS) {
                jobs.push(PairJob {
                    candidate: candidate.clone(),
                    reference: self.reference.clone(),
                    max_turns: self.options.max_turns,
                    pairs: chunk.to_vec(),
                });
                owner.push(i);
            }
        }
        let results = pool::run_jobs(&jobs, self.options.workers)?;
        let mut tallies = vec![Tally::default(); candidates.len()];
        for (i, chunk) in owner.into_iter().zip(results) {
            for r in chunk {
                tallies[i].add(r.score, r.capped as usize);
            }
        }
        Ok(tallies)
    }
}

fn load_checkpoint(options: &TrainerOptions, fingerprint: &str) -> Result<Option<TrainerCheckpoint>> {
    let Some(path) = options.resume.as_ref().filter(|p| p.exists()) else {
        return Ok(None);
    };
    let raw = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let checkpoint: TrainerCheckpoint = serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    if checkpoint.fingerprint != fingerprint {
        bail!(
            "{} was made with other settings and cannot be resumed:\n  checkpoint: {}\n  now:        {}",
            path.display(),
            checkpoint.fingerprint,
            fingerprint
        );
    }
    Ok(Some(checkpoint))
}

fn write_atomically(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

fn describe(offsets: &[f64], start: &AiConfig) -> String {
    let tuned = genes::values(&genes::apply(start, offsets));
    tuned.iter().map(|(name, v)| format!("{name}={v:.4}")).collect::<Vec<_>>().join(" ")
}

pub async fn run_trainer(base_cfg: AiConfig, options: TrainerOptions) -> Result<TrainerSummary> {
    train(base_cfg, options)
}

fn train(base_cfg: AiConfig, mut options: TrainerOptions) -> Result<TrainerSummary> {
    options.normalize();

    // One search thread and a fixed depth make a game a pure function of its seed and the two configs.
    let mut start = base_cfg.clone();
    start.max_depth = options.depth;
    start.runtime.threads = 1;
    start.debug_logging = false;
    let reference = match &options.reference {
        Some(cfg) => {
            let mut cfg = cfg.clone();
            cfg.max_depth = options.depth;
            cfg.runtime.threads = 1;
            cfg.debug_logging = false;
            cfg
        }
        None => start.clone(),
    };
    let fingerprint = format!("{} start={:?} reference={:?}", options.fingerprint(), genes::values(&start), genes::values(&reference));
    let evaluator = Evaluator {
        reference: &reference,
        options: &options,
    };

    let mut mean: Offsets = vec![0.0; GENE_COUNT];
    let mut history: Vec<Offsets> = Vec::new();
    let mut observations: Vec<Observation> = Vec::new();
    let mut first_generation = 0;
    if let Some(checkpoint) = load_checkpoint(&options, &fingerprint)? {
        first_generation = checkpoint.generation.min(options.gens);
        mean = checkpoint.mean;
        history = checkpoint.history;
        observations = checkpoint.observations;
        println!("Resumed at generation {first_generation}");
    }

    let decay = if options.gens > 1 { (options.sigma_end / options.sigma).powf(1.0 / (options.gens - 1) as f64) } else { 1.0 };
    println!(
        "Trainer: {} candidates, {} generations, {} pairs per candidate and generation; depth {}, boards {:?}, {} workers",
        options.candidates, options.gens, options.pairs, options.depth, options.boards, options.workers
    );
    println!(
        "Reference: {} ({}). A pair is one seed played twice with the sides swapped; score 0.5 = even.",
        if options.reference.is_some() { "the given reference weights" } else { "the starting weights" },
        describe(&[0.0; GENE_COUNT], &reference)
    );
    if options.reference.is_some() {
        println!("Starting from: {}", describe(&mean, &start));
    }
    println!(
        "{} pairs ({} games) per generation; step size {:.3} -> {:.3}.",
        options.candidates * options.pairs,
        options.candidates * options.pairs * 2,
        options.sigma,
        options.sigma_end
    );

    for generation in first_generation..options.gens {
        let sigma = options.sigma * decay.powi(generation as i32);
        let mut rng = StdRng::seed_from_u64(splitmix(options.seed ^ splitmix(generation as u64 + 1)));
        let mut points: Vec<Offsets> = vec![mean.clone()];
        while points.len() < options.candidates {
            let (plus, minus) = genes::sample_antithetic(&mean, sigma, &mut rng);
            points.push(plus);
            points.push(minus);
        }
        let configs: Vec<AiConfig> = points.iter().map(|p| genes::apply(&start, p)).collect();

        // Everyone plays the same seeds, so board and luck cancel out of the comparison between candidates.
        let round_specs = specs((0..options.pairs).map(|k| training_seed(options.seed, generation, 0, k)), &options.boards);
        let tallies = evaluator.evaluate(&configs, &round_specs)?;
        for (point, t) in points.iter().zip(&tallies) {
            observations.push(Observation {
                offsets: point.clone(),
                pairs: t.n,
                mean: t.mean(),
                variance: t.variance(),
            });
        }

        let (moved, predicted, gradient) = match surface::plan_step(&observations, &mean, options.sigma, sigma) {
            Some(step) => {
                for (m, d) in mean.iter_mut().zip(&step.delta) {
                    *m = (*m + d).clamp(-genes::MAX_OFFSET, genes::MAX_OFFSET);
                }
                (step.delta.iter().map(|d| d * d).sum::<f64>().sqrt(), step.predicted_gain, step.gradient_norm)
            }
            None => (0.0, 0.0, 0.0),
        };
        println!(
            "Gen {:>2}/{} sigma {:.3} | mean was {:.3} +- {:.3} | moved {:.3}, model expects {:+.3} (optimistic), gradient {:.3} | mean now: {}",
            generation + 1,
            options.gens,
            sigma,
            tallies[0].mean(),
            tallies[0].margin(),
            moved,
            predicted,
            gradient,
            describe(&mean, &start)
        );
        history.push(mean.clone());

        if let Some(path) = &options.checkpoint {
            let checkpoint = TrainerCheckpoint {
                fingerprint: fingerprint.clone(),
                generation: generation + 1,
                mean: mean.clone(),
                history: history.clone(),
                observations: observations.clone(),
            };
            write_atomically(path, &serde_json::to_string_pretty(&checkpoint)?)?;
        }
    }

    // The answer: the average of the second half of the means, fixed before any validation game is played.
    let tail: Vec<&[f64]> = history[history.len() / 2..].iter().map(Vec::as_slice).collect();
    let answer = if tail.is_empty() { mean.clone() } else { genes::average(&tail) };
    let answer_cfg = genes::apply(&start, &answer);

    println!("\nValidating the answer on {} fresh pairs...", options.validation_pairs);
    let validation_specs = specs((0..options.validation_pairs).map(|k| validation_seed(options.seed, k)), &options.boards);
    let tally = evaluator.evaluate(std::slice::from_ref(&answer_cfg), &validation_specs)?.remove(0);
    let (elo_low, elo_high) = tally.elo_interval();
    let proven = elo_low > 0.0;

    println!("Tuned weights vs the starting weights:");
    for ((name, before), (_, after)) in genes::values(&start).into_iter().zip(genes::values(&answer_cfg)) {
        println!("  {name:<16} {before:>12.4} -> {after:>12.4}  (x{:.2})", after / before);
    }
    println!(
        "Validation: score {:.3} +- {:.3} over {} pairs ({} games, {} hit the turn cap)",
        tally.mean(),
        tally.margin(),
        tally.n,
        tally.n * 2,
        tally.capped
    );
    println!("Elo {:+.1} (95% interval {:+.1} to {:+.1})", tally.elo(), elo_low, elo_high);
    if proven {
        println!("VERDICT: proven better than the reference at this depth. Confirm it with the bench-depth workflow before adopting it.");
    } else {
        println!("VERDICT: not proven. The interval includes zero (or is below it): keep the current weights, or train longer / validate with more pairs.");
    }

    let summary = TrainerSummary {
        proven,
        score: tally.mean(),
        elo: tally.elo(),
        elo_low,
        elo_high,
        validation_pairs: tally.n,
        genes: genes::values(&answer_cfg),
    };

    if let Some(path) = &options.save {
        let tuned = genes::apply(&base_cfg, &answer);
        let output = json!({
            "verdict": if proven { "proven" } else { "not_proven" },
            "validation": {
                "pairs": tally.n,
                "score": tally.mean(),
                "scoreStdErr": tally.std_err(),
                "elo": tally.elo(),
                "eloLow": elo_low,
                "eloHigh": elo_high,
                "gamesHittingTurnCap": tally.capped,
            },
            "genes": summary.genes.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
            "bestConfig": tuned,
            "settings": &options,
        });
        write_atomically(path, &serde_json::to_string_pretty(&output)?)?;
        println!("Saved {}", path.display());
    }

    Ok(summary)
}
