use crate::{Point, RngSource};

pub const SNAKE_START_SIZE: usize = 3;

const BOARD_SIZE_SMALL: i32 = 7;

/// A number in `0..n`. Uses the upper bits: the low bits of the LCG behind `LcgRng` only alternate.
fn below<R: RngSource>(rng: &mut R, n: usize) -> usize {
    if n <= 1 { 0 } else { (rng.next_u32() >> 8) as usize % n }
}

fn shuffle<T, R: RngSource>(rng: &mut R, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = below(rng, i + 1);
        items.swap(i, j);
    }
}

/// Start squares for `count` snakes, in snake order. `None` if the board has no room.
pub fn standard_start_points<R: RngSource>(rng: &mut R, width: i32, height: i32, count: usize) -> Option<Vec<Point>> {
    if width == height && count <= 8 && width >= BOARD_SIZE_SMALL {
        let (mn, md, mx) = (1, (width - 1) / 2, width - 2);
        let mut corners = [(mn, mn), (mn, mx), (mx, mn), (mx, mx)];
        let mut cardinals = [(mn, md), (md, mn), (md, mx), (mx, md)];
        shuffle(rng, &mut corners);
        shuffle(rng, &mut cardinals);
        let (first, second) = if below(rng, 2) == 0 { (corners, cardinals) } else { (cardinals, corners) };
        return Some(first.into_iter().chain(second).take(count).map(|(x, y)| Point { x, y }).collect());
    }

    let center = Point { x: (width - 1) / 2, y: (height - 1) / 2 };
    let mut taken: Vec<Point> = Vec::with_capacity(count);
    for _ in 0..count {
        let free: Vec<Point> = (0..height)
            .flat_map(|y| (0..width).map(move |x| Point { x, y }))
            .filter(|p| (p.x + p.y) % 2 == 0 && *p != center && !taken.contains(p))
            .collect();
        if free.is_empty() {
            return None;
        }
        taken.push(free[below(rng, free.len())]);
    }
    Some(taken)
}
