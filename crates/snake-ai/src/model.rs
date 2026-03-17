use snake_domain::Point;

const MAX_BODY: usize = 512;
const MASK: usize = MAX_BODY - 1;

#[derive(Debug, Clone)]
pub struct FastBody {
    buffer: [Point; MAX_BODY],
    head_idx: usize,
    len: usize,
}

impl FastBody {
    pub fn from_points<I>(points: I) -> Self
    where
        I: IntoIterator<Item = Point>,
    {
        let mut buffer = [Point { x: 0, y: 0 }; MAX_BODY];
        let mut len = 0;
        for (i, p) in points.into_iter().enumerate() {
            buffer[i] = p;
            len = i + 1;
        }
        Self { buffer, head_idx: 0, len }
    }

    pub fn from_vec(v: &[Point]) -> Self {
        Self::from_points(v.iter().copied())
    }

    #[inline(always)]
    pub fn head(&self) -> Point {
        unsafe { *self.buffer.get_unchecked(self.head_idx) }
    }

    #[inline(always)]
    pub fn get(&self, index: usize) -> Point {
        unsafe { *self.buffer.get_unchecked((self.head_idx + index) & MASK) }
    }

    #[inline(always)]
    pub fn last(&self) -> Point {
        self.get(self.len - 1)
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = Point> + '_ {
        (0..self.len).map(move |i| self.get(i))
    }

    // --- Deque Operations for Search ---

    #[inline(always)]
    pub fn push_front(&mut self, p: Point) {
        self.head_idx = (self.head_idx.wrapping_sub(1)) & MASK;

        unsafe {
            *self.buffer.get_unchecked_mut(self.head_idx) = p;
        }
        self.len += 1;
    }

    #[inline(always)]
    pub fn pop_front(&mut self) {
        self.head_idx = (self.head_idx + 1) & MASK;
        self.len -= 1;
    }

    #[inline(always)]
    pub fn pop_back(&mut self) -> Point {
        let p = self.last();
        self.len -= 1;
        p
    }

    #[inline(always)]
    pub fn push_back(&mut self, p: Point) {
        let tail_idx = (self.head_idx + self.len) & MASK;
        unsafe {
            *self.buffer.get_unchecked_mut(tail_idx) = p;
        }
        self.len += 1;
    }
}

#[derive(Debug, Clone)]
pub struct AgentState {
    pub body: FastBody,
    pub health: i32,
}

#[derive(Debug, Clone)]
pub struct SearchBuffers {
    pub current_gen: u16,
}

impl SearchBuffers {
    pub fn new(_size: usize) -> Self {
        Self { current_gen: 0 }
    }
    pub fn ensure_adj(&mut self, _width: i32, _height: i32) {}
    pub fn next_gen(&mut self) -> u16 {
        self.current_gen = self.current_gen.wrapping_add(1);
        if self.current_gen == 0 {
            self.current_gen = 1;
        }
        self.current_gen
    }
}
