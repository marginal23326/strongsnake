use std::collections::HashSet;

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

fn get_unoccupied_points(width: i32, height: i32, snakes: &[Snake], food: &[Point]) -> Vec<Point> {
    let mut occupied = HashSet::new();
    for snake in snakes {
        for part in &snake.body {
            occupied.insert((part.x, part.y));
        }
    }
    for item in food {
        occupied.insert((item.x, item.y));
    }

    let mut points = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if !occupied.contains(&(x, y)) {
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
    let mut unoccupied = get_unoccupied_points(width, height, snakes, food);
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
