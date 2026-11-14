//! Paired games: one seed played twice with the sides swapped, so board and luck are the same for both engines.
//!
//! The engine's transposition table is process-global, so games cannot run in parallel inside one process. Instead the
//! trainer starts worker processes (`snake-app trainer-worker`), each playing a batch of pairs in sequence.

use std::io::Read;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use snake_ai::AiConfig;

use crate::services::common::{HttpClient, MatchRunConfig, MatchRuntimeOptions, OpponentMode, build_http_client, run_single_match_with_options_and_client};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) struct PairSpec {
    pub seed: u32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PairJob {
    pub candidate: AiConfig,
    pub reference: AiConfig,
    pub max_turns: u32,
    pub pairs: Vec<PairSpec>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) struct PairResult {
    /// Mean of the two games (win 1, draw 0.5, loss 0).
    pub score: f64,
    /// Games of the pair that hit the turn cap.
    pub capped: u8,
}

/// Marks the result line on a worker's stdout, apart from any log lines.
pub(super) const RESULT_PREFIX: &str = "RESULT ";

/// `snake-app trainer-worker`: reads a job as JSON on stdin, prints the results as one line.
pub fn run_worker() -> Result<()> {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).context("reading the job from stdin")?;
    let job: PairJob = serde_json::from_str(&raw).context("parsing the job")?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let results = rt.block_on(play_all(&job));
    println!("{RESULT_PREFIX}{}", serde_json::to_string(&results)?);
    Ok(())
}

async fn play_all(job: &PairJob) -> Vec<PairResult> {
    let client = build_http_client(1000);
    let mut results = Vec::with_capacity(job.pairs.len());
    for spec in &job.pairs {
        results.push(play_pair(job, spec, &client).await);
    }
    results
}

async fn play_pair(job: &PairJob, spec: &PairSpec, client: &HttpClient) -> PairResult {
    let match_cfg = MatchRunConfig {
        width: spec.width,
        height: spec.height,
        max_turns: job.max_turns,
        request_timeout_ms: 1000,
        payload_timeout_ms: 50,
    };
    let opponent = OpponentMode::Local(job.reference.clone());
    let mut score = 0.0;
    let mut capped = 0u8;
    for swap_sides in [false, true] {
        let options = MatchRuntimeOptions {
            swap_sides,
            ..Default::default()
        };
        let result = run_single_match_with_options_and_client(spec.seed, &job.candidate, &opponent, &match_cfg, options, client)
            .await
            .result;
        score += match result.winner.as_str() {
            "local" => 1.0,
            "opponent" => 0.0,
            _ => 0.5,
        };
        capped += (result.turns >= job.max_turns) as u8;
    }
    PairResult { score: score / 2.0, capped }
}
