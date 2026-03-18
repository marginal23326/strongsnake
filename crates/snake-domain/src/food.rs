use serde::{Deserialize, Serialize};

use crate::{Point, Snake, rng::RngSource};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FoodSettings {
    pub initial_food: usize,
    pub minimum_food: usize,
    pub food_spawn_chance: usize,
}

impl Default for FoodSettings {
    fn default() -> Self {
        Self {
            initial_food: 3,
            minimum_food: 1,
            food_spawn_chance: 15,
        }
    }
}

#[inline]
fn in_bounds(width: i32, height: i32, point: Point) -> bool {
    point.x >= 0 && point.x < width && point.y >= 0 && point.y < height
}

#[inline]
fn point_idx(width: i32, point: Point) -> usize {
    (point.y * width + point.x) as usize
}

fn build_occupied_map(width: i32, height: i32, snakes: &[Snake], food: &[Point]) -> Vec<bool> {
    let area = (width.max(0) * height.max(0)) as usize;
    let mut occupied = vec![false; area];

    for snake in snakes {
        for &part in &snake.body {
            if in_bounds(width, height, part) {
                occupied[point_idx(width, part)] = true;
            }
        }
    }

    for &item in food {
        if in_bounds(width, height, item) {
            occupied[point_idx(width, item)] = true;
        }
    }

    occupied
}

fn sample_single_unoccupied<R: RngSource>(rng: &mut R, width: i32, height: i32, occupied: &[bool]) -> Option<Point> {
    let mut seen = 0usize;
    let mut selected = None;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if occupied[idx] {
                continue;
            }

            seen += 1;
            if rng.rand_int(seen) == 0 {
                selected = Some(Point { x, y });
            }
        }
    }

    selected
}

fn collect_unoccupied_points(width: i32, height: i32, occupied: &[bool]) -> Vec<Point> {
    let mut points = Vec::new();
    points.reserve(occupied.len());

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if !occupied[idx] {
                points.push(Point { x, y });
            }
        }
    }

    points
}

fn shuffle_in_place<R: RngSource>(arr: &mut [Point], rng: &mut R) {
    if arr.len() <= 1 {
        return;
    }
    for i in (1..arr.len()).rev() {
        let j = rng.rand_int(i + 1);
        arr.swap(i, j);
    }
}

fn check_food_needing_placement<R: RngSource>(rng: &mut R, settings: FoodSettings, current_food_count: usize) -> usize {
    if current_food_count < settings.minimum_food {
        return settings.minimum_food - current_food_count;
    }

    if settings.food_spawn_chance > 0 && (100usize.saturating_sub(rng.rand_int(100))) < settings.food_spawn_chance {
        return 1;
    }

    0
}

fn place_food_randomly_at_positions<R: RngSource>(rng: &mut R, food: &mut Vec<Point>, count: usize, positions: &mut [Point]) -> usize {
    let n = count.min(positions.len());
    if n == 0 {
        return 0;
    }
    shuffle_in_place(positions, rng);
    food.extend_from_slice(&positions[..n]);
    n
}

fn place_food_randomly<R: RngSource>(rng: &mut R, width: i32, height: i32, snakes: &[Snake], food: &mut Vec<Point>, count: usize) -> usize {
    if count == 0 || width <= 0 || height <= 0 {
        return 0;
    }

    let occupied = build_occupied_map(width, height, snakes, food);
    if count == 1 {
        if let Some(point) = sample_single_unoccupied(rng, width, height, &occupied) {
            food.push(point);
            return 1;
        }
        return 0;
    }

    let mut unoccupied = collect_unoccupied_points(width, height, &occupied);
    place_food_randomly_at_positions(rng, food, count, &mut unoccupied)
}

pub fn apply_standard_food_spawning<R: RngSource>(
    rng: &mut R,
    width: i32,
    height: i32,
    snakes: &[Snake],
    food: &mut Vec<Point>,
    settings: FoodSettings,
) -> usize {
    let needed = check_food_needing_placement(rng, settings, food.len());
    if needed == 0 {
        return 0;
    }
    place_food_randomly(rng, width, height, snakes, food, needed)
}

pub fn place_initial_standard_food<R: RngSource>(
    rng: &mut R,
    width: i32,
    height: i32,
    snakes: &[Snake],
    food: &mut Vec<Point>,
    settings: FoodSettings,
) -> usize {
    if settings.initial_food == 0 {
        return 0;
    }
    place_food_randomly(rng, width, height, snakes, food, settings.initial_food)
}
