use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use snake_ai::model::FastBody;
use snake_ai::{AgentState, AiConfig, decide_move};
use snake_api::parse_move_request;
use snake_domain::Direction;
use tokio::{net::TcpListener, sync::RwLock};
use tracing::info;

#[derive(Clone)]
pub struct ServerState {
    pub config: Arc<RwLock<AiConfig>>,
}

pub async fn run_server(addr: SocketAddr, config: AiConfig) -> Result<()> {
    run_server_with_shutdown(addr, config, async { std::future::pending::<()>().await }).await
}

pub async fn run_server_with_shutdown<F>(addr: SocketAddr, config: AiConfig, shutdown: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let state = ServerState {
        config: Arc::new(RwLock::new(config)),
    };

    let app = Router::new()
        .route("/move", post(handle_move))
        .route("/start", post(handle_empty))
        .route("/end", post(handle_empty))
        .route("/configure", post(handle_configure))
        .route("/config", get(handle_get_config))
        .with_state(state);

    let listener = TcpListener::bind(addr).await?;
    info!("Rust snake server listening on {}", addr);
    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    Ok(())
}

async fn handle_empty() -> impl IntoResponse {
    Json(json!({}))
}

async fn handle_get_config(State(state): State<ServerState>) -> impl IntoResponse {
    let cfg = state.config.read().await.clone();
    Json(json!(cfg))
}

async fn handle_configure(State(state): State<ServerState>, Json(update): Json<Value>) -> impl IntoResponse {
    let mut cfg_guard = state.config.write().await;
    let mut current = serde_json::to_value(cfg_guard.clone()).unwrap_or_else(|_| json!({}));
    merge_json(&mut current, &update);
    let next: AiConfig = serde_json::from_value(current).unwrap_or_else(|_| cfg_guard.clone());
    *cfg_guard = next.clone();
    Json(json!({ "status": "ok", "config": next }))
}

async fn handle_move(State(state): State<ServerState>, Json(body): Json<Value>) -> impl IntoResponse {
    let parsed = match parse_move_request(&body) {
        Ok(v) => v,
        Err(_) => {
            return Json(json!({ "move": "up" }));
        }
    };

    let you = parsed.snakes.iter().find(|s| s.id.0 == parsed.you_id);
    let enemy = parsed.snakes.iter().find(|s| s.id.0 != parsed.you_id);
    let Some(you) = you else {
        return Json(json!({ "move": "up" }));
    };

    let cfg = state.config.read().await.clone();
    let decision = decide_move(
        AgentState {
            body: FastBody::from_points(you.body.iter().copied()),
            health: you.health,
        },
        AgentState {
            body: FastBody::from_points(enemy.into_iter().flat_map(|e| e.body.iter().copied())),
            health: enemy.map(|e| e.health).unwrap_or(0),
        },
        &parsed.food,
        parsed.width,
        parsed.height,
        &cfg,
    );

    let mv = match decision.best_move {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    };

    Json(json!({ "move": mv }))
}

fn merge_json(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            for (key, source_value) in source_map {
                match target_map.get_mut(key) {
                    Some(target_value) => merge_json(target_value, source_value),
                    None => {
                        target_map.insert(key.clone(), source_value.clone());
                    }
                }
            }
        }
        (target, source) => {
            *target = source.clone();
        }
    }
}
