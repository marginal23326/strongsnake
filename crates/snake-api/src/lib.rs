use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use snake_domain::{Direction, GameState, Point, Snake};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiFlavor {
    Auto,
    Standard,
    Legacy,
}

#[derive(Debug, Clone)]
pub struct ParsedMoveRequest {
    pub turn: i32,
    pub width: i32,
    pub height: i32,
    pub food: Vec<Point>,
    pub snakes: Vec<Snake>,
    pub you_id: String,
}

pub fn normalize_api_type(value: Option<&str>) -> ApiFlavor {
    match value.unwrap_or("legacy").trim().to_ascii_lowercase().as_str() {
        "standard" => ApiFlavor::Standard,
        "legacy" => ApiFlavor::Legacy,
        "auto" => ApiFlavor::Auto,
        _ => ApiFlavor::Legacy,
    }
}

pub fn normalize_move_name(value: &str) -> Option<Direction> {
    match value.trim().to_ascii_lowercase().as_str() {
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        "left" => Some(Direction::Left),
        "right" => Some(Direction::Right),
        _ => None,
    }
}

pub fn parse_move_request(body: &Value) -> Result<ParsedMoveRequest> {
    if body.get("board").is_some() && body.get("you").is_some() {
        parse_standard(body)
    } else {
        parse_legacy(body)
    }
}

fn parse_standard(body: &Value) -> Result<ParsedMoveRequest> {
    let turn = body.get("turn").and_then(Value::as_i64).unwrap_or(0) as i32;
    let board = body.get("board").context("missing board")?;
    let width = board.get("width").and_then(Value::as_i64).context("missing board.width")? as i32;
    let height = board.get("height").and_then(Value::as_i64).context("missing board.height")? as i32;
    let food = parse_points(board.get("food").unwrap_or(&Value::Array(vec![])));
    let snakes = parse_standard_snakes(board.get("snakes").unwrap_or(&Value::Array(vec![])));
    let you_id = body
        .get("you")
        .and_then(|v| v.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("you")
        .to_owned();

    Ok(ParsedMoveRequest {
        turn,
        width,
        height,
        food,
        snakes,
        you_id,
    })
}

fn parse_legacy(body: &Value) -> Result<ParsedMoveRequest> {
    let turn = body.get("turn").and_then(Value::as_i64).unwrap_or(0) as i32;
    let width = body
        .get("width")
        .and_then(Value::as_i64)
        .or_else(|| body.get("board").and_then(|b| b.get("width")).and_then(Value::as_i64))
        .unwrap_or(15) as i32;
    let height = body
        .get("height")
        .and_then(Value::as_i64)
        .or_else(|| body.get("board").and_then(|b| b.get("height")).and_then(Value::as_i64))
        .unwrap_or(15) as i32;

    let food_node = body.get("food").unwrap_or(&Value::Null);
    let snakes_node = body.get("snakes").unwrap_or(&Value::Null);
    let you_node = body.get("you").unwrap_or(&Value::Null);

    let food_points = parse_points(&clean_list(food_node));
    let snakes = parse_legacy_snakes(&clean_list(snakes_node), height);
    let you_id = clean_object(you_node).get("id").and_then(Value::as_str).unwrap_or("you").to_owned();

    Ok(ParsedMoveRequest {
        turn,
        width,
        height,
        food: food_points,
        snakes,
        you_id,
    })
}

fn parse_standard_snakes(node: &Value) -> Vec<Snake> {
    node.as_array()
        .into_iter()
        .flatten()
        .map(|s| Snake {
            id: snake_domain::SnakeId(s.get("id").and_then(Value::as_str).unwrap_or("s").to_owned()),
            name: s.get("name").and_then(Value::as_str).unwrap_or("snake").to_owned(),
            body: parse_points(s.get("body").unwrap_or(&Value::Array(vec![]))).into(),
            health: s.get("health").and_then(Value::as_i64).unwrap_or(100) as i32,
            alive: true,
        })
        .collect()
}

fn parse_legacy_snakes(node: &Value, height: i32) -> Vec<Snake> {
    node.as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let obj = clean_object(s);
            let body_raw = clean_list(obj.get("body").unwrap_or(&Value::Null));
            let body: Vec<Point> = parse_points(&body_raw)
                .into_iter()
                .map(|p| Point {
                    x: p.x,
                    y: invert_y(p.y, height),
                })
                .collect();
            Snake {
                id: snake_domain::SnakeId(obj.get("id").and_then(Value::as_str).unwrap_or("s").to_owned()),
                name: obj.get("name").and_then(Value::as_str).unwrap_or("snake").to_owned(),
                body: body.into(),
                health: obj.get("health").or_else(|| obj.get("hp")).and_then(Value::as_i64).unwrap_or(100) as i32,
                alive: true,
            }
        })
        .collect()
}

fn parse_points(node: &Value) -> Vec<Point> {
    node.as_array()
        .into_iter()
        .flatten()
        .map(clean_object)
        .map(|obj| Point {
            x: obj.get("x").and_then(Value::as_i64).unwrap_or(0) as i32,
            y: obj.get("y").and_then(Value::as_i64).unwrap_or(0) as i32,
        })
        .collect()
}

fn clean_list(node: &Value) -> Value {
    if let Some(data) = node.get("data")
        && data.is_array()
    {
        return data.clone();
    }
    node.clone()
}

fn clean_object(node: &Value) -> Value {
    if let Some(data) = node.get("data")
        && data.is_object()
    {
        return data.clone();
    }
    node.clone()
}

pub fn build_move_payload(state: &GameState, you_id: &str, flavor: ApiFlavor, game_id: &str, timeout: u32) -> Result<Value> {
    match flavor {
        ApiFlavor::Standard => Ok(build_standard_payload(state, you_id, game_id, timeout)),
        ApiFlavor::Legacy | ApiFlavor::Auto => Ok(build_legacy_payload(state, you_id, game_id)),
    }
}

fn format_snake_for_standard(snake: &Snake) -> Value {
    let body = snake.body.iter().map(|p| json!({ "x": p.x, "y": p.y })).collect::<Vec<_>>();
    let head = snake.body.front().copied().unwrap_or(Point { x: 0, y: 0 });
    json!({
        "id": snake.id.0,
        "name": snake.name,
        "health": snake.health,
        "body": body,
        "head": { "x": head.x, "y": head.y },
        "length": snake.body.len(),
        "latency": "100",
        "shout": ""
    })
}

fn build_standard_payload(state: &GameState, you_id: &str, game_id: &str, timeout: u32) -> Value {
    let snakes = state.board.snakes.iter().map(format_snake_for_standard).collect::<Vec<_>>();
    let you = state
        .board
        .snakes
        .iter()
        .find(|s| s.id.0 == you_id)
        .map(format_snake_for_standard)
        .unwrap_or_else(|| json!({ "id": you_id, "body": [] }));

    json!({
        "game": {
            "id": game_id,
            "ruleset": { "name": "standard", "version": "v1.2.3" },
            "map": "standard",
            "timeout": timeout,
            "source": "rust-arena"
        },
        "turn": state.turn,
        "board": {
            "height": state.board.height,
            "width": state.board.width,
            "food": state.board.food.iter().map(|f| json!({"x":f.x, "y":f.y})).collect::<Vec<_>>(),
            "hazards": [],
            "snakes": snakes
        },
        "you": you
    })
}

fn build_legacy_payload(state: &GameState, you_id: &str, game_id: &str) -> Value {
    let to_legacy_point = |p: &Point| json!({"object": "point", "x": p.x, "y": invert_y(p.y, state.board.height)});
    let snakes = state
        .board
        .snakes
        .iter()
        .map(|s| {
            json!({
                "object": "snake",
                "id": s.id.0,
                "name": s.name,
                "health": s.health,
                "body": {
                    "object":"list",
                    "data": s.body.iter().map(to_legacy_point).collect::<Vec<_>>()
                }
            })
        })
        .collect::<Vec<_>>();

    let you = state
        .board
        .snakes
        .iter()
        .find(|s| s.id.0 == you_id)
        .ok_or_else(|| anyhow!("you id not found"))
        .map(|s| {
            json!({
                "object": "snake",
                "id": s.id.0,
                "name": s.name,
                "health": s.health,
                "body": {
                    "object":"list",
                    "data": s.body.iter().map(to_legacy_point).collect::<Vec<_>>()
                }
            })
        })
        .unwrap_or_else(|_| json!({}));

    json!({
        "object": "world",
        "id": game_id,
        "width": state.board.width,
        "height": state.board.height,
        "turn": state.turn,
        "food": {
            "object":"list",
            "data": state.board.food.iter().map(to_legacy_point).collect::<Vec<_>>()
        },
        "snakes": {
            "object":"list",
            "data": snakes
        },
        "you": you
    })
}

#[inline]
pub fn invert_y(y: i32, height: i32) -> i32 {
    height - 1 - y
}
