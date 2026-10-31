use std::collections::{HashMap, VecDeque};

use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use snake_ai::{AgentState, AiConfig, decide_move};
use snake_api::{ApiFlavor, build_move_payload, normalize_move_name};
use snake_domain::{Direction, FoodSettings, GameState, LcgRng, Point, SimConfig, Snake, place_initial_standard_food, simulate_turn};
use snake_io::{AI_ID, PLAYER_ID};

#[derive(Debug, Clone, Serialize)]
pub struct MatchResult {
    pub seed: u32,
    pub turns: u32,
    pub winner: String,
    pub local_length: usize,
    pub opp_length: usize,
    pub death: MatchDeathSummary,
    pub local_died: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MatchDeathSummary {
    pub local: DeathReasonCounts,
    pub opponent: DeathReasonCounts,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeathReasonCounts {
    pub starvation: usize,
    pub wall: usize,
    pub body: usize,
    pub head: usize,
}

impl DeathReasonCounts {
    pub fn record(&mut self, reason: &str) {
        match reason {
            "Starvation" => self.starvation += 1,
            "Wall" => self.wall += 1,
            "Body" => self.body += 1,
            "Head" => self.head += 1,
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum OpponentMode {
    Local(AiConfig),
    Http { url: String, flavor: ApiFlavor },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MatchRunConfig {
    pub width: i32,
    pub height: i32,
    pub max_turns: u32,
    pub request_timeout_ms: u64,
    pub payload_timeout_ms: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MatchRuntimeOptions {
    pub initial_state: Option<GameState>,
    pub rng_seed: Option<u32>,
    pub scripted_opponent_moves: VecDeque<Direction>,
    pub capture_trace: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MatchTraceFrame {
    pub rng_seed: u32,
    pub state: GameState,
    pub opponent_move: Direction,
}

#[derive(Debug, Clone)]
pub(crate) struct MatchRunOutput {
    pub result: MatchResult,
    pub trace: Option<Vec<MatchTraceFrame>>,
}

fn default_opponent_roster() -> HashMap<&'static str, (&'static str, ApiFlavor)> {
    HashMap::from([
        ("local-old", ("http://localhost:9000", ApiFlavor::Standard)),
        ("shapeshifter", ("http://localhost:8080", ApiFlavor::Standard)),
        ("snek-two", ("http://localhost:7000", ApiFlavor::Legacy)),
    ])
}

pub fn format_opponent_roster() -> String {
    let mut rows: Vec<(&str, &str, &str)> = default_opponent_roster()
        .into_iter()
        .map(|(name, (url, flavor))| {
            let api = match flavor {
                ApiFlavor::Standard => "standard",
                ApiFlavor::Legacy => "legacy",
                ApiFlavor::Auto => "auto",
            };
            (name, api, url)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));

    let name_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max("Name".len());
    let api_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(3).max("API".len());

    let mut out = String::new();
    out.push_str("Available opponents:\n");
    out.push_str(&format!(
        "{:<name_w$}  {:<api_w$}  {}\n",
        "Name",
        "API",
        "URL",
        name_w = name_w,
        api_w = api_w
    ));
    out.push_str(&format!(
        "{:-<name_w$}  {:-<api_w$}  {:-<40}\n",
        "",
        "",
        "",
        name_w = name_w,
        api_w = api_w
    ));
    for (name, api, url) in rows {
        out.push_str(&format!(
            "{:<name_w$}  {:<api_w$}  {}\n",
            name,
            api,
            url,
            name_w = name_w,
            api_w = api_w
        ));
    }
    out
}

pub(crate) fn resolve_opponent(opponent: &str, self_play: bool, fallback_cfg: &AiConfig, flavor: ApiFlavor) -> OpponentMode {
    if self_play {
        return OpponentMode::Local(fallback_cfg.clone());
    }
    if opponent.starts_with("http://") || opponent.starts_with("https://") {
        return OpponentMode::Http {
            url: opponent.to_owned(),
            flavor,
        };
    }

    if opponent.eq_ignore_ascii_case("local") {
        return OpponentMode::Local(fallback_cfg.clone());
    }

    if let Some((url, roster_flavor)) = default_opponent_roster().get(opponent) {
        return OpponentMode::Http {
            url: (*url).to_owned(),
            flavor: match flavor {
                ApiFlavor::Auto => *roster_flavor,
                other => other,
            },
        };
    }

    OpponentMode::Local(fallback_cfg.clone())
}

fn build_initial_state(width: i32, height: i32, seed: u32) -> (GameState, LcgRng) {
    let mut rng = LcgRng::new(seed);
    let pad = 2;
    let mut state = GameState {
        turn: 0,
        seed: seed as u64,
        board: snake_domain::Board {
            width,
            height,
            food: Vec::new(),
            snakes: vec![
                Snake::new(
                    "local",
                    "local",
                    vec![Point { x: pad, y: pad }, Point { x: pad, y: pad - 1 }, Point { x: pad, y: pad - 2 }],
                    100,
                ),
                Snake::new(
                    "opponent",
                    "opponent",
                    vec![
                        Point {
                            x: width - pad - 1,
                            y: height - pad - 1,
                        },
                        Point {
                            x: width - pad - 1,
                            y: height - pad,
                        },
                        Point {
                            x: width - pad - 1,
                            y: height - pad + 1,
                        },
                    ],
                    100,
                ),
            ],
        },
    };
    let food_settings = FoodSettings::default();
    let board_width = state.board.width;
    let board_height = state.board.height;
    place_initial_standard_food(
        &mut rng,
        board_width,
        board_height,
        &state.board.snakes,
        &mut state.board.food,
        food_settings,
    );
    (state, rng)
}

pub fn build_playground_state(width: i32, height: i32, seed: u32) -> (GameState, LcgRng) {
    let (mut state, rng) = build_initial_state(width, height, seed);
    for (snake, (id, name)) in state.board.snakes.iter_mut().zip([(PLAYER_ID, "Player"), (AI_ID, "AI")]) {
        snake.id = id.into();
        snake.name = name.to_owned();
    }
    (state, rng)
}

fn resolve_match_snake_indices(state: &GameState) -> (Option<usize>, Option<usize>) {
    let mut local_idx = None;
    let mut opponent_idx = None;

    for (idx, snake) in state.board.snakes.iter().enumerate() {
        match snake.id.0.as_str() {
            "local" if local_idx.is_none() => local_idx = Some(idx),
            "opponent" if opponent_idx.is_none() => opponent_idx = Some(idx),
            _ => {}
        }
        if local_idx.is_some() && opponent_idx.is_some() {
            break;
        }
    }

    let local_idx = local_idx.or((!state.board.snakes.is_empty()).then_some(0));
    let opponent_idx = opponent_idx.or_else(|| {
        state
            .board
            .snakes
            .iter()
            .enumerate()
            .find(|(idx, _)| Some(*idx) != local_idx)
            .map(|(idx, _)| idx)
    });

    (local_idx, opponent_idx)
}

fn extract_agent_pair(state: &GameState, me_idx: Option<usize>, enemy_idx: Option<usize>) -> (AgentState, AgentState) {
    let me = me_idx.and_then(|idx| state.board.snakes.get(idx));
    let enemy = enemy_idx.and_then(|idx| state.board.snakes.get(idx));
    (
        AgentState {
            body: snake_ai::model::FastBody::from_points(me.into_iter().flat_map(|snake| snake.body.iter().copied())),
            health: me.map(|snake| snake.health).unwrap_or(0),
        },
        AgentState {
            body: snake_ai::model::FastBody::from_points(enemy.into_iter().flat_map(|snake| snake.body.iter().copied())),
            health: enemy.map(|snake| snake.health).unwrap_or(0),
        },
    )
}

async fn request_http_move(
    client: &Client,
    url: &str,
    state: &GameState,
    snake_id: &str,
    flavor: ApiFlavor,
    payload_timeout_ms: u32,
) -> Direction {
    let payload = match build_move_payload(state, snake_id, flavor, "rust-arena", payload_timeout_ms) {
        Ok(v) => v,
        Err(_) => return Direction::Up,
    };
    let endpoint = format!("{}/move", url.trim_end_matches('/'));
    let Ok(resp) = client.post(endpoint).json(&payload).send().await else {
        return Direction::Up;
    };
    let Ok(json) = resp.json::<Value>().await else {
        return Direction::Up;
    };
    json.get("move")
        .and_then(Value::as_str)
        .and_then(normalize_move_name)
        .unwrap_or(Direction::Up)
}

pub(crate) fn build_http_client(timeout_ms: u64) -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_millis(timeout_ms))
        .build()
        .expect("client build")
}

pub(crate) async fn run_single_match_with_client(
    seed: u32,
    cfg: &AiConfig,
    opponent: &OpponentMode,
    match_cfg: &MatchRunConfig,
    client: &Client,
) -> MatchResult {
    run_single_match_with_options_and_client(seed, cfg, opponent, match_cfg, MatchRuntimeOptions::default(), client)
        .await
        .result
}

pub(crate) async fn run_single_match_with_options_and_client(
    seed: u32,
    cfg: &AiConfig,
    opponent: &OpponentMode,
    match_cfg: &MatchRunConfig,
    options: MatchRuntimeOptions,
    client: &Client,
) -> MatchRunOutput {
    let mut scripted_opponent_moves = options.scripted_opponent_moves;
    let (mut state, mut rng) = if let Some(initial_state) = options.initial_state {
        let rng_seed = options.rng_seed.unwrap_or(seed);
        (initial_state, LcgRng::new(rng_seed))
    } else {
        let rng_seed = options.rng_seed.unwrap_or(seed);
        build_initial_state(match_cfg.width, match_cfg.height, rng_seed)
    };
    let mut death = MatchDeathSummary::default();
    let mut trace = options.capture_trace.then(Vec::new);
    let (local_idx, opponent_idx) = resolve_match_snake_indices(&state);
    let mut intents = vec![Direction::Up; state.board.snakes.len()];

    let sim_cfg = SimConfig::default();

    while state.turn < match_cfg.max_turns {
        let alive = state.board.snakes.iter().filter(|s| s.alive && !s.body.is_empty()).count();
        if alive <= 1 {
            break;
        }

        let (me_local, enemy_local) = extract_agent_pair(&state, local_idx, opponent_idx);
        let dir_local = decide_move(me_local, enemy_local, &state.board.food, state.board.width, state.board.height, cfg).best_move;

        let dir_opponent = if let Some(scripted_move) = scripted_opponent_moves.pop_front() {
            scripted_move
        } else {
            match opponent {
                OpponentMode::Local(opp_cfg) => {
                    let (me_opp, enemy_opp) = extract_agent_pair(&state, opponent_idx, local_idx);
                    decide_move(me_opp, enemy_opp, &state.board.food, state.board.width, state.board.height, opp_cfg).best_move
                }
                OpponentMode::Http { url, flavor } => {
                    let opponent_net_id = opponent_idx
                        .and_then(|idx| state.board.snakes.get(idx))
                        .map(|snake| snake.id.0.as_str())
                        .unwrap_or("opponent");
                    request_http_move(client, url, &state, opponent_net_id, *flavor, match_cfg.payload_timeout_ms).await
                }
            }
        };

        if let Some(frames) = trace.as_mut() {
            frames.push(MatchTraceFrame {
                rng_seed: rng.state(),
                state: state.clone(),
                opponent_move: dir_opponent,
            });
        }

        intents.fill(Direction::Up);
        if let Some(idx) = local_idx
            && idx < intents.len()
        {
            intents[idx] = dir_local;
        }
        if let Some(idx) = opponent_idx
            && idx < intents.len()
        {
            intents[idx] = dir_opponent;
        }

        let turn_summary = simulate_turn(&mut state, &intents, &mut rng, sim_cfg);

        let local_net_id = local_idx
            .and_then(|idx| state.board.snakes.get(idx))
            .map(|snake| snake.id.0.as_str());
        let opponent_net_id = opponent_idx
            .and_then(|idx| state.board.snakes.get(idx))
            .map(|snake| snake.id.0.as_str());
        for event in &turn_summary.dead {
            if local_net_id.is_some_and(|id| event.snake_id.0 == id) {
                death.local.record(&event.reason);
            } else if opponent_net_id.is_some_and(|id| event.snake_id.0 == id) {
                death.opponent.record(&event.reason);
            }
        }
    }

    let local_snake = local_idx.and_then(|idx| state.board.snakes.get(idx));
    let opponent_snake = opponent_idx.and_then(|idx| state.board.snakes.get(idx));
    let local_alive = local_snake.is_some_and(|snake| snake.alive);
    let opponent_alive = opponent_snake.is_some_and(|snake| snake.alive);
    let local_len = local_snake.map_or(0, |snake| snake.len());
    let opponent_len = opponent_snake.map_or(0, |snake| snake.len());

    let winner = if local_alive && !opponent_alive {
        "local".to_owned()
    } else if opponent_alive && !local_alive {
        "opponent".to_owned()
    } else if local_alive && opponent_alive {
        if local_len > opponent_len {
            "local".to_owned()
        } else if opponent_len > local_len {
            "opponent".to_owned()
        } else {
            "draw".to_owned()
        }
    } else {
        "draw".to_owned()
    };

    let result = MatchResult {
        seed,
        turns: state.turn,
        winner,
        local_length: local_len,
        opp_length: opponent_len,
        death,
        local_died: !local_alive,
    };

    MatchRunOutput { result, trace }
}
