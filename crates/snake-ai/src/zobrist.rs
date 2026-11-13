use crate::grid::Grid;
use crate::model::{AgentState, FastBody};

#[derive(Debug, Clone)]
pub struct Zobrist {
    pub width: i32,
    pub height: i32,
    table: Vec<[u64; 4]>,
    /// Per cell and side: the key of a snake's head / tail standing there. The cell sets alone cannot tell
    /// a snake from its reverse.
    head: Vec<[u64; 2]>,
    tail: Vec<[u64; 2]>,
    pub health: [[u64; 101]; 2], // 0 = me, 1 = enemy.
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

        let head: Vec<[u64; 2]> = (0..size).map(|_| [splitmix64(&mut seed), splitmix64(&mut seed)]).collect();
        let tail: Vec<[u64; 2]> = (0..size).map(|_| [splitmix64(&mut seed), splitmix64(&mut seed)]).collect();

        let mut health = [[0u64; 101]; 2];
        let (row0, row1) = health.split_at_mut(1);
        for (h0, h1) in row0[0].iter_mut().zip(row1[0].iter_mut()) {
            *h0 = splitmix64(&mut seed);
            *h1 = splitmix64(&mut seed);
        }

        Self {
            width,
            height,
            table,
            head,
            tail,
            health,
        }
    }

    /// Key of where `body`'s head and tail are.
    #[inline(always)]
    pub fn ends(&self, side: usize, body: &FastBody) -> u64 {
        if body.is_empty() {
            return 0;
        }
        let (head, tail) = (body.head(), body.last());
        self.head[(head.y * self.width + head.x) as usize][side] ^ self.tail[(tail.y * self.width + tail.x) as usize][side]
    }

    pub fn compute_hash<const W: usize, const N: usize>(&self, grid: &Grid<W, N>, me: &AgentState, enemy: &AgentState) -> u64 {
        let mut h = 0u64;
        let mut xor_bits = |bb: crate::bitboard::BitBoard<W, N>, piece: usize| {
            for i in 0..W {
                let mut val = bb.words[i];
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

        h ^= self.health[0][clamp_health(me.health)] ^ self.ends(0, &me.body);
        h ^= self.health[1][clamp_health(enemy.health)] ^ self.ends(1, &enemy.body);
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
