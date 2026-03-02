use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, Shr};

/// 7 words * 64 bits = 448 bits. Perfectly fits up to 20x20 boards (400 bits).
const N_WORDS: usize = 7;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BitBoard(pub [u64; N_WORDS]);

impl BitBoard {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self([0; N_WORDS])
    }

    #[inline(always)]
    pub fn with_bit(idx: usize) -> Self {
        let mut b = Self::empty();
        b.set(idx);
        b
    }

    #[inline(always)]
    pub fn set(&mut self, idx: usize) {
        unsafe {
            *self.0.get_unchecked_mut(idx >> 6) |= 1u64 << (idx & 63);
        }
    }

    #[inline(always)]
    pub fn unset(&mut self, idx: usize) {
        unsafe {
            *self.0.get_unchecked_mut(idx >> 6) &= !(1u64 << (idx & 63));
        }
    }

    #[inline(always)]
    pub fn get(&self, idx: usize) -> bool {
        unsafe { (*self.0.get_unchecked(idx >> 6) >> (idx & 63)) & 1 == 1 }
    }

    #[inline(always)]
    pub fn count_ones(&self) -> u32 {
        self.0[0].count_ones()
            + self.0[1].count_ones()
            + self.0[2].count_ones()
            + self.0[3].count_ones()
            + self.0[4].count_ones()
            + self.0[5].count_ones()
            + self.0[6].count_ones()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        (self.0[0] | self.0[1] | self.0[2] | self.0[3] | self.0[4] | self.0[5] | self.0[6]) == 0
    }

    #[inline(always)]
    pub fn any(&self) -> bool {
        !self.is_empty()
    }

    /// Fast Trailing Zero Count. Returns the index of the first set bit.
    /// Incredibly fast way to find the next active cell without iterating.
    #[inline(always)]
    pub fn tzcnt(&self) -> u32 {
        if self.0[0] != 0 {
            return self.0[0].trailing_zeros();
        }
        if self.0[1] != 0 {
            return 64 + self.0[1].trailing_zeros();
        }
        if self.0[2] != 0 {
            return 128 + self.0[2].trailing_zeros();
        }
        if self.0[3] != 0 {
            return 192 + self.0[3].trailing_zeros();
        }
        if self.0[4] != 0 {
            return 256 + self.0[4].trailing_zeros();
        }
        if self.0[5] != 0 {
            return 320 + self.0[5].trailing_zeros();
        }
        if self.0[6] != 0 {
            return 384 + self.0[6].trailing_zeros();
        }
        448
    }

    /// Removes and returns the index of the first set bit.
    #[inline(always)]
    pub fn pop_first(&mut self) -> Option<usize> {
        let idx = self.tzcnt() as usize;
        if idx >= 448 {
            None
        } else {
            self.unset(idx);
            Some(idx)
        }
    }
}

// --- Display / Debug ---

impl fmt::Debug for BitBoard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BitBoard(")?;
        for i in 0..N_WORDS {
            write!(f, "{:016X}", self.0[N_WORDS - 1 - i])?;
        }
        write!(f, ")")
    }
}

// --- Bitwise Operations ---

impl Not for BitBoard {
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self {
        Self([!self.0[0], !self.0[1], !self.0[2], !self.0[3], !self.0[4], !self.0[5], !self.0[6]])
    }
}

impl BitAnd for BitBoard {
    type Output = Self;
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self {
        Self([
            self.0[0] & rhs.0[0],
            self.0[1] & rhs.0[1],
            self.0[2] & rhs.0[2],
            self.0[3] & rhs.0[3],
            self.0[4] & rhs.0[4],
            self.0[5] & rhs.0[5],
            self.0[6] & rhs.0[6],
        ])
    }
}

impl BitAndAssign for BitBoard {
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0[0] &= rhs.0[0];
        self.0[1] &= rhs.0[1];
        self.0[2] &= rhs.0[2];
        self.0[3] &= rhs.0[3];
        self.0[4] &= rhs.0[4];
        self.0[5] &= rhs.0[5];
        self.0[6] &= rhs.0[6];
    }
}

impl BitOr for BitBoard {
    type Output = Self;
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        Self([
            self.0[0] | rhs.0[0],
            self.0[1] | rhs.0[1],
            self.0[2] | rhs.0[2],
            self.0[3] | rhs.0[3],
            self.0[4] | rhs.0[4],
            self.0[5] | rhs.0[5],
            self.0[6] | rhs.0[6],
        ])
    }
}

impl BitOrAssign for BitBoard {
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0[0] |= rhs.0[0];
        self.0[1] |= rhs.0[1];
        self.0[2] |= rhs.0[2];
        self.0[3] |= rhs.0[3];
        self.0[4] |= rhs.0[4];
        self.0[5] |= rhs.0[5];
        self.0[6] |= rhs.0[6];
    }
}

impl BitXor for BitBoard {
    type Output = Self;
    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self {
        Self([
            self.0[0] ^ rhs.0[0],
            self.0[1] ^ rhs.0[1],
            self.0[2] ^ rhs.0[2],
            self.0[3] ^ rhs.0[3],
            self.0[4] ^ rhs.0[4],
            self.0[5] ^ rhs.0[5],
            self.0[6] ^ rhs.0[6],
        ])
    }
}

impl BitXorAssign for BitBoard {
    #[inline(always)]
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0[0] ^= rhs.0[0];
        self.0[1] ^= rhs.0[1];
        self.0[2] ^= rhs.0[2];
        self.0[3] ^= rhs.0[3];
        self.0[4] ^= rhs.0[4];
        self.0[5] ^= rhs.0[5];
        self.0[6] ^= rhs.0[6];
    }
}

// --- Dynamic Shifts for Any Board Size ---

impl Shl<usize> for BitBoard {
    type Output = Self;
    #[inline]
    fn shl(self, rhs: usize) -> Self {
        let mut ret = Self::empty();
        let word_shift = rhs >> 6;
        let bit_shift = rhs & 63;

        if word_shift >= N_WORDS {
            return ret;
        }

        if bit_shift == 0 {
            for i in 0..(N_WORDS - word_shift) {
                ret.0[i + word_shift] = self.0[i];
            }
        } else {
            for i in (0..(N_WORDS - word_shift)).rev() {
                let lower = self.0[i] << bit_shift;
                let carry = if i > 0 { self.0[i - 1] >> (64 - bit_shift) } else { 0 };
                ret.0[i + word_shift] = lower | carry;
            }
        }
        ret
    }
}

impl Shr<usize> for BitBoard {
    type Output = Self;
    #[inline]
    fn shr(self, rhs: usize) -> Self {
        let mut ret = Self::empty();
        let word_shift = rhs >> 6;
        let bit_shift = rhs & 63;

        if word_shift >= N_WORDS {
            return ret;
        }

        if bit_shift == 0 {
            for i in 0..(N_WORDS - word_shift) {
                ret.0[i] = self.0[i + word_shift];
            }
        } else {
            for i in 0..(N_WORDS - 1 - word_shift) {
                let lower = self.0[i + word_shift] >> bit_shift;
                let carry = self.0[i + word_shift + 1] << (64 - bit_shift);
                ret.0[i] = lower | carry;
            }
            ret.0[N_WORDS - 1 - word_shift] = self.0[N_WORDS - 1] >> bit_shift;
        }
        ret
    }
}

// --- Search Context (Precalculated Masks) ---

#[derive(Clone)]
pub struct SearchContext {
    pub width: u8,
    pub height: u8,
    pub valid_cells: BitBoard,
    pub not_left_edge: BitBoard,
    pub not_right_edge: BitBoard,
}

impl SearchContext {
    pub fn new(width: i32, height: i32) -> Self {
        assert!((width * height) as usize <= N_WORDS * 64, "Board too large");

        let mut valid = BitBoard::empty();
        let mut not_left = !BitBoard::empty();
        let mut not_right = !BitBoard::empty();

        let w = width as usize;
        for y in 0..(height as usize) {
            not_left.unset(y * w); // Clear x = 0
            not_right.unset(y * w + w - 1); // Clear x = width - 1
            for x in 0..w {
                valid.set(y * w + x);
            }
        }

        Self {
            width: width as u8,
            height: height as u8,
            valid_cells: valid,
            not_left_edge: not_left,
            not_right_edge: not_right,
        }
    }

    #[inline(always)]
    pub fn up(&self, b: BitBoard) -> BitBoard {
        let shift = self.width as usize;
        let inv = 64 - shift;
        let mut ret = BitBoard::empty();
        ret.0[6] = (b.0[6] << shift) | (b.0[5] >> inv);
        ret.0[5] = (b.0[5] << shift) | (b.0[4] >> inv);
        ret.0[4] = (b.0[4] << shift) | (b.0[3] >> inv);
        ret.0[3] = (b.0[3] << shift) | (b.0[2] >> inv);
        ret.0[2] = (b.0[2] << shift) | (b.0[1] >> inv);
        ret.0[1] = (b.0[1] << shift) | (b.0[0] >> inv);
        ret.0[0] = b.0[0] << shift;

        ret & self.valid_cells
    }

    #[inline(always)]
    pub fn down(&self, b: BitBoard) -> BitBoard {
        let shift = self.width as usize;
        let inv = 64 - shift;
        let mut ret = BitBoard::empty();
        ret.0[0] = (b.0[0] >> shift) | (b.0[1] << inv);
        ret.0[1] = (b.0[1] >> shift) | (b.0[2] << inv);
        ret.0[2] = (b.0[2] >> shift) | (b.0[3] << inv);
        ret.0[3] = (b.0[3] >> shift) | (b.0[4] << inv);
        ret.0[4] = (b.0[4] >> shift) | (b.0[5] << inv);
        ret.0[5] = (b.0[5] >> shift) | (b.0[6] << inv);
        ret.0[6] = b.0[6] >> shift;

        ret & self.valid_cells
    }

    #[inline(always)]
    pub fn left(&self, b: BitBoard) -> BitBoard {
        let masked = b & self.not_left_edge;
        let mut ret = BitBoard::empty();
        ret.0[0] = (masked.0[0] >> 1) | (masked.0[1] << 63);
        ret.0[1] = (masked.0[1] >> 1) | (masked.0[2] << 63);
        ret.0[2] = (masked.0[2] >> 1) | (masked.0[3] << 63);
        ret.0[3] = (masked.0[3] >> 1) | (masked.0[4] << 63);
        ret.0[4] = (masked.0[4] >> 1) | (masked.0[5] << 63);
        ret.0[5] = (masked.0[5] >> 1) | (masked.0[6] << 63);
        ret.0[6] = masked.0[6] >> 1;
        ret
    }

    #[inline(always)]
    pub fn right(&self, b: BitBoard) -> BitBoard {
        let masked = b & self.not_right_edge;
        let mut ret = BitBoard::empty();
        ret.0[6] = (masked.0[6] << 1) | (masked.0[5] >> 63);
        ret.0[5] = (masked.0[5] << 1) | (masked.0[4] >> 63);
        ret.0[4] = (masked.0[4] << 1) | (masked.0[3] >> 63);
        ret.0[3] = (masked.0[3] << 1) | (masked.0[2] >> 63);
        ret.0[2] = (masked.0[2] << 1) | (masked.0[1] >> 63);
        ret.0[1] = (masked.0[1] << 1) | (masked.0[0] >> 63);
        ret.0[0] = masked.0[0] << 1;
        ret
    }

    /// Returns the four cardinal neighbors of the given frontier bitset.
    #[inline(always)]
    pub fn expand_neighbors(&self, b: BitBoard) -> BitBoard {
        self.up(b) | self.down(b) | self.left(b) | self.right(b)
    }
}
