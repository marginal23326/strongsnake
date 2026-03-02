use crate::grid::Grid;

#[derive(Debug, Clone)]
pub struct Zobrist {
    pub width: i32,
    pub height: i32,
    table: Vec<[u64; 4]>,
    my_health: [u64; 101],
    enemy_health: [u64; 101],
}

impl Zobrist {
    pub fn new(width: i32, height: i32) -> Self {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64 ^ ((width as u64) << 32) ^ ((height as u64) << 8);
        let size = (width * height) as usize;
        let mut table = vec![[0u64; 4]; size];
        for slot in &mut table {
            slot[1] = splitmix64(&mut seed);
            slot[2] = splitmix64(&mut seed);
            slot[3] = splitmix64(&mut seed);
        }
        let mut my_health = [0u64; 101];
        let mut enemy_health = [0u64; 101];
        for i in 0..=100 {
            my_health[i] = splitmix64(&mut seed);
            enemy_health[i] = splitmix64(&mut seed);
        }
        Self {
            width,
            height,
            table,
            my_health,
            enemy_health,
        }
    }

    pub fn compute_hash(&self, grid: &Grid, my_health: i32, enemy_health: i32) -> u64 {
        let mut h = 0u64;

        let mut xor_bits = |bb: crate::bitboard::BitBoard, piece: usize| {
            for i in 0..7 {
                let mut val = bb.0[i];
                while val != 0 {
                    let bit = val.trailing_zeros();
                    h ^= self.table[(i << 6) | (bit as usize)][piece];
                    val &= val - 1;
                }
            }
        };

        xor_bits(grid.food, 1);
        xor_bits(grid.my_body, 2);
        xor_bits(grid.enemy_body, 3);

        h ^= self.my_health[clamp_health(my_health)];
        h ^= self.enemy_health[clamp_health(enemy_health)];
        h
    }

    /// # Safety
    /// Caller must ensure `(x, y)` is in-bounds and `piece` is in `0..=3`.
    #[inline(always)]
    pub unsafe fn xor_unchecked(&self, current_hash: u64, x: i32, y: i32, piece: i8) -> u64 {
        let idx = (y * self.width + x) as usize;
        let p_idx = piece as usize;

        unsafe { current_hash ^ *self.table.get_unchecked(idx).get_unchecked(p_idx) }
    }

    #[inline(always)]
    pub fn xor_health(&self, current_hash: u64, old_health: i32, new_health: i32, is_me: bool) -> u64 {
        let table = if is_me { &self.my_health } else { &self.enemy_health };
        current_hash ^ table[clamp_health(old_health)] ^ table[clamp_health(new_health)]
    }
}

#[inline(always)]
fn clamp_health(v: i32) -> usize {
    v.clamp(0, 100) as usize
}

#[inline]
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
