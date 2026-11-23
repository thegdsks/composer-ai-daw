//! Small seeded generators. Same seed, same sequence, on every platform: no clocks, no
//! OS randomness, no hash maps with random state.

use composer_model::{Id, IdGen, Project};
use std::collections::BTreeSet;

/// Seeded integer source (splitmix64 steps) for musical choices.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// `salt` keeps two recipes with the same seed from making the same choices.
    pub fn new(seed: u64, salt: u64) -> Self {
        Self {
            state: seed ^ salt.rotate_left(32) ^ 0xA076_1D64_78BD_642F,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 0..n, or 0 when n is 0. The tiny modulo bias is fine for musical choices.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    /// True `percent` times in 100.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    /// Inclusive range.
    pub fn between(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo) as u64 + 1) as i64
    }

    /// One of the items. The slice must not be empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

/// Stable text hash used as a salt (FNV-1a).
pub fn salt(text: &str) -> u64 {
    text.bytes().fold(0xCBF2_9CE4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

/// New ids for a plan: seeded, never zero, never one the project already uses.
pub struct IdSource<'a> {
    project: &'a Project,
    gen: IdGen,
    issued: BTreeSet<Id>,
}

impl<'a> IdSource<'a> {
    pub fn new(project: &'a Project, seed: u64, salt: u64) -> Self {
        Self {
            project,
            gen: IdGen::new(seed ^ salt.rotate_left(17)),
            issued: BTreeSet::new(),
        }
    }

    /// Not an iterator: it never ends and each id must be remembered.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Id {
        loop {
            let id = self.gen.next_id();
            if id.0 != 0 && !self.project.contains_id(id) && self.issued.insert(id) {
                return id;
            }
        }
    }
}
