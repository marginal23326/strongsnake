use snake_domain::Direction;
use std::cell::UnsafeCell;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TtFlag {
    #[default]
    Exact,
    LowerBound,
    UpperBound,
}

#[derive(Debug, Clone, Copy)]
pub struct TtMove {
    pub x: i32,
    pub y: i32,
    pub dir: Direction,
    pub dir_int: usize,
}

impl Default for TtMove {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            dir: Direction::Up,
            dir_int: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TtEntry {
    pub key: u64,
    pub score: i32,
    pub generation: u16,
    pub depth: u8,
    pub flag: TtFlag,
    pub mv_x: u8,
    pub mv_y: u8,
    pub mv_dir: u8, // 0=Up, 1=Down, 2=Left, 3=Right, 255=None
}

impl Default for TtEntry {
    fn default() -> Self {
        Self {
            key: 0,
            score: 0,
            generation: 0,
            depth: 0,
            flag: TtFlag::Exact,
            mv_x: 255,
            mv_y: 255,
            mv_dir: 255,
        }
    }
}

impl TtEntry {
    #[inline]
    pub fn get_direction(&self) -> Option<Direction> {
        match self.mv_dir {
            0 => Some(Direction::Up),
            1 => Some(Direction::Down),
            2 => Some(Direction::Left),
            3 => Some(Direction::Right),
            _ => None,
        }
    }
}

pub struct TtSlot {
    seq: AtomicUsize,
    entry: UnsafeCell<TtEntry>,
}

unsafe impl Sync for TtSlot {}
unsafe impl Send for TtSlot {}

impl Default for TtSlot {
    fn default() -> Self {
        Self {
            seq: AtomicUsize::new(0),
            entry: UnsafeCell::new(TtEntry::default()),
        }
    }
}

impl Clone for TtSlot {
    fn clone(&self) -> Self {
        let seq = self.seq.load(Ordering::Relaxed);
        let entry = unsafe { *self.entry.get() };
        Self {
            seq: AtomicUsize::new(seq),
            entry: UnsafeCell::new(entry),
        }
    }
}

pub struct TranspositionTable {
    entries: Vec<TtSlot>,
    mask: usize,
    pub generation: u16,
}

impl TranspositionTable {
    pub fn new(size: usize) -> Self {
        let size = if size.is_power_of_two() { size } else { size.next_power_of_two() };
        Self {
            entries: vec![TtSlot::default(); size],
            mask: size - 1,
            generation: 0,
        }
    }

    pub fn prepare_for_search(&mut self, requested_size: usize) {
        let size = if requested_size.is_power_of_two() {
            requested_size
        } else {
            requested_size.next_power_of_two()
        };

        if size > self.entries.len() {
            self.entries.resize(size, TtSlot::default());
        }
        self.mask = size - 1;

        self.generation = self.generation.wrapping_add(1);

        if self.generation == 0 {
            self.generation = 1;
            self.entries.fill(TtSlot::default());
        }
    }

    pub fn entries_for_hash_mb(hash_mb: usize) -> usize {
        if hash_mb == 0 {
            return 0;
        }
        let bytes = hash_mb.saturating_mul(1024 * 1024);
        let entry_size = mem::size_of::<TtSlot>().max(1);
        let entries = (bytes / entry_size).max(1);
        if entries.is_power_of_two() {
            entries
        } else {
            entries.next_power_of_two()
        }
    }

    #[inline]
    pub fn get(&self, hash: u64) -> Option<TtEntry> {
        let idx = (hash as usize) & self.mask;
        let slot = unsafe { self.entries.get_unchecked(idx) };

        let mut backoff = 0;
        loop {
            let seq1 = slot.seq.load(Ordering::Acquire);
            if seq1 & 1 != 0 {
                backoff += 1;
                if backoff > 10 {
                    return None;
                } // Prevent deadlock, drop hit
                std::hint::spin_loop();
                continue;
            }
            let entry = unsafe { *slot.entry.get() };
            let seq2 = slot.seq.load(Ordering::Acquire);
            if seq1 == seq2 {
                if entry.key == hash && entry.generation == self.generation {
                    return Some(entry);
                }
                return None;
            }
        }
    }

    #[inline]
    pub fn set(&self, hash: u64, depth: usize, score: i32, flag: TtFlag, mv_x: u8, mv_y: u8, mv_dir: u8) {
        let idx = (hash as usize) & self.mask;
        let slot = unsafe { self.entries.get_unchecked(idx) };

        let mut seq = slot.seq.load(Ordering::Relaxed);
        loop {
            if seq & 1 != 0 {
                return;
            }
            match slot.seq.compare_exchange_weak(seq, seq + 1, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => break,
                Err(s) => seq = s,
            }
        }

        let depth_u8 = depth as u8;
        unsafe {
            let curr = &mut *slot.entry.get();
            if curr.generation != self.generation || curr.key != hash || depth_u8 >= curr.depth {
                *curr = TtEntry {
                    key: hash,
                    score,
                    generation: self.generation,
                    depth: depth_u8,
                    flag,
                    mv_x,
                    mv_y,
                    mv_dir,
                };
            }
        }

        slot.seq.store(seq + 2, Ordering::Release);
    }
}
