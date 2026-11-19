//! Basic blocks: straight-line runs of instructions, fetched from memory and
//! decoded once (see fast.rs) and kept, so a loop or a hot function doesn't
//! fetch and decode every instruction again each time it runs.
//!
//! A block starts wherever execution arrives and ends at the first
//! instruction that can change the flow (a branch, an exception or system
//! instruction), at the end of its 16 KiB page, or at [`MAX_LEN`]. A page's
//! blocks are dropped when code in that page is written
//! ([`Memory::code_changes_since`]): pomme hooking a function, or writing a
//! trampoline. Blocks of other pages stay.
//!
//! Executing a block is exactly executing its instructions one by one: the
//! same results, the same instruction count at every exit (a fault mid-block
//! stops at that instruction), and a block only runs whole when it fits
//! before the next stop (the step budget or a profiler sample), so threads
//! switch and samples land at the same instructions as without blocks.

use crate::fast::{Op, decode};
use pomme_mem::{Memory, PAGE_SIZE};

/// The longest block, in instructions.
pub(crate) const MAX_LEN: usize = 32;

/// Whether an instruction may change the flow (it ends a block): the branch,
/// exception-generating and system encodings.
#[inline(always)]
pub(crate) fn ends_block(insn: u32) -> bool {
    matches!((insn >> 25) & 0xf, 0b1010 | 0b1011)
}

/// Block slots, direct-mapped by address: a lookup is one array read. Two
/// hot blocks sharing a slot just fetch each other out (fetching is cheap).
const SLOTS: usize = 1 << 16;

#[derive(Clone)]
pub struct Blocks {
    slots: Vec<Option<(u64, Box<[Op]>)>>,
    /// The memory's code generation these blocks were fetched at.
    generation: u64,
}

impl Default for Blocks {
    fn default() -> Self {
        Blocks { slots: Vec::new(), generation: 0 }
    }
}

impl std::fmt::Debug for Blocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} blocks", self.slots.iter().flatten().count())
    }
}

impl Blocks {
    /// The block at `pc`, fetching it the first time. None if `pc` can't be
    /// fetched (the caller's single step reports the fault).
    #[inline]
    pub(crate) fn get(&mut self, pc: u64, mem: &mut Memory) -> Option<&[Op]> {
        if self.generation != mem.code_generation() || self.slots.is_empty() {
            self.code_changed(mem);
        }
        let i = (pc as usize >> 2) & (SLOTS - 1);
        if !matches!(&self.slots[i], Some((at, _)) if *at == pc) {
            let block = fetch(pc, mem)?;
            #[cfg(feature = "stats")]
            crate::stats::with(|s| s.blocks_fetched += 1);
            mem.mark_code(pc);
            self.slots[i] = Some((pc, block));
        }
        self.slots[i].as_ref().map(|(_, b)| &b[..])
    }
}

impl Blocks {
    /// Code was written since these blocks were fetched: drop the blocks of
    /// the pages that changed, or every block when that is unknown.
    #[cold]
    fn code_changed(&mut self, mem: &Memory) {
        match mem.code_changes_since(self.generation) {
            Some(pages) if !self.slots.is_empty() => {
                for &page in pages {
                    // A page's blocks can only be in the slots its instructions map to.
                    let first = ((page * PAGE_SIZE) as usize >> 2) & (SLOTS - 1);
                    for slot in &mut self.slots[first..first + PAGE_INSNS] {
                        if matches!(slot, Some((at, _)) if pomme_mem::untag(*at) / PAGE_SIZE == page) {
                            *slot = None;
                        }
                    }
                }
            }
            _ => {
                #[cfg(feature = "stats")]
                if !self.slots.is_empty() {
                    crate::stats::with(|s| s.cache_flushes += 1);
                }
                self.slots.clear();
                self.slots.resize(SLOTS, None);
            }
        }
        self.generation = mem.code_generation();
    }
}

/// Instructions in a page: a page's blocks map to this many consecutive slots.
const PAGE_INSNS: usize = (PAGE_SIZE / 4) as usize;
const _: () = assert!(SLOTS.is_multiple_of(PAGE_INSNS));

pub(crate) fn fetch(pc: u64, mem: &Memory) -> Option<Box<[Op]>> {
    let page_end = (pc | (PAGE_SIZE - 1)) + 1;
    // Decoded on the stack, then copied into one allocation of the right size.
    let mut ops = [Op::Nop; MAX_LEN];
    let mut n = 0;
    let mut a = pc;
    while a < page_end && n < MAX_LEN {
        let insn = mem.read_u32(a).ok()?;
        ops[n] = decode(insn, a);
        n += 1;
        if ends_block(insn) {
            break;
        }
        a += 4;
    }
    (n > 0).then(|| Box::from(&ops[..n]))
}
