use snake_ai::AiConfig;

use super::{genes::apply_genes, matchups::TrainerMatchup, types::TrainerOptions};
use crate::services::common::{MatchRunConfig, build_http_client, run_single_match_with_client};

pub(super) async fn evaluate_candidate_with_matchups(
    base_cfg: &AiConfig,
    candidate: &[f64],
    options: &TrainerOptions,
    matchups: &[TrainerMatchup],
    seed: u32,
) -> f64 {
    let candidate_cfg = apply_genes(base_cfg.clone(), candidate);
    let mut fitness = 0.0;
    let mut next_seed = seed;
    let match_cfg = MatchRunConfig {
        width: options.width,
        height: options.height,
        max_turns: options.max_turns,
        request_timeout_ms: 2000,
        payload_timeout_ms: 50,
    };
    let http_client = build_http_client(match_cfg.request_timeout_ms);

    for matchup in matchups {
        for _ in 0..matchup.games {
            let result = run_single_match_with_client(next_seed, &candidate_cfg, &matchup.opponent, &match_cfg, &http_client).await;
            next_seed = next_seed.wrapping_add(1);

            match result.winner.as_str() {
                "local" => fitness += 3.0,
                "opponent" => {}
                _ => fitness += 1.0,
            }
            fitness += (result.local_length as f64 - result.opp_length as f64) * 0.05;
            fitness += (result.turns as f64 / options.max_turns as f64) * 0.2;
        }
    }

    fitness
}
