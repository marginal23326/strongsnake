use crate::bitboard::{BitBoard, SearchContext};
use snake_domain::Point;

#[derive(Clone)]
pub struct Grid<const N: usize>
where
    [(); (N + 63) / 64]: Sized,
{
    pub width: i32,
    pub height: i32,
    pub ctx: SearchContext<N>,
    pub food: BitBoard<N>,
    pub my_body: BitBoard<N>,
    pub enemy_body: BitBoard<N>,
}

impl<const N: usize> Grid<N>
where
    [(); (N + 63) / 64]: Sized,
{
    pub fn new(width: i32, height: i32) -> Self {
        if (width * height) as usize > N {
            panic!("Grid size {}x{} too large for {} bits.", width, height, N);
        }
        Self {
            width,
            height,
            ctx: SearchContext::new(width, height),
            food: BitBoard::empty(),
            my_body: BitBoard::empty(),
            enemy_body: BitBoard::empty(),
        }
    }

    #[inline(always)]
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    #[inline(always)]
    pub fn get(&self, x: i32, y: i32) -> i8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 9;
        }
        let idx = self.idx(x, y);
        if self.my_body.get(idx) {
            return 2;
        }
        if self.enemy_body.get(idx) {
            return 3;
        }
        if self.food.get(idx) {
            return 1;
        }
        0
    }

    #[inline(always)]
    pub fn set(&mut self, x: i32, y: i32, val: i8) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        let idx = self.idx(x, y);

        // Always clear the cell first to prevent overlapping state
        self.food.unset(idx);
        self.my_body.unset(idx);
        self.enemy_body.unset(idx);

        match val {
            1 => self.food.set(idx),
            2 => self.my_body.set(idx),
            3 => self.enemy_body.set(idx),
            _ => {}
        }
    }

    /// Replaces `old_val` with `new_val` directly.
    /// # Safety
    /// Caller must ensure `(x, y)` is in-bounds.
    #[inline(always)]
    pub unsafe fn replace_unchecked(&mut self, x: i32, y: i32, old_val: i8, new_val: i8) {
        let idx = (y * self.width + x) as usize;

        match old_val {
            1 => self.food.unset(idx),
            2 => self.my_body.unset(idx),
            3 => self.enemy_body.unset(idx),
            _ => {}
        }

        match new_val {
            1 => self.food.set(idx),
            2 => self.my_body.set(idx),
            3 => self.enemy_body.set(idx),
            _ => {}
        }
    }

    /// Clears `old_val` from the board.
    /// # Safety
    /// Caller must ensure `(x, y)` is in-bounds.
    #[inline(always)]
    pub unsafe fn clear_unchecked(&mut self, x: i32, y: i32, old_val: i8) {
        let idx = (y * self.width + x) as usize;
        match old_val {
            1 => self.food.unset(idx),
            2 => self.my_body.unset(idx),
            3 => self.enemy_body.unset(idx),
            _ => {}
        }
    }

    #[inline(always)]
    pub fn is_safe(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return false;
        }
        let idx = self.idx(x, y);
        !self.my_body.get(idx) && !self.enemy_body.get(idx)
    }

    #[inline(always)]
    pub fn occupied(&self) -> BitBoard<N> {
        self.my_body | self.enemy_body
    }

    #[inline(always)]
    pub fn safe_cells(&self) -> BitBoard<N> {
        self.ctx.valid_cells & !self.occupied()
    }

    pub fn from_state(cols: i32, rows: i32, food: &[Point], my_body: &crate::model::FastBody, enemy_body: &crate::model::FastBody) -> Self {
        let mut g = Self::new(cols, rows);
        for f in food {
            g.set(f.x, f.y, 1);
        }
        for p in my_body.iter() {
            g.set(p.x, p.y, 2);
        }
        for p in enemy_body.iter() {
            g.set(p.x, p.y, 3);
        }
        g
    }
}
