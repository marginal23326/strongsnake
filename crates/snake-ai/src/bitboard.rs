use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitBoard<const N: usize>
where
    [(); (N + 63) / 64]: Sized,
{
    pub words: [u64; (N + 63) / 64],
}

impl<const N: usize> Default for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    #[inline(always)]
    fn default() -> Self {
        Self::empty()
    }
}

impl<const N: usize> BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    pub const NUM_WORDS: usize = (N + 63) / 64;

    #[inline(always)]
    pub const fn empty() -> Self {
        Self { words: [0; (N + 63) / 64] }
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
            *self.words.get_unchecked_mut(idx >> 6) |= 1u64 << (idx & 63);
        }
    }

    #[inline(always)]
    pub fn unset(&mut self, idx: usize) {
        unsafe {
            *self.words.get_unchecked_mut(idx >> 6) &= !(1u64 << (idx & 63));
        }
    }

    #[inline(always)]
    pub fn get(&self, idx: usize) -> bool {
        unsafe { (*self.words.get_unchecked(idx >> 6) >> (idx & 63)) & 1 == 1 }
    }

    #[inline(always)]
    pub fn count_ones(&self) -> u32 {
        let mut count = 0;
        for i in 0..Self::NUM_WORDS {
            count += self.words[i].count_ones();
        }
        count
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        for i in 0..Self::NUM_WORDS {
            if self.words[i] != 0 {
                return false;
            }
        }
        true
    }

    #[inline(always)]
    pub fn any(&self) -> bool {
        !self.is_empty()
    }

    #[inline(always)]
    pub fn tzcnt(&self) -> u32 {
        for i in 0..Self::NUM_WORDS {
            if self.words[i] != 0 {
                return (i as u32 * 64) + self.words[i].trailing_zeros();
            }
        }
        N as u32
    }

    /// Removes and returns the index of the first set bit instantly.
    #[inline(always)]
    pub fn pop_first(&mut self) -> Option<usize> {
        for i in 0..Self::NUM_WORDS {
            if self.words[i] != 0 {
                let bit = self.words[i].trailing_zeros();
                self.words[i] &= self.words[i] - 1; // Unset the lowest bit instantly
                return Some((i * 64) + bit as usize);
            }
        }
        None
    }
}

impl<const N: usize> fmt::Debug for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BitBoard(")?;
        for i in (0..Self::NUM_WORDS).rev() {
            write!(f, "{:016X}", self.words[i])?;
        }
        write!(f, ")")
    }
}

// --- Dynamic Unrolled Bitwise Ops ---

impl<const N: usize> Not for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self {
        let mut ret = Self::empty();
        for i in 0..Self::NUM_WORDS {
            ret.words[i] = !self.words[i];
        }
        ret
    }
}

impl<const N: usize> BitAnd for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    type Output = Self;
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self {
        let mut ret = Self::empty();
        for i in 0..Self::NUM_WORDS {
            ret.words[i] = self.words[i] & rhs.words[i];
        }
        ret
    }
}

impl<const N: usize> BitAndAssign for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        for i in 0..Self::NUM_WORDS {
            self.words[i] &= rhs.words[i];
        }
    }
}

impl<const N: usize> BitOr for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    type Output = Self;
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self {
        let mut ret = Self::empty();
        for i in 0..Self::NUM_WORDS {
            ret.words[i] = self.words[i] | rhs.words[i];
        }
        ret
    }
}

impl<const N: usize> BitOrAssign for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        for i in 0..Self::NUM_WORDS {
            self.words[i] |= rhs.words[i];
        }
    }
}

impl<const N: usize> BitXor for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    type Output = Self;
    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self {
        let mut ret = Self::empty();
        for i in 0..Self::NUM_WORDS {
            ret.words[i] = self.words[i] ^ rhs.words[i];
        }
        ret
    }
}

impl<const N: usize> BitXorAssign for BitBoard<N>
where
    [(); (N + 63) / 64]: Sized,
{
    #[inline(always)]
    fn bitxor_assign(&mut self, rhs: Self) {
        for i in 0..Self::NUM_WORDS {
            self.words[i] ^= rhs.words[i];
        }
    }
}

// --- Search Context (Precalculated Masks) ---

#[derive(Clone)]
pub struct SearchContext<const N: usize>
where
    [(); (N + 63) / 64]: Sized,
{
    pub width: u8,
    pub height: u8,
    pub valid_cells: BitBoard<N>,
    pub not_left_edge: BitBoard<N>,
    pub not_right_edge: BitBoard<N>,
}

impl<const N: usize> SearchContext<N>
where
    [(); (N + 63) / 64]: Sized,
{
    pub fn new(width: i32, height: i32) -> Self {
        assert!((width * height) as usize <= N, "Board too large for bitset");

        let mut valid = BitBoard::empty();
        let mut not_left = !BitBoard::empty();
        let mut not_right = !BitBoard::empty();

        let w = width as usize;
        for y in 0..(height as usize) {
            not_left.unset(y * w);
            not_right.unset(y * w + w - 1);
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
    pub fn up(&self, b: BitBoard<N>) -> BitBoard<N> {
        let mut ret = BitBoard::empty();
        if BitBoard::<N>::NUM_WORDS == 2 {
            unsafe {
                let x = std::mem::transmute_copy::<[u64; 2], u128>(&[b.words[0], b.words[1]]);
                let split: [u64; 2] = std::mem::transmute(x << self.width);
                ret.words[0] = split[0];
                ret.words[1] = split[1];
            }
        } else {
            let shift = self.width as usize;
            let inv = 64 - shift;
            for i in (1..BitBoard::<N>::NUM_WORDS).rev() {
                ret.words[i] = (b.words[i] << shift) | (b.words[i - 1] >> inv);
            }
            ret.words[0] = b.words[0] << shift;
        }
        ret & self.valid_cells
    }

    #[inline(always)]
    pub fn down(&self, b: BitBoard<N>) -> BitBoard<N> {
        let mut ret = BitBoard::empty();
        if BitBoard::<N>::NUM_WORDS == 2 {
            unsafe {
                let x = std::mem::transmute_copy::<[u64; 2], u128>(&[b.words[0], b.words[1]]);
                let split: [u64; 2] = std::mem::transmute(x >> self.width);
                ret.words[0] = split[0];
                ret.words[1] = split[1];
            }
        } else {
            let shift = self.width as usize;
            let inv = 64 - shift;
            for i in 0..(BitBoard::<N>::NUM_WORDS - 1) {
                ret.words[i] = (b.words[i] >> shift) | (b.words[i + 1] << inv);
            }
            ret.words[BitBoard::<N>::NUM_WORDS - 1] = b.words[BitBoard::<N>::NUM_WORDS - 1] >> shift;
        }
        ret & self.valid_cells
    }

    #[inline(always)]
    pub fn left(&self, b: BitBoard<N>) -> BitBoard<N> {
        let masked = b & self.not_left_edge;
        let mut ret = BitBoard::empty();
        if BitBoard::<N>::NUM_WORDS == 2 {
            unsafe {
                let x = std::mem::transmute_copy::<[u64; 2], u128>(&[masked.words[0], masked.words[1]]);
                let split: [u64; 2] = std::mem::transmute(x >> 1);
                ret.words[0] = split[0];
                ret.words[1] = split[1];
            }
        } else {
            for i in 0..(BitBoard::<N>::NUM_WORDS - 1) {
                ret.words[i] = (masked.words[i] >> 1) | (masked.words[i + 1] << 63);
            }
            ret.words[BitBoard::<N>::NUM_WORDS - 1] = masked.words[BitBoard::<N>::NUM_WORDS - 1] >> 1;
        }
        ret
    }

    #[inline(always)]
    pub fn right(&self, b: BitBoard<N>) -> BitBoard<N> {
        let masked = b & self.not_right_edge;
        let mut ret = BitBoard::empty();
        if BitBoard::<N>::NUM_WORDS == 2 {
            unsafe {
                let x = std::mem::transmute_copy::<[u64; 2], u128>(&[masked.words[0], masked.words[1]]);
                let split: [u64; 2] = std::mem::transmute(x << 1);
                ret.words[0] = split[0];
                ret.words[1] = split[1];
            }
        } else {
            for i in (1..BitBoard::<N>::NUM_WORDS).rev() {
                ret.words[i] = (masked.words[i] << 1) | (masked.words[i - 1] >> 63);
            }
            ret.words[0] = masked.words[0] << 1;
        }
        ret
    }

    #[inline(always)]
    pub fn expand_neighbors(&self, b: BitBoard<N>) -> BitBoard<N> {
        self.up(b) | self.down(b) | self.left(b) | self.right(b)
    }
}
