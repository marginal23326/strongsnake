use snake_ai::AiConfig;
use snake_api::ApiFlavor;

use super::types::TrainerOptions;
use crate::services::common::{OpponentMode, resolve_opponent};

#[derive(Debug, Clone)]
pub(super) struct TrainerMatchup {
    pub label: String,
    pub opponent: OpponentMode,
    pub games: usize,
}

pub(super) fn build_trainer_matchups(base_cfg: &AiConfig, options: &TrainerOptions) -> Vec<TrainerMatchup> {
    let mut targets = if options.opponents.is_empty() {
        vec!["local-old".to_owned()]
    } else {
        options.opponents.clone()
    };

    if options.self_play {
        targets.push("local".to_owned());
    }

    targets.sort();
    targets.dedup();

    let flavor = if options.legacy_http {
        ApiFlavor::Legacy
    } else {
        options.http_api_mode
    };

    let mut out = Vec::new();
    for target in targets {
        let mode = resolve_opponent(&target, false, base_cfg, flavor);
        if options.only_http && !matches!(mode, OpponentMode::Http { .. }) {
            continue;
        }
        let games = if matches!(mode, OpponentMode::Http { .. }) {
            options.http_games.max(1)
        } else {
            options.games.max(1)
        };
        out.push(TrainerMatchup {
            label: target,
            opponent: mode,
            games,
        });
    }

    if out.is_empty() {
        out.push(TrainerMatchup {
            label: "local".to_owned(),
            opponent: OpponentMode::Local(base_cfg.clone()),
            games: options.games.max(1),
        });
    }

    out
}
