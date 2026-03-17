use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use serde::Serialize;
use snake_ai::{AiConfig, decide_move_debug, warm_up_runtime};
use snake_io::{Expectation, load_scenarios_from_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegressionOutput {
    #[default]
    Full, // Headers, Passes, Fails, Summaries
    Summary,      // Headers, Fails, Summaries (No individual passes)
    FailuresOnly, // Only Fails (No headers or summaries)
    Silent,       // Nothing
}

impl RegressionOutput {
    pub fn from_flags(quiet: bool, fail_only: bool) -> Self {
        match (quiet, fail_only) {
            (false, false) => Self::Full,
            (false, true) => Self::Summary,
            (true, true) => Self::FailuresOnly,
            (true, false) => Self::Silent,
        }
    }

    fn show_headers(&self) -> bool {
        matches!(self, Self::Full | Self::Summary)
    }

    fn show_passes(&self) -> bool {
        matches!(self, Self::Full)
    }

    fn show_fails(&self) -> bool {
        !matches!(self, Self::Silent)
    }
}

#[derive(Debug, Clone)]
pub struct RegressionOptions {
    pub scenario_dir: PathBuf,
    pub depths: Vec<usize>,
    pub output: RegressionOutput,
    pub repeat: usize,
}

impl RegressionOptions {
    pub const DEFAULT_DEPTHS_RAW: &'static str = "6";
}

#[derive(Debug, Clone, Serialize)]
pub struct DepthResult {
    pub depth: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ns: u128,
    pub nodes: u64,
    pub nps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegressionSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub scenarios: usize,
    pub by_depth: Vec<DepthResult>,
    pub duration_ns: u128,
    pub nodes: u64,
    pub nps: u64,
}

impl DepthResult {
    pub fn duration_display(&self) -> String {
        format_duration_ns(self.duration_ns)
    }
}

impl RegressionSummary {
    pub fn duration_display(&self) -> String {
        format_duration_ns(self.duration_ns)
    }
}

fn format_duration_ns(duration_ns: u128) -> String {
    let whole_ms = duration_ns / 1_000_000;
    let fractional_ns = duration_ns % 1_000_000;
    format!("{whole_ms}.{fractional_ns:06}ms")
}

pub fn run_regression_suite(mut cfg: AiConfig, options: RegressionOptions) -> Result<RegressionSummary> {
    let scenarios = load_scenarios_from_dir(&options.scenario_dir)
        .with_context(|| format!("failed to load scenarios from {}", options.scenario_dir.display()))?;

    let depths = if options.depths.is_empty() {
        vec![cfg.max_depth]
    } else {
        options.depths
    };

    let repeats = options.repeat.max(1);

    let mut summary = RegressionSummary {
        passed: 0,
        failed: 0,
        skipped: 0,
        scenarios: scenarios.len(),
        by_depth: Vec::new(),
        duration_ns: 0,
        nodes: 0,
        nps: 0,
    };

    if options.output.show_headers() {
        let repeat_msg = if repeats > 1 {
            format!(" (averaged over {} repeats)", repeats)
        } else {
            "".to_string()
        };
        println!(
            "\nRUNNING RUST REGRESSION SUITE ({} scenarios){}\nDepths: {}\n",
            scenarios.len(),
            repeat_msg,
            depths.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")
        );
    }

    let warmup_dims = scenarios.iter().find_map(|named| {
        (named.scenario.board.width > 0 && named.scenario.board.height > 0)
            .then_some((named.scenario.board.width, named.scenario.board.height))
    });

    for depth in depths {
        if options.output.show_headers() {
            println!("=== DEPTH {depth} ===");
        }
        cfg.max_depth = depth;

        if let Some((cols, rows)) = warmup_dims {
            warm_up_runtime(cols, rows, &cfg);
        }

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut total_depth_nodes = 0u64;
        let mut total_depth_duration_ns = 0u128;

        for named in &scenarios {
            let scenario = &named.scenario;

            // WARMUP
            if repeats > 1 {
                if let Some((me, enemy, food, cols, rows)) = scenario.into_ai_inputs() {
                    let _ = decide_move_debug(me, enemy, &food, cols, rows, &cfg);
                }
            }

            let mut total_scenario_nodes = 0u64;
            let mut last_decision = None;
            let mut skipped_scenario = false;

            let measure_start = Instant::now();

            for _ in 0..repeats {
                let Some((me, enemy, food, cols, rows)) = scenario.into_ai_inputs() else {
                    skipped_scenario = true;
                    break;
                };
                let decision = decide_move_debug(me, enemy, &food, cols, rows, &cfg);
                total_scenario_nodes += decision.search_nodes;
                last_decision = Some(decision);
            }

            let scenario_duration = measure_start.elapsed().as_nanos();

            if skipped_scenario || last_decision.is_none() {
                skipped += 1;
                continue;
            }

            total_depth_duration_ns += scenario_duration;
            total_depth_nodes += total_scenario_nodes;

            let decision = last_decision.unwrap();
            let pass = scenario.expectation.passes(decision.best_move);
            let file_name = short_scenario_name(&named.file);

            if pass {
                passed += 1;
                if options.output.show_passes() {
                    println!("PASS {file_name} -> {}", decision.best_move.as_lower());
                }
            } else {
                failed += 1;
                if options.output.show_fails() {
                    println!("DEBUG: Root Moves for {}:", file_name);
                    for child in &decision.root_children {
                        println!(
                            "  - {:?}: Score: {:.2}, Recursive: {:.2}, Penalty: {:.2}, Ate: {}",
                            child.mv.dir, child.modified_score, child.raw_recursion_score, child.collision_penalty, child.ate
                        );
                    }

                    let pv_str = decision.pv.iter().take(12).map(|d| d.as_upper()).collect::<Vec<_>>().join(" -> ");
                    println!("  PV: {}", pv_str);

                    println!(
                        "{} [Depth {}] {file_name}: expected {}, got {}.",
                        colorize_red("FAIL"),
                        depth,
                        concise_expectation(&scenario.expectation),
                        decision.best_move.as_lower(),
                    );
                }
            }
        }

        let duration_ns = total_depth_duration_ns / (repeats as u128);
        let depth_nodes = total_depth_nodes / (repeats as u64);
        let secs = duration_ns as f64 / 1_000_000_000.0;
        let nps = if secs > 0.0 { (depth_nodes as f64 / secs) as u64 } else { 0 };

        summary.passed += passed;
        summary.failed += failed;
        summary.skipped += skipped;
        summary.nodes += depth_nodes;
        summary.by_depth.push(DepthResult {
            depth,
            passed,
            failed,
            skipped,
            duration_ns,
            nodes: depth_nodes,
            nps,
        });

        if options.output.show_headers() {
            let last_res = summary.by_depth.last().unwrap();
            println!(
                "Depth {depth} summary: pass={passed} fail={failed} skipped={skipped} time={} nodes={} nps={}\n",
                last_res.duration_display(),
                last_res.nodes,
                last_res.nps
            );
        }
    }

    summary.duration_ns = summary.by_depth.iter().map(|r| r.duration_ns).sum();
    let total_secs = summary.duration_ns as f64 / 1_000_000_000.0;
    summary.nps = if total_secs > 0.0 {
        (summary.nodes as f64 / total_secs) as u64
    } else {
        0
    };

    if options.output.show_headers() {
        println!("--- RESULTS ---");
        println!("Passed:  {}", summary.passed);
        println!("Failed:  {}", summary.failed);
        println!("Skipped: {}", summary.skipped);
        println!("--- TIME BY DEPTH ---");
        for result in &summary.by_depth {
            println!(
                "Depth {}: {} ({} nodes, {} NPS)",
                result.depth,
                result.duration_display(),
                result.nodes,
                result.nps
            );
        }
        println!(
            "Total:   {} ({} nodes, {} NPS)",
            summary.duration_display(),
            summary.nodes,
            summary.nps
        );
    }
    Ok(summary)
}

fn short_scenario_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn concise_expectation(expectation: &Expectation) -> String {
    match expectation {
        Expectation::Exact { direction } => direction.as_lower().to_owned(),
        Expectation::Avoid { directions } => format!(
            "not {}",
            directions
                .iter()
                .map(|direction| direction.as_lower())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn colorize_red(text: &str) -> String {
    if std::env::var_os("NO_COLOR").is_some() || !std::io::stdout().is_terminal() {
        text.to_owned()
    } else {
        format!("\x1b[31m{text}\x1b[0m")
    }
}
