//! The JIT as a [`Backend`]: the loop that runs guest code, the table of
//! blocks, when to translate, and throwing translations away when their code
//! changes (design decisions 6, 7 and 8).
//!
//! Code that has not run often yet runs on the interpreter. A block that
//! has run [`Jit::threshold`] times is queued, and queued blocks are
//! translated together by an [`Engine`] (WebAssembly in the browser, the IR
//! evaluator in tests). Whether a block runs translated or on the
//! interpreter never changes a result, so the threshold and the batching
//! only change speed.

use crate::eval;
use crate::ir::Block;
use crate::lift::lift;
use pomme_cpu::{Backend, Budget, Cpu, Exit, HOST_RETURN};
use pomme_mem::{Memory, PAGE_SIZE, untag};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

/// A cheap hasher for guest addresses (std's default hasher showed up in
/// profiles; these keys come from the guest's own code, not from an attacker).
#[derive(Default)]
struct PcHasher(u64);

impl Hasher for PcHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0.rotate_left(5) ^ *b as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u64(&mut self, x: u64) {
        self.0 = (x >> 2).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

type PcMap<V> = HashMap<u64, V, BuildHasherDefault<PcHasher>>;

/// A translated block in the direct-mapped table in front of the map.
/// Translated code reads this table too, to jump to the next block itself.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Fast {
    pub(crate) pc: u64,
    pub(crate) handle: usize,
    pub(crate) len: u64,
    /// Kept alive by the block's entry: the table is cleared whenever entries go.
    pub(crate) block: *const Block,
}

impl std::fmt::Debug for Fast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fast({:#x})", self.pc)
    }
}

const NO_FAST: Fast = Fast { pc: u64::MAX, handle: 0, len: 0, block: std::ptr::null() };
/// Slots in the direct-mapped table (a power of two).
pub const FAST_SLOTS: usize = 1 << 16;

fn fast_slot(pc: u64) -> usize {
    (pc >> 2) as usize & (FAST_SLOTS - 1)
}

/// What translated code is passed while it runs: the state its helpers need.
/// Its address is the `ctx` the generated code gets.
#[repr(C)]
pub struct Ctx {
    /// Set to 1 by a load or store helper that faulted (the code checks it).
    pub fault: u32,
    /// The address of the memory's TLB (for inline loads and stores).
    pub tlb: usize,
    /// The address of the JIT's table of translated blocks ([`Fast`]), for
    /// going on to the next block without returning (0: never).
    pub fast: usize,
    /// Set by the helpers when code was written (then no block may follow
    /// without the JIT looking first).
    pub changed: u32,
    /// The memory's code generation when the run started.
    pub generation: u64,
    /// A block that would end past this step count must not start.
    pub stop: u64,
    /// Blocks that went on to the next block themselves.
    pub chained: u64,
    pub cpu: *mut Cpu,
    pub mem: *mut Memory,
    pub block: *const Block,
    /// Why the block stopped, when it returns 1.
    pub exit: Option<Exit>,
}

/// Turns IR blocks into something that runs.
pub trait Engine: std::fmt::Debug {
    /// Translate `blocks`; one handle for each, in order.
    fn compile(&mut self, blocks: &[Rc<Block>]) -> Vec<usize>;
    /// Run translated block `handle` (which is `block`), from `cpu.pc == block.pc`.
    /// An engine may go on to more blocks from `link` itself.
    fn call(&mut self, handle: usize, block: &Block, cpu: &mut Cpu, mem: &mut Memory, link: &mut Link) -> Result<(), Exit>;
    /// A fresh engine of the same kind (for a snapshot: translations are not state).
    fn fresh(&self) -> Box<dyn Engine>;
    fn name(&self) -> &'static str;
    /// Whether handle 0 means the block could not be translated.
    fn zero_means_failed(&self) -> bool {
        false
    }
    /// The engine's own numbers for [`Jit::report`] (time spent compiling…).
    fn report(&self) -> String {
        String::new()
    }
}

/// What a run of translated code may go on to.
#[derive(Debug, Clone, Copy, Default)]
pub struct Link {
    /// The table of translated blocks.
    pub fast: usize,
    /// No block that would end past this step count may start.
    pub stop: u64,
    /// Set by the engine: blocks it went on to.
    pub chained: u64,
}

/// The IR evaluator as an engine: slow, for tests and for checking the
/// front end on whole apps (`POMME_CPU=ir-eval`).
#[derive(Debug, Default)]
pub struct EvalEngine;

impl Engine for EvalEngine {
    fn compile(&mut self, blocks: &[Rc<Block>]) -> Vec<usize> {
        vec![0; blocks.len()]
    }

    fn call(&mut self, _: usize, block: &Block, cpu: &mut Cpu, mem: &mut Memory, _: &mut Link) -> Result<(), Exit> {
        eval::run(block, cpu, mem)
    }

    fn fresh(&self) -> Box<dyn Engine> {
        Box::new(EvalEngine)
    }

    fn name(&self) -> &'static str {
        "ir-eval"
    }
}

#[derive(Debug)]
struct Entry {
    block: Rc<Block>,
    /// The engine's handle, once translated.
    code: Option<usize>,
}

/// How many queued blocks are translated together.
const BATCH: usize = 64;
/// Translate a smaller batch after this many interpreted instructions, so
/// blocks don't wait long for the batch to fill.
const FLUSH_EVERY: u64 = 1 << 22;

#[derive(Debug)]
pub struct Jit {
    engine: Box<dyn Engine>,
    entries: PcMap<Entry>,
    /// Translated blocks by pc, checked before `entries`.
    fast: Vec<Fast>,
    /// Block runs on the interpreter, by fast-table slot.
    counts: Vec<u32>,
    queue: Vec<u64>,
    /// The memory's code generation the entries are valid for.
    generation: u64,
    /// Runs on the interpreter before a block is translated.
    pub threshold: u32,
    since_flush: u64,
    /// Counts for [`Jit::report`].
    pub stats: Stats,
    /// Called with [`Jit::report`] every [`REPORT_EVERY`] block runs (for measuring).
    pub log: Option<fn(&str)>,
    next_report: u64,
    /// A clock in ms, for timing what the JIT does (with `log`).
    pub clock: Option<fn() -> f64>,
    /// ms in the interpreter, in translated code, and translating.
    times: [f64; 3],
}

/// What the JIT did, for measuring.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    /// Block runs (and their instructions) through translated code.
    pub runs_translated: u64,
    pub insns_translated: u64,
    /// Calls into translated code from the JIT's loop.
    pub calls: u64,
    /// Block runs (and their instructions) on the interpreter.
    pub runs_interpreted: u64,
    pub insns_interpreted: u64,
    /// Instructions run one at a time (a block that didn't fit before a stop).
    pub single_steps: u64,
    /// Blocks lifted to IR, and dropped because their code changed.
    pub lifted: u64,
    pub dropped: u64,
    /// Blocks handed to the engine.
    pub translated: u64,
}

/// How often [`Jit::log`] gets a report, in block runs.
pub const REPORT_EVERY: u64 = 1 << 24;

impl Jit {
    pub fn new(engine: Box<dyn Engine>) -> Jit {
        Jit {
            engine,
            entries: PcMap::default(),
            fast: vec![NO_FAST; FAST_SLOTS],
            counts: vec![0; FAST_SLOTS],
            queue: Vec::new(),
            generation: 0,
            threshold: 50,
            since_flush: 0,
            stats: Stats::default(),
            log: None,
            next_report: REPORT_EVERY,
            clock: None,
            times: [0.0; 3],
        }
    }

    /// One line on what the JIT did so far.
    pub fn report(&self) -> String {
        let s = &self.stats;
        let insns = (s.insns_translated + s.insns_interpreted + s.single_steps).max(1);
        format!(
            "jit: {:.0} ms interpreting, {:.0} ms in translated code, {:.0} ms translating; {:.1}% of instructions translated ({} block runs, {:.2} insns each, {} entered from pomme), {} interpreted runs, {} single steps; {} blocks lifted, {} dropped, {} translated, {} in the table; {}",
            self.times[0],
            self.times[1],
            self.times[2],
            100.0 * s.insns_translated as f64 / insns as f64,
            s.runs_translated,
            s.insns_translated as f64 / s.runs_translated.max(1) as f64,
            s.calls,
            s.runs_interpreted,
            s.single_steps,
            s.lifted,
            s.dropped,
            s.translated,
            self.entries.len(),
            self.engine.report()
        )
    }

    /// Drop the entries whose code was written.
    fn code_changed(&mut self, mem: &Memory) {
        match mem.code_changes_since(self.generation) {
            Some(pages) => {
                let pages: Vec<u64> = pages.to_vec();
                let before = self.entries.len();
                self.entries.retain(|at, _| !pages.contains(&(untag(*at) / PAGE_SIZE)));
                self.stats.dropped += (before - self.entries.len()) as u64;
            }
            None => {
                self.stats.dropped += self.entries.len() as u64;
                self.entries.clear()
            }
        }
        self.queue.retain(|pc| self.entries.contains_key(pc));
        self.fast.fill(NO_FAST);
        self.generation = mem.code_generation();
    }

    fn now(&self) -> f64 {
        self.clock.map_or(0.0, |c| c())
    }

    fn translate_queue(&mut self) {
        let t = self.now();
        let pcs = std::mem::take(&mut self.queue);
        let blocks: Vec<Rc<Block>> = pcs.iter().map(|pc| self.entries[pc].block.clone()).collect();
        let handles = self.engine.compile(&blocks);
        self.stats.translated += blocks.len() as u64;
        for (pc, h) in pcs.iter().zip(handles) {
            if let Some(e) = self.entries.get_mut(pc) {
                // Handle 0 is "not translated" for an engine that failed (it stays in the map, so it is not tried again).
                e.code = (h != 0 || !self.engine.zero_means_failed()).then_some(h);
                if e.code.is_some() {
                    self.fast[fast_slot(*pc)] = Fast { pc: *pc, handle: h, len: e.block.len() as u64, block: Rc::as_ptr(&e.block) };
                }
            }
        }
        self.since_flush = 0;
        self.times[2] += self.now() - t;
    }

    /// Run translated code from block `block`, counting what ran.
    fn call(&mut self, handle: usize, block: &Block, cpu: &mut Cpu, mem: &mut Memory, stop: u64) -> Result<(), Exit> {
        let mut link = Link { fast: self.fast.as_ptr() as usize, stop, chained: 0 };
        let before = cpu.steps;
        let t = self.now();
        let r = self.engine.call(handle, block, cpu, mem, &mut link);
        self.times[1] += self.now() - t;
        self.stats.runs_translated += 1 + link.chained;
        self.stats.calls += 1;
        self.stats.insns_translated += cpu.steps - before;
        r
    }

    /// A block became hot: lift it and queue it for translation.
    fn hot(&mut self, pc: u64, mem: &mut Memory) {
        if let std::collections::hash_map::Entry::Vacant(v) = self.entries.entry(pc)
            && let Some(ops) = pomme_cpu::fetch_block(pc, mem)
        {
            mem.mark_code(pc);
            self.stats.lifted += 1;
            v.insert(Entry { block: Rc::new(lift(pc, ops)), code: None });
            self.queue.push(pc);
        }
    }

    /// Put translated block `pc` back in the fast table if it lost its slot
    /// to another block; true if it is translated.
    fn refill(&mut self, pc: u64) -> bool {
        match self.entries.get(&pc) {
            Some(e @ Entry { code: Some(h), .. }) => {
                self.fast[fast_slot(pc)] = Fast { pc, handle: *h, len: e.block.len() as u64, block: Rc::as_ptr(&e.block) };
                true
            }
            _ => false,
        }
    }
}

impl Backend for Jit {
    fn run(&mut self, cpu: &mut Cpu, mem: &mut Memory, budget: Budget) -> Exit {
        // The same stops as `Cpu::run`: the budget, or the next profiler sample.
        let limit = cpu.steps.saturating_add(budget.steps);
        let stop = if cpu.sample_at == 0 { limit } else { limit.min(cpu.sample_at) };
        let mut hot = Vec::new();
        loop {
            if cpu.pc == HOST_RETURN {
                return Exit::Returned;
            }
            if cpu.steps >= stop {
                return if cpu.sample_at != 0 && cpu.steps >= cpu.sample_at { Exit::Sample } else { Exit::StepLimit };
            }
            if self.generation != mem.code_generation() {
                self.code_changed(mem);
            }
            let pc = cpu.pc;
            let f = self.fast[fast_slot(pc)];
            let r = if f.pc == pc {
                if f.len <= stop - cpu.steps {
                    // SAFETY: `f.block` belongs to an entry still in
                    // `entries` (the table is cleared whenever entries go).
                    self.call(f.handle, unsafe { &*f.block }, cpu, mem, stop)
                } else {
                    // Doesn't fit before the stop: one instruction.
                    self.stats.single_steps += 1;
                    cpu.step(mem)
                }
            } else {
                // The interpreter, until it reaches a translated block. It
                // counts block runs in `counts` (a direct-mapped table: a
                // shared slot only makes a block hot sooner).
                let (fast, counts, threshold) = (&self.fast, &mut self.counts, self.threshold);
                let mut runs = 0u64;
                let before = cpu.steps;
                let t = self.clock.map_or(0.0, |c| c());
                let exit = cpu.run_until(mem, stop - cpu.steps, |pc| {
                    let slot = fast_slot(pc);
                    if fast[slot].pc == pc {
                        return true;
                    }
                    runs += 1;
                    let c = &mut counts[slot];
                    *c = c.saturating_add(1);
                    // Hot now; or hot before, and maybe translated but pushed
                    // out of the fast table by another block.
                    if *c == threshold || (*c > threshold && *c % 256 == 0) {
                        hot.push(pc);
                    }
                    false
                });
                self.times[0] += self.now() - t;
                self.stats.runs_interpreted += runs;
                self.stats.insns_interpreted += cpu.steps - before;
                self.since_flush += cpu.steps - before;
                for pc in hot.drain(..) {
                    if !self.refill(pc) {
                        self.hot(pc, mem);
                    }
                }
                if self.queue.len() >= BATCH || (!self.queue.is_empty() && self.since_flush >= FLUSH_EVERY) {
                    self.translate_queue();
                }
                match exit {
                    None | Some(Exit::StepLimit) | Some(Exit::Returned) => Ok(()),
                    Some(exit) => Err(exit),
                }
            };
            if let Some(log) = self.log {
                let runs = self.stats.runs_translated + self.stats.runs_interpreted + self.stats.single_steps;
                if runs >= self.next_report {
                    self.next_report = runs + REPORT_EVERY;
                    log(&self.report());
                }
            }
            if let Err(exit) = r {
                return exit;
            }
        }
    }

    fn name(&self) -> &'static str {
        "jit"
    }

    fn box_clone(&self) -> Box<dyn Backend> {
        let mut j = Jit::new(self.engine.fresh());
        j.threshold = self.threshold;
        j.log = self.log;
        j.clock = self.clock;
        Box::new(j)
    }
}
