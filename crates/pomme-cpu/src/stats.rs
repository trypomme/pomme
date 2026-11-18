//! Counters for measuring where the CPU's time goes (the `stats` feature,
//! off by default). They only count: they never change what the CPU does,
//! so a run with them gives the same instructions, report and screen.
//!
//! Used to size the JIT (openspec/changes/add-jit, task 0.2).

use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct Stats {
    /// Blocks run whole, and the instructions in them.
    pub blocks_run: u64,
    pub block_insns: u64,
    /// Instructions run one at a time (a block didn't fit, or couldn't be fetched).
    pub single_steps: u64,
    /// Blocks fetched and decoded (cache misses).
    pub blocks_fetched: u64,
    /// Times the whole block cache was dropped because code was written.
    pub cache_flushes: u64,
    /// How many blocks run whole had each length (index = length).
    pub len_hist: Vec<u64>,
    /// Distinct blocks seen, by start address, with how many times each ran.
    pub block_runs: HashMap<u64, u64>,
    /// `Op::Generic` instructions run inside blocks, by encoding with the
    /// register fields (bits 0-9 and 16-20) cleared.
    pub generic: HashMap<u32, u64>,
    pub generic_total: u64,
}

thread_local! {
    static STATS: RefCell<Stats> = RefCell::new(Stats::default());
}

pub(crate) fn with(f: impl FnOnce(&mut Stats)) {
    STATS.with(|s| f(&mut s.borrow_mut()));
}

/// A copy of the counters so far.
pub fn snapshot() -> Stats {
    STATS.with(|s| s.borrow().clone())
}
