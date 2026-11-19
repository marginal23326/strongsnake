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
//!
//! A run can also be spread over machines (`--stage eval` / `--stage merge`, driven by .github/workflows/trainer.yml): a
//! generation is a fixed set of (candidate, seed) games whose results depend on nothing else, so each machine plays a
//! slice, and the merge step adds the slices up and does the model step. The numbers are the same as in one process.

mod genes;
mod pairs;
mod pool;
mod stats;
mod surface;
mod types;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rand::{SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
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
        self.evaluate_slice(candidates, specs, 0, 1)
    }

    /// Like `evaluate`, but plays only slice `slice` of `slices`: the pair `k` of candidate `i` belongs to slice
    /// `(i + k) % slices`, which gives every slice the same number of games (give or take one) whatever the pair count.
    /// The tallies of all slices add up to what `evaluate` returns.
    fn evaluate_slice(&self, candidates: &[AiConfig], specs: &[PairSpec], slice: usize, slices: usize) -> Result<Vec<Tally>> {
        let mut jobs = Vec::new();
        let mut owner = Vec::new();
        for (i, candidate) in candidates.iter().enumerate() {
            let mine: Vec<PairSpec> = specs.iter().enumerate().filter(|(k, _)| (i + k) % slices == slice).map(|(_, s)| *s).collect();
            for chunk in mine.chunks(CHUNK_PAIRS) {
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

fn load_checkpoint(path: Option<&PathBuf>, fingerprint: &str) -> Result<Option<TrainerCheckpoint>> {
    let Some(path) = path.filter(|p| p.exists()) else {
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

/// What a run keeps from one generation to the next.
struct State {
    /// Generations completed.
    generation: usize,
    mean: Offsets,
    /// The mean after every completed generation.
    history: Vec<Offsets>,
    observations: Vec<Observation>,
}

impl State {
    fn fresh() -> Self {
        Self {
            generation: 0,
            mean: vec![0.0; GENE_COUNT],
            history: Vec::new(),
            observations: Vec::new(),
        }
    }
}

/// The candidates of one generation and the seeds they all play.
struct Plan {
    points: Vec<Offsets>,
    configs: Vec<AiConfig>,
    specs: Vec<PairSpec>,
}

/// What stays the same through a run.
struct Setup {
    options: TrainerOptions,
    base_cfg: AiConfig,
    start: AiConfig,
    reference: AiConfig,
    fingerprint: String,
}

impl Setup {
    fn new(base_cfg: AiConfig, mut options: TrainerOptions) -> Self {
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
        Self {
            options,
            base_cfg,
            start,
            reference,
            fingerprint,
        }
    }

    fn evaluator(&self) -> Evaluator<'_> {
        Evaluator {
            reference: &self.reference,
            options: &self.options,
        }
    }

    /// The state in the checkpoint file, or the start of a run when there is no such file.
    fn load_state(&self, path: Option<&PathBuf>) -> Result<State> {
        Ok(match load_checkpoint(path, &self.fingerprint)? {
            Some(checkpoint) => State {
                generation: checkpoint.generation.min(self.options.gens),
                mean: checkpoint.mean,
                history: checkpoint.history,
                observations: checkpoint.observations,
            },
            None => State::fresh(),
        })
    }

    fn write_checkpoint(&self, path: &Path, state: &State) -> Result<()> {
        let checkpoint = TrainerCheckpoint {
            fingerprint: self.fingerprint.clone(),
            generation: state.generation,
            mean: state.mean.clone(),
            history: state.history.clone(),
            observations: state.observations.clone(),
        };
        write_atomically(path, &serde_json::to_string_pretty(&checkpoint)?)
    }

    fn print_header(&self, state: &State) {
        let options = &self.options;
        if state.generation > 0 {
            println!("Resumed at generation {}", state.generation);
        }
        println!(
            "Trainer: {} candidates, {} generations, {} pairs per candidate and generation; depth {}, boards {:?}, {} workers",
            options.candidates, options.gens, options.pairs, options.depth, options.boards, options.workers
        );
        println!(
            "Reference: {} ({}). A pair is one seed played twice with the sides swapped; score 0.5 = even.",
            if options.reference.is_some() { "the given reference weights" } else { "the starting weights" },
            describe(&[0.0; GENE_COUNT], &self.reference)
        );
        if options.reference.is_some() {
            println!("Starting from: {}", describe(&state.mean, &self.start));
        }
        println!(
            "{} pairs ({} games) per generation; step size {:.3} -> {:.3}.",
            options.candidates * options.pairs,
            options.candidates * options.pairs * 2,
            options.sigma,
            options.sigma_end
        );
    }

    fn sigma(&self, generation: usize) -> f64 {
        let options = &self.options;
        let decay = if options.gens > 1 { (options.sigma_end / options.sigma).powf(1.0 / (options.gens - 1) as f64) } else { 1.0 };
        options.sigma * decay.powi(generation as i32)
    }

    /// The candidates of generation `state.generation`: the mean and mirrored pairs around it. Depends on nothing but the
    /// state and the options, so every machine of a sharded run computes the same plan.
    fn plan(&self, state: &State) -> Plan {
        let options = &self.options;
        let generation = state.generation;
        let sigma = self.sigma(generation);
        let mut rng = StdRng::seed_from_u64(splitmix(options.seed ^ splitmix(generation as u64 + 1)));
        let mut points: Vec<Offsets> = vec![state.mean.clone()];
        while points.len() < options.candidates {
            let (plus, minus) = genes::sample_antithetic(&state.mean, sigma, &mut rng);
            points.push(plus);
            points.push(minus);
        }
        let configs: Vec<AiConfig> = points.iter().map(|p| genes::apply(&self.start, p)).collect();

        // Everyone plays the same seeds, so board and luck cancel out of the comparison between candidates.
        let specs = specs((0..options.pairs).map(|k| training_seed(options.seed, generation, 0, k)), &options.boards);
        Plan { points, configs, specs }
    }

    /// Takes in the results of the generation `plan` and moves the mean.
    fn advance(&self, state: &mut State, plan: &Plan, tallies: &[Tally]) {
        let generation = state.generation;
        let sigma = self.sigma(generation);
        for (point, t) in plan.points.iter().zip(tallies) {
            state.observations.push(Observation {
                offsets: point.clone(),
                pairs: t.n,
                mean: t.mean(),
                variance: t.variance(),
            });
        }

        let (moved, predicted, gradient) = match surface::plan_step(&state.observations, &state.mean, self.options.sigma, sigma) {
            Some(step) => {
                for (m, d) in state.mean.iter_mut().zip(&step.delta) {
                    *m = (*m + d).clamp(-genes::MAX_OFFSET, genes::MAX_OFFSET);
                }
                (step.delta.iter().map(|d| d * d).sum::<f64>().sqrt(), step.predicted_gain, step.gradient_norm)
            }
            None => (0.0, 0.0, 0.0),
        };
        println!(
            "Gen {:>2}/{} sigma {:.3} | mean was {:.3} +- {:.3} | moved {:.3}, model expects {:+.3} (optimistic), gradient {:.3} | mean now: {}",
            generation + 1,
            self.options.gens,
            sigma,
            tallies[0].mean(),
            tallies[0].margin(),
            moved,
            predicted,
            gradient,
            describe(&state.mean, &self.start)
        );
        state.history.push(state.mean.clone());
        state.generation += 1;
    }

    /// The answer: the average of the second half of the means, fixed before any validation game is played.
    fn answer(&self, state: &State) -> (Offsets, AiConfig) {
        let tail: Vec<&[f64]> = state.history[state.history.len() / 2..].iter().map(Vec::as_slice).collect();
        let answer = if tail.is_empty() { state.mean.clone() } else { genes::average(&tail) };
        let cfg = genes::apply(&self.start, &answer);
        (answer, cfg)
    }

    fn validation_specs(&self) -> Vec<PairSpec> {
        specs((0..self.options.validation_pairs).map(|k| validation_seed(self.options.seed, k)), &self.options.boards)
    }

    /// Reports how the answer did on the validation pairs and saves the result.
    fn conclude(&self, state: &State, tally: Tally) -> Result<TrainerSummary> {
        let (answer, answer_cfg) = self.answer(state);
        let (elo_low, elo_high) = tally.elo_interval();
        let proven = elo_low > 0.0;

        println!("Tuned weights vs the starting weights:");
        for ((name, before), (_, after)) in genes::values(&self.start).into_iter().zip(genes::values(&answer_cfg)) {
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

        if let Some(path) = &self.options.save {
            let tuned = genes::apply(&self.base_cfg, &answer);
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
                "settings": &self.options,
            });
            write_atomically(path, &serde_json::to_string_pretty(&output)?)?;
            println!("Saved {}", path.display());
        }
        Ok(summary)
    }
}

fn train(base_cfg: AiConfig, options: TrainerOptions) -> Result<TrainerSummary> {
    let setup = Setup::new(base_cfg, options);
    let mut state = setup.load_state(setup.options.resume.as_ref())?;
    setup.print_header(&state);
    let evaluator = setup.evaluator();

    for _ in state.generation..setup.options.gens {
        let plan = setup.plan(&state);
        let tallies = evaluator.evaluate(&plan.configs, &plan.specs)?;
        setup.advance(&mut state, &plan, &tallies);
        if let Some(path) = &setup.options.checkpoint {
            setup.write_checkpoint(path, &state)?;
        }
    }

    println!("\nValidating the answer on {} fresh pairs...", setup.options.validation_pairs);
    let (_, answer_cfg) = setup.answer(&state);
    let tally = evaluator.evaluate(std::slice::from_ref(&answer_cfg), &setup.validation_specs())?.remove(0);
    setup.conclude(&state, tally)
}

/// One machine's share of a sharded generation, written by `run_trainer_eval` and read by `run_trainer_merge`.
#[derive(Debug, Serialize, Deserialize)]
struct Partial {
    fingerprint: String,
    /// Generations completed when the slice was played (the generation being played, counting from zero).
    generation: usize,
    /// `train` for a generation, `validate` for the final answer's validation pairs.
    phase: String,
    slice: usize,
    slices: usize,
    tallies: Vec<Tally>,
}

fn phase_of(setup: &Setup, state: &State) -> &'static str {
    if state.generation < setup.options.gens { "train" } else { "validate" }
}

/// `--stage eval`: plays slice `slice` of `slices` of the next generation of the run in the checkpoint (a fresh run when
/// there is none), or of the validation once every generation is done, and writes the tallies to `partial`.
pub fn run_trainer_eval(base_cfg: AiConfig, options: TrainerOptions, slice: usize, slices: usize, partial: &Path) -> Result<()> {
    if slices == 0 || slice >= slices {
        bail!("--shard {slice} must be below --shards {slices}");
    }
    let setup = Setup::new(base_cfg, options);
    let state = setup.load_state(setup.options.checkpoint.as_ref())?;
    let evaluator = setup.evaluator();
    let phase = phase_of(&setup, &state);
    let tallies = if phase == "train" {
        let plan = setup.plan(&state);
        evaluator.evaluate_slice(&plan.configs, &plan.specs, slice, slices)?
    } else {
        let (_, answer_cfg) = setup.answer(&state);
        evaluator.evaluate_slice(std::slice::from_ref(&answer_cfg), &setup.validation_specs(), slice, slices)?
    };
    let pairs: usize = tallies.iter().map(|t| t.n).sum();
    let body = Partial {
        fingerprint: setup.fingerprint.clone(),
        generation: state.generation,
        phase: phase.to_owned(),
        slice,
        slices,
        tallies,
    };
    write_atomically(partial, &serde_json::to_string(&body)?)?;
    println!("Slice {slice}/{slices} of the {phase} step of generation {}: {pairs} pairs played.", state.generation + 1);
    Ok(())
}

/// `--stage merge`: adds up the slices in `dir`, moves the run on by one generation and rewrites the checkpoint; after the
/// last generation it judges the answer on the validation slices and saves the result. Prints `STATUS continue` when
/// another step is due and `STATUS done` at the end.
pub fn run_trainer_merge(base_cfg: AiConfig, options: TrainerOptions, dir: &Path) -> Result<()> {
    let setup = Setup::new(base_cfg, options);
    let Some(checkpoint_path) = setup.options.checkpoint.clone() else {
        bail!("--stage merge needs --checkpoint <file>");
    };
    let mut state = setup.load_state(Some(&checkpoint_path))?;
    let phase = phase_of(&setup, &state);

    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut partials = Vec::new();
    for file in &files {
        let raw = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        partials.push(serde_json::from_str::<Partial>(&raw).with_context(|| format!("parsing {}", file.display()))?);
    }
    let Some(slices) = partials.first().map(|p| p.slices) else {
        bail!("no slice files in {}", dir.display());
    };

    // A missing or repeated slice would silently skew the result, so insist on exactly one of each.
    let plan = (phase == "train").then(|| setup.plan(&state));
    let candidates = plan.as_ref().map_or(1, |p| p.configs.len());
    let mut seen = vec![false; slices];
    let mut total = vec![Tally::default(); candidates];
    for p in &partials {
        if p.fingerprint != setup.fingerprint || p.generation != state.generation || p.phase != phase || p.slices != slices {
            bail!("a slice file belongs to another step or run (slice {}, {} of generation {}, expected {phase} of generation {})", p.slice, p.phase, p.generation, state.generation);
        }
        if p.slice >= slices || std::mem::replace(&mut seen[p.slice], true) {
            bail!("slice {} appears twice or is out of range", p.slice);
        }
        if p.tallies.len() != candidates {
            bail!("slice {} has {} tallies, expected {candidates}", p.slice, p.tallies.len());
        }
        for (sum, t) in total.iter_mut().zip(&p.tallies) {
            sum.merge(t);
        }
    }
    if let Some(missing) = seen.iter().position(|s| !s) {
        bail!("slice {missing} of {slices} is missing");
    }

    if let Some(plan) = plan {
        if state.generation == 0 {
            setup.print_header(&state);
        }
        setup.advance(&mut state, &plan, &total);
        setup.write_checkpoint(&checkpoint_path, &state)?;
        println!("STATUS continue");
    } else {
        println!("Validating the answer on {} fresh pairs...", setup.options.validation_pairs);
        setup.conclude(&state, total.remove(0))?;
        println!("STATUS done");
    }
    Ok(())
}
