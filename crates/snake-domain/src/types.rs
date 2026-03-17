use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::direction::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    #[inline]
    pub const fn moved(self, dir: Direction) -> Self {
        let (dx, dy) = dir.vector();
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnakeId(pub String);

impl From<&str> for SnakeId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snake {
    pub id: SnakeId,
    pub name: String,
    pub body: VecDeque<Point>,
    pub health: i32,
    #[serde(default)]
    pub alive: bool,
}

impl Snake {
    #[inline]
    pub fn new(id: impl Into<SnakeId>, name: impl Into<String>, body: impl Into<VecDeque<Point>>, health: i32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            body: body.into(),
            health,
            alive: true,
        }
    }

    #[inline]
    pub fn head(&self) -> Option<Point> {
        self.body.front().copied()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.body.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.body.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub width: i32,
    pub height: i32,
    pub food: Vec<Point>,
    pub snakes: Vec<Snake>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub turn: u32,
    pub board: Board,
    #[serde(default)]
    pub seed: u64,
}
