use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Result;
use snake_ai::AiConfig;

use super::super::common::{
    MatchResult, MatchRunConfig, MatchRunOutput, MatchRuntimeOptions, MatchTraceFrame, resolve_opponent, run_single_match,
    run_single_match_with_options,
};
use super::snapshot::{build_snapshot, display_path, load_snapshot, quote_for_cli, snapshot_path_for_mode, write_snapshot};
use super::stats::{ArenaAccumulator, ArenaSummaryInput};
use super::types::{ArenaFindMode, ArenaFindResult, ArenaOptions, ArenaProgress, ArenaSummary};

#[derive(Debug, Clone)]
struct FindCandidate {
    result: MatchResult,
}

pub async fn run_arena_with_progress<F>(cfg: AiConfig, options: ArenaOptions, mut on_progress: F) -> Result<ArenaSummary>
where
    F: FnMut(ArenaProgress),
{
    let started = Instant::now();
    let opponent = resolve_opponent(&options.opponent, options.self_play, &cfg, options.api_flavor);

    let match_cfg = MatchRunConfig {
        width: options.width,
        height: options.height,
        max_turns: options.max_turns,
        request_timeout_ms: options.request_timeout_ms,
        payload_timeout_ms: options.payload_timeout_ms,
    };
    let loaded_snapshot = if options.resume {
        Some(load_snapshot(&options.snapshot_file, options.width, options.height)?)
    } else {
        None
    };
    let total_games = if loaded_snapshot.is_some() { 1usize } else { options.games.max(1) };
    let capture_during_run = !options.find_modes.is_empty();

    let mut acc = ArenaAccumulator::default();
    let mut results = Vec::with_capacity(total_games);
    let mut find_candidates: HashMap<ArenaFindMode, FindCandidate> = HashMap::new();
    let mut find_traces: HashMap<u32, Arc<Vec<MatchTraceFrame>>> = HashMap::new();

    for i in 0..total_games {
        let (seed, runtime_options) = if i == 0 {
            if let Some(snapshot) = loaded_snapshot.clone() {
                (
                    snapshot.rng_seed,
                    MatchRuntimeOptions {
                        initial_state: Some(snapshot.state),
                        rng_seed: Some(snapshot.rng_seed),
                        scripted_opponent_moves: snapshot.opponent_moves,
                        capture_trace: capture_during_run,
                    },
                )
            } else {
                (
                    options.seed.wrapping_add(i as u32),
                    MatchRuntimeOptions {
                        capture_trace: capture_during_run,
                        ..Default::default()
                    },
                )
            }
        } else {
            (
                options.seed.wrapping_add(i as u32),
                MatchRuntimeOptions {
                    capture_trace: capture_during_run,
                    ..Default::default()
                },
            )
        };

        let output = if runtime_options.initial_state.is_some() || runtime_options.capture_trace {
            run_single_match_with_options(seed, &cfg, &opponent, &match_cfg, runtime_options).await
        } else {
            let result = run_single_match(seed, &cfg, &opponent, &match_cfg).await;
            MatchRunOutput { result, trace: None }
        };

        let result = output.result.clone();
        if let Some(trace) = output.trace {
            find_traces.insert(result.seed, Arc::new(trace));
        }
        acc.record_result(&result);
        on_progress(acc.progress_snapshot(i + 1, total_games, started.elapsed().as_millis(), &result));

        if !options.find_modes.is_empty() && (!options.only_loss || result.local_died) {
            for mode in &options.find_modes {
                let should_replace = find_candidates
                    .get(mode)
                    .is_none_or(|current| mode.better(&result, &current.result));
                if should_replace {
                    find_candidates.insert(*mode, FindCandidate { result: result.clone() });
                }
            }
        }

        results.push(result);
    }

    let mut find_results = Vec::new();
    for mode in &options.find_modes {
        let Some(candidate) = find_candidates.get(mode).cloned() else {
            continue;
        };

        let snapshot_path = snapshot_path_for_mode(&options.snapshot_file, *mode, options.find_modes.len() > 1);
        let snapshot_file = if let Some(trace) = find_traces.get(&candidate.result.seed) {
            if let Some(snapshot) = build_snapshot(&trace, options.snapshot_ticks, options.width, options.height) {
                write_snapshot(&snapshot_path, &snapshot)?;
                Some(display_path(&snapshot_path))
            } else {
                None
            }
        } else {
            None
        };

        let resume_hint = snapshot_file.as_ref().map(|_| {
            let shown = display_path(&snapshot_path);
            format!("snake-app arena --resume --snapshot-file {}", quote_for_cli(&shown))
        });
        find_results.push(ArenaFindResult {
            mode: *mode,
            mode_title: mode.title().to_owned(),
            seed: candidate.result.seed,
            winner: candidate.result.winner.clone(),
            turns: candidate.result.turns,
            local_length: candidate.result.local_length,
            metric_label: mode.metric_label(&candidate.result),
            snapshot_file,
            reproduce_hint: format!(
                "snake-app arena --games 1 --seed {} --opponent {}",
                candidate.result.seed,
                quote_for_cli(&options.opponent)
            ),
            resume_hint,
        });
    }

    Ok(acc.into_summary(ArenaSummaryInput {
        total_games,
        duration_ms: started.elapsed().as_millis(),
        resumed_from_snapshot: loaded_snapshot.is_some(),
        snapshot_file: loaded_snapshot.as_ref().map(|_| display_path(&options.snapshot_file)),
        invalid_find_modes: options.invalid_find_modes.clone(),
        find_results,
        results,
    }))
}
