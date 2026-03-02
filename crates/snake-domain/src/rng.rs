pub trait RngSource {
    fn next_u32(&mut self) -> u32;

    #[inline]
    fn rand_int(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            return 0;
        }
        (self.next_u32() as usize) % upper_exclusive
    }
}

#[derive(Debug, Clone)]
pub struct LcgRng {
    state: u32,
}

impl LcgRng {
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub fn state(&self) -> u32 {
        self.state
    }
}

impl RngSource for LcgRng {
    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.state
    }
}
