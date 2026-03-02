use std::{fmt, str::FromStr};

use snake_domain::{Direction, GameState, Point, Snake};
use thiserror::Error;

pub const SFEN_PREFIX: &str = "sfen";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnakeFen {
    pub cols: i32,
    pub rows: i32,
    pub turn: u32,
    pub seed: u32,
    pub p_health: i32,
    pub a_health: i32,
    pub p_body: Vec<Point>,
    pub a_body: Vec<Point>,
    pub foods: Vec<Point>,
    pub opponent_moves: Vec<Direction>,
}

impl SnakeFen {
    pub fn into_game_state(self) -> GameState {
        GameState {
            turn: self.turn,
            seed: u64::from(self.seed),
            board: snake_domain::Board {
                width: self.cols,
                height: self.rows,
                food: self.foods,
                snakes: vec![
                    Snake::new("s1", "Player", self.p_body, self.p_health),
                    Snake::new("s2", "AI", self.a_body, self.a_health),
                ],
            },
        }
    }

    pub fn parse(input: &str) -> Result<Self, SnakeFenError> {
        input.parse()
    }
}

impl fmt::Display for SnakeFen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let meta = format!("{}:{}:{}:{}", self.turn, self.seed, self.p_health, self.a_health);
        write!(
            f,
            "{SFEN_PREFIX} {}x{} {} {} {} {} {}",
            self.cols,
            self.rows,
            meta,
            encode_body(&self.p_body),
            encode_body(&self.a_body),
            encode_foods(&self.foods, self.cols, self.rows),
            encode_moves(&self.opponent_moves),
        )
    }
}

impl FromStr for SnakeFen {
    type Err = SnakeFenError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(SnakeFenError::EmptyInput);
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let fields = match parts.as_slice() {
            [prefix, rest @ ..] if prefix.eq_ignore_ascii_case(SFEN_PREFIX) => rest,
            [dims, meta, p_body, a_body, foods, moves] => &[*dims, *meta, *p_body, *a_body, *foods, *moves],
            _ => return Err(SnakeFenError::BadFieldCount(parts.len())),
        };

        let [dims, meta, p_body, a_body, foods, moves] = fields else {
            return Err(SnakeFenError::BadFieldCount(fields.len()));
        };

        let (cols, rows) = parse_dims(dims)?;
        if cols <= 0 || rows <= 0 {
            return Err(SnakeFenError::InvalidBoardSize(cols, rows));
        }

        let (turn, seed, p_health, a_health) = parse_meta(meta)?;

        Ok(Self {
            cols,
            rows,
            turn,
            seed,
            p_health,
            a_health,
            p_body: decode_body(p_body)?,
            a_body: decode_body(a_body)?,
            foods: decode_foods(foods, cols, rows)?,
            opponent_moves: decode_moves(moves)?,
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SnakeFenError {
    #[error("input is empty")]
    EmptyInput,
    #[error("expected 6 SFEN fields, got {0}")]
    BadFieldCount(usize),
    #[error("invalid board dimensions `{0}`")]
    InvalidDims(String),
    #[error("invalid board size {0}x{1}")]
    InvalidBoardSize(i32, i32),
    #[error("invalid meta field `{0}`")]
    InvalidMeta(String),
    #[error("invalid number `{field}` in `{value}`")]
    InvalidNumber { field: &'static str, value: String },
    #[error("invalid body segment `{0}`")]
    InvalidBody(String),
    #[error("invalid path step `{0}`")]
    InvalidPathStep(char),
    #[error("invalid food grid")]
    InvalidFoodGrid,
    #[error("invalid move token `{0}`")]
    InvalidMove(char),
    #[error("invalid run-length encoding `{0}`")]
    InvalidRle(String),
}

fn parse_dims(raw: &str) -> Result<(i32, i32), SnakeFenError> {
    let Some((cols, rows)) = raw.split_once('x') else {
        return Err(SnakeFenError::InvalidDims(raw.to_owned()));
    };
    Ok((parse_number("cols", cols)?, parse_number("rows", rows)?))
}

fn parse_meta(raw: &str) -> Result<(u32, u32, i32, i32), SnakeFenError> {
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() != 4 {
        return Err(SnakeFenError::InvalidMeta(raw.to_owned()));
    }
    Ok((
        parse_number("turn", parts[0])?,
        parse_number("seed", parts[1])?,
        parse_number("p_health", parts[2])?,
        parse_number("a_health", parts[3])?,
    ))
}

fn parse_number<T>(field: &'static str, raw: &str) -> Result<T, SnakeFenError>
where
    T: FromStr,
{
    raw.parse().map_err(|_| SnakeFenError::InvalidNumber {
        field,
        value: raw.to_owned(),
    })
}

fn encode_body(body: &[Point]) -> String {
    if body.is_empty() {
        return "-".to_owned();
    }

    let head = body[0];
    let mut path = String::with_capacity(body.len().saturating_sub(1));
    for pair in body.windows(2) {
        let prev = pair[0];
        let next = pair[1];
        let step = match (next.x - prev.x, next.y - prev.y) {
            (1, 0) => 'R',
            (-1, 0) => 'L',
            (0, 1) => 'U',
            (0, -1) => 'D',
            (0, 0) => 'S',
            _ => 'X',
        };
        path.push(step);
    }

    if path.is_empty() {
        format!("{},{}", head.x, head.y)
    } else {
        format!("{},{}:{}", head.x, head.y, rle_encode(&path))
    }
}

fn decode_body(raw: &str) -> Result<Vec<Point>, SnakeFenError> {
    if raw == "-" {
        return Ok(Vec::new());
    }

    let (head_raw, path_raw) = raw.split_once(':').map_or((raw, ""), |(head, path)| (head, path));
    let Some((x_raw, y_raw)) = head_raw.split_once(',') else {
        return Err(SnakeFenError::InvalidBody(raw.to_owned()));
    };

    let mut body = vec![Point {
        x: parse_number("body_x", x_raw)?,
        y: parse_number("body_y", y_raw)?,
    }];

    for step in rle_decode(path_raw)?.chars() {
        let prev = *body.last().expect("body always contains head");
        let next = match step {
            'U' => Point { x: prev.x, y: prev.y + 1 },
            'D' => Point { x: prev.x, y: prev.y - 1 },
            'L' => Point { x: prev.x - 1, y: prev.y },
            'R' => Point { x: prev.x + 1, y: prev.y },
            'S' => prev,
            other => return Err(SnakeFenError::InvalidPathStep(other)),
        };
        body.push(next);
    }

    Ok(body)
}

fn encode_foods(foods: &[Point], width: i32, height: i32) -> String {
    if foods.is_empty() {
        return "-".to_owned();
    }

    let mut grid = vec![false; (width * height) as usize];
    for food in foods {
        if food.x >= 0 && food.x < width && food.y >= 0 && food.y < height {
            grid[(food.y * width + food.x) as usize] = true;
        }
    }

    let mut rows = Vec::with_capacity(height as usize);
    for y in (0..height).rev() {
        let mut row = String::new();
        let mut empty_count = 0;
        for x in 0..width {
            if grid[(y * width + x) as usize] {
                if empty_count > 0 {
                    row.push_str(&empty_count.to_string());
                    empty_count = 0;
                }
                row.push('f');
            } else {
                empty_count += 1;
            }
        }
        if empty_count > 0 {
            row.push_str(&empty_count.to_string());
        }
        rows.push(row);
    }

    rows.join("/")
}

fn decode_foods(raw: &str, width: i32, height: i32) -> Result<Vec<Point>, SnakeFenError> {
    if raw == "-" {
        return Ok(Vec::new());
    }

    let rows: Vec<&str> = raw.split('/').collect();
    if rows.len() != height as usize {
        return Err(SnakeFenError::InvalidFoodGrid);
    }

    let mut foods = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let y = height - 1 - row_idx as i32;
        let mut x = 0;
        let mut digits = String::new();
        for ch in row.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                continue;
            }

            if !digits.is_empty() {
                let skip: i32 = parse_number("food_skip", &digits)?;
                if skip <= 0 {
                    return Err(SnakeFenError::InvalidFoodGrid);
                }
                x += skip;
                digits.clear();
            }

            if ch != 'f' {
                return Err(SnakeFenError::InvalidFoodGrid);
            }
            if x >= width {
                return Err(SnakeFenError::InvalidFoodGrid);
            }
            foods.push(Point { x, y });
            x += 1;
        }

        if !digits.is_empty() {
            let skip: i32 = parse_number("food_skip", &digits)?;
            if skip <= 0 {
                return Err(SnakeFenError::InvalidFoodGrid);
            }
            x += skip;
        }

        if x != width {
            return Err(SnakeFenError::InvalidFoodGrid);
        }
    }

    Ok(foods)
}

fn encode_moves(moves: &[Direction]) -> String {
    if moves.is_empty() {
        return "-".to_owned();
    }

    let chars: String = moves
        .iter()
        .map(|direction| match direction {
            Direction::Up => 'U',
            Direction::Down => 'D',
            Direction::Left => 'L',
            Direction::Right => 'R',
        })
        .collect();
    rle_encode(&chars)
}

fn decode_moves(raw: &str) -> Result<Vec<Direction>, SnakeFenError> {
    if raw == "-" {
        return Ok(Vec::new());
    }

    rle_decode(raw)?
        .chars()
        .map(|ch| match ch {
            'U' => Ok(Direction::Up),
            'D' => Ok(Direction::Down),
            'L' => Ok(Direction::Left),
            'R' => Ok(Direction::Right),
            other => Err(SnakeFenError::InvalidMove(other)),
        })
        .collect()
}

fn rle_encode(input: &str) -> String {
    let mut chars = input.chars();
    let Some(mut current) = chars.next() else {
        return String::new();
    };

    let mut count = 1usize;
    let mut out = String::new();
    for ch in chars {
        if ch == current {
            count += 1;
            continue;
        }
        if count > 1 {
            out.push_str(&count.to_string());
        }
        out.push(current);
        current = ch;
        count = 1;
    }

    if count > 1 {
        out.push_str(&count.to_string());
    }
    out.push(current);
    out
}

fn rle_decode(input: &str) -> Result<String, SnakeFenError> {
    let mut out = String::new();
    let mut count = String::new();

    for ch in input.chars() {
        if ch.is_ascii_digit() {
            count.push(ch);
            continue;
        }

        let repeat = if count.is_empty() {
            1
        } else {
            let repeat: usize = parse_number("rle_count", &count)?;
            if repeat == 0 {
                return Err(SnakeFenError::InvalidRle(input.to_owned()));
            }
            count.clear();
            repeat
        };
        out.extend(std::iter::repeat_n(ch, repeat));
    }

    if !count.is_empty() {
        return Err(SnakeFenError::InvalidRle(input.to_owned()));
    }

    Ok(out)
}
