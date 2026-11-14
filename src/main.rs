mod gui;
mod server;
mod services;

use std::{
    io::{self, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Result, bail};
use clap::{ArgAction, Args, Parser, Subcommand};
use snake_ai::{AiConfig, RuntimeConfig};
use snake_api::normalize_api_type;
use tracing_subscriber::EnvFilter;

use crate::{
    gui::run_gui,
    server::run_server,
    services::{
        ArenaOptions, RegressionOptions, RegressionOutput, TrainerOptions, default_scenario_dir, format_arena_progress_line,
        format_arena_summary_report, format_opponent_roster, parse_arena_find_modes, parse_boards, parse_depths,
        run_arena_with_progress, run_regression_suite, run_trainer, run_trainer_worker,
    },
};

const TEST_DEPTHS_USAGE: &str = "`snake-app test --depths <LIST>`";

#[derive(Debug, Parser)]
#[command(name = "snake-app", version, about = "Snake Lab Rust: GUI + CLI runners")]
struct Cli {
    #[arg(
        long,
        value_parser = parse_positive_usize,
        help = "AI search depth (must be >= 1)"
    )]
    depth: Option<usize>,
    #[arg(long = "depths", hide = true)]
    test_depths: Option<String>,

    #[command(flatten)]
    runtime: RuntimeCliArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Args)]
struct RuntimeCliArgs {
    #[arg(
        long,
        global = true,
        default_value_t = RuntimeConfig::DEFAULT_THREADS,
        help = "AI search threads (0 = auto)"
    )]
    threads: usize,
    #[arg(
        long = "hash-mb",
        alias = "tt-mb",
        alias = "hashSize",
        global = true,
        default_value_t = RuntimeConfig::DEFAULT_HASH_MB,
        help = "Transposition table size in MiB (0 = auto)"
    )]
    hash_mb: usize,
    #[arg(long, global = true)]
    debug_perf: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server {
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long, default_value_t = 9000)]
        port: u16,
        #[arg(long, value_parser = parse_positive_usize)]
        depth: Option<usize>,
    },
    Test(TestCliArgs),
    Arena(ArenaCliArgs),
    Trainer(Box<TrainerCliArgs>),
    /// Internal: plays a batch of trainer pairs for the trainer process (job on stdin, result on stdout).
    #[command(hide = true)]
    TrainerWorker,
}

#[derive(Debug, Args)]
struct TestCliArgs {
    #[arg(long)]
    scenario_dir: Option<PathBuf>,
    #[arg(long, default_value = RegressionOptions::DEFAULT_DEPTHS_RAW)]
    depths: String,
    #[arg(long, default_value_t = false)]
    quiet: bool,
    #[arg(long, default_value_t = false)]
    quiet_fail_only: bool,
    #[arg(long, short = 'r', default_value_t = 1)]
    repeat: usize,
}

impl TestCliArgs {
    fn into_runtime(self, rust_root: &Path) -> RegressionOptions {
        let scenario_dir = self.scenario_dir.unwrap_or_else(|| default_scenario_dir(rust_root));

        // Map CLI flags to explicit Enum state
        let output = RegressionOutput::from_flags(self.quiet, self.quiet_fail_only);

        RegressionOptions {
            scenario_dir,
            depths: parse_depths(&self.depths),
            output,
            repeat: self.repeat,
        }
    }
}

#[derive(Debug, Args)]
struct ArenaCliArgs {
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_GAMES)]
    games: usize,
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_SEED)]
    seed: u32,
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_WIDTH)]
    width: i32,
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_HEIGHT)]
    height: i32,
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_MAX_TURNS)]
    max_turns: u32,
    #[arg(long, default_value = ArenaOptions::DEFAULT_OPPONENT)]
    opponent: String,
    #[arg(long, default_value_t = ArenaOptions::DEFAULT_SELF_PLAY)]
    self_play: bool,
    #[arg(long, default_value = ArenaOptions::DEFAULT_API_RAW)]
    api: String,
    #[arg(
        long,
        default_value_t = ArenaOptions::DEFAULT_REQUEST_TIMEOUT_MS
    )]
    request_timeout: u64,
    #[arg(
        long,
        alias = "move-timeout",
        default_value_t = ArenaOptions::DEFAULT_PAYLOAD_TIMEOUT_MS
    )]
    payload_timeout: u32,
    #[arg(long, value_parser = parse_positive_usize)]
    depth: Option<usize>,
    #[arg(long = "find", short = 'f', action = ArgAction::Append)]
    find: Vec<String>,
    #[arg(
        long,
        alias = "loss-only",
        default_value_t = ArenaOptions::DEFAULT_ONLY_LOSS
    )]
    only_loss: bool,
    #[arg(
        long,
        short = 'r',
        alias = "load-snapshot",
        default_value_t = ArenaOptions::DEFAULT_RESUME
    )]
    resume: bool,
    #[arg(long = "list-opponents", alias = "listOpponents", short = 'L', action = ArgAction::SetTrue)]
    list_opponents: bool,
    #[arg(long = "snapshot-file", short = 'R', alias = "resume-file", alias = "snapshotFile")]
    snapshot_file: Option<PathBuf>,
    #[arg(
        long = "snapshot-ticks",
        default_value_t = ArenaOptions::DEFAULT_SNAPSHOT_TICKS
    )]
    snapshot_ticks: usize,
}

impl ArenaCliArgs {
    fn into_runtime(self, rust_root: &Path) -> ArenaOptions {
        let (find_modes, invalid_find_modes) = parse_arena_find_modes(&self.find);
        ArenaOptions {
            games: self.games,
            seed: self.seed,
            width: self.width,
            height: self.height,
            max_turns: self.max_turns,
            opponent: self.opponent,
            self_play: self.self_play,
            api_flavor: normalize_api_type(Some(self.api.as_str())),
            request_timeout_ms: self.request_timeout,
            payload_timeout_ms: self.payload_timeout,
            find_modes,
            invalid_find_modes,
            only_loss: self.only_loss,
            resume: self.resume,
            snapshot_file: self
                .snapshot_file
                .unwrap_or_else(|| rust_root.join("data").join("arena_snapshot.json")),
            snapshot_ticks: self.snapshot_ticks.max(1),
        }
    }
}

#[derive(Debug, Args)]
struct TrainerCliArgs {
    #[arg(long, alias = "pop", default_value_t = TrainerOptions::DEFAULT_CANDIDATES, help = "Candidates per generation (rounded up to an odd number: the mean plus mirrored pairs)")]
    candidates: usize,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_GENS)]
    gens: usize,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_PAIRS, help = "Pairs (one seed, both sides) each candidate plays per generation")]
    pairs: usize,
    #[arg(long, value_parser = parse_positive_usize, help = "Search depth of both sides")]
    depth: Option<usize>,
    #[arg(long, default_value = TrainerOptions::DEFAULT_BOARDS_RAW, help = "Comma-separated boards, e.g. 11x11,16x9")]
    boards: String,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_MAX_TURNS)]
    max_turns: u32,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_SIGMA, help = "First step size (relative weight change, log scale)")]
    sigma: f64,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_SIGMA_END, help = "Step size of the last generation")]
    sigma_end: f64,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_SEED)]
    seed: u64,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_WORKERS, help = "Worker processes (0 = one per core)")]
    workers: usize,
    #[arg(long, default_value_t = TrainerOptions::DEFAULT_VALIDATION_PAIRS, help = "Fresh pairs the final answer is judged on")]
    validation_pairs: usize,
    #[arg(long, help = "Start (and default reference) weights: an AiConfig JSON or a saved result")]
    start: Option<PathBuf>,
    #[arg(long, help = "Fixed opponent weights (same formats as --start)")]
    reference: Option<PathBuf>,
    #[arg(long)]
    save: Option<PathBuf>,
    #[arg(long)]
    resume: Option<PathBuf>,
    #[arg(long)]
    checkpoint: Option<PathBuf>,
}

/// An `AiConfig` from a JSON file: either the config itself or a trainer result (its `bestConfig`).
fn load_start_config(path: &Path) -> Result<AiConfig> {
    let raw = std::fs::read_to_string(path)?;
    let mut value: serde_json::Value = serde_json::from_str(&raw)?;
    if let Some(best) = value.get_mut("bestConfig") {
        value = best.take();
    }
    Ok(serde_json::from_value(value)?)
}

impl TrainerCliArgs {
    fn into_runtime(self, base_cfg: AiConfig, rust_root: &Path, depth: Option<usize>) -> Result<(AiConfig, TrainerOptions)> {
        let options = TrainerOptions {
            candidates: self.candidates,
            gens: self.gens,
            pairs: self.pairs,
            depth: depth.unwrap_or(TrainerOptions::DEFAULT_DEPTH),
            boards: parse_boards(&self.boards)?,
            max_turns: self.max_turns,
            sigma: self.sigma,
            sigma_end: self.sigma_end,
            seed: self.seed,
            workers: self.workers,
            validation_pairs: self.validation_pairs,
            reference: self.reference.as_deref().map(load_start_config).transpose()?,
            save: self.save.or_else(|| Some(rust_root.join("data").join("trainer_result.json"))),
            resume: self.resume,
            checkpoint: self.checkpoint,
        };
        let mut cfg = match &self.start {
            Some(path) => {
                let mut start = load_start_config(path)?;
                start.runtime = base_cfg.runtime.clone();
                start
            }
            None => base_cfg,
        };
        cfg.max_depth = options.depth;
        Ok((cfg, options))
    }
}

fn rust_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn parse_positive_usize(raw: &str) -> std::result::Result<usize, String> {
    let parsed = raw.parse::<usize>().map_err(|_| "must be an integer >= 1".to_owned())?;
    if parsed == 0 {
        return Err("must be >= 1".to_owned());
    }
    Ok(parsed)
}

fn resolve_depth(global_depth: Option<usize>, command_depth: Option<usize>) -> Option<usize> {
    command_depth.or(global_depth)
}

fn apply_depth_override(cfg: &mut AiConfig, global_depth: Option<usize>, command_depth: Option<usize>) {
    if let Some(depth) = resolve_depth(global_depth, command_depth) {
        cfg.max_depth = depth;
    }
}

fn apply_runtime_config(cfg: &mut AiConfig, runtime: &RuntimeCliArgs) {
    cfg.runtime.threads = runtime.threads;
    cfg.runtime.hash_mb = runtime.hash_mb;
    cfg.debug_logging = runtime.debug_perf;
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .with_target(false)
        .compact()
        .init();

    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let msg = err.to_string();
            if msg.contains("--depths <TEST_DEPTHS>") && msg.contains("value is required") {
                bail!("--depths is only supported with test; use {TEST_DEPTHS_USAGE}");
            }
            err.exit();
        }
    };
    let command = cli.command;
    if cli.test_depths.is_some() && !matches!(&command, Some(Command::Test(_))) {
        bail!("--depths is only supported with test; use {TEST_DEPTHS_USAGE}");
    }
    let mut base_cfg = AiConfig::default();
    apply_runtime_config(&mut base_cfg, &cli.runtime);

    match command {
        None => {
            apply_depth_override(&mut base_cfg, cli.depth, None);
            run_gui(base_cfg)?
        }
        Some(Command::Server { host, port, depth }) => {
            apply_depth_override(&mut base_cfg, cli.depth, depth);
            let addr = SocketAddr::from_str(&format!("{host}:{port}"))?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            rt.block_on(run_server(addr, base_cfg))?;
        }
        Some(Command::Test(args)) => {
            if cli.depth.is_some() {
                bail!("--depth is not supported with test; use {TEST_DEPTHS_USAGE}");
            }

            let mut args = args;
            if args.depths == RegressionOptions::DEFAULT_DEPTHS_RAW
                && let Some(raw) = cli.test_depths.as_deref()
            {
                args.depths = raw.to_owned();
            }

            let rust_root = rust_root();
            let options = args.into_runtime(&rust_root);
            let summary = run_regression_suite(base_cfg, options)?;
            if summary.failed > 0 {
                std::process::exit(1);
            }
        }
        Some(Command::Arena(args)) => {
            if args.list_opponents {
                println!("{}", format_opponent_roster());
                return Ok(());
            }
            apply_depth_override(&mut base_cfg, cli.depth, args.depth);
            let rust_root = rust_root();
            let options = args.into_runtime(&rust_root);
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            let summary = rt.block_on(run_arena_with_progress(base_cfg, options, |progress| {
                print!("\r{}", format_arena_progress_line(&progress));
                let _ = io::stdout().flush();
            }))?;
            println!();
            println!("{}", format_arena_summary_report(&summary));
        }
        Some(Command::Trainer(args)) => {
            let args = *args;
            let effective_depth = resolve_depth(cli.depth, args.depth);
            let rust_root = rust_root();
            let (cfg, options) = args.into_runtime(base_cfg, &rust_root, effective_depth)?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            rt.block_on(run_trainer(cfg, options))?;
        }
        Some(Command::TrainerWorker) => run_trainer_worker()?,
    }

    Ok(())
}
