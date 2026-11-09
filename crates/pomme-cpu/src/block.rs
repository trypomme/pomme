//! Basic blocks: straight-line runs of instructions, fetched from memory once
//! and kept, so a loop or a hot function doesn't go back to guest memory for
//! every instruction it runs again.
//!
//! A block starts wherever execution arrives and ends at the first
//! instruction that can change the flow (a branch, an exception or system
//! instruction), at the end of its 16 KiB page, or at [`MAX_LEN`]. Blocks
//! are dropped when code they came from is written ([`Memory::code_generation`]):
//! pomme hooking a function, or writing a trampoline.
//!
//! Executing a block is exactly executing its instructions one by one: the
//! same results, the same instruction count at every exit (a fault mid-block
//! stops at that instruction), and a block only runs whole when it fits
//! before the next stop (the step budget or a profiler sample), so threads
//! switch and samples land at the same instructions as without blocks.

use pomme_mem::{Memory, PAGE_SIZE};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The longest block, in instructions.
pub(crate) const MAX_LEN: usize = 32;

/// Whether an instruction may change the flow (it ends a block): the branch,
/// exception-generating and system encodings.
#[inline(always)]
pub(crate) fn ends_block(insn: u32) -> bool {
    matches!((insn >> 25) & 0xf, 0b1010 | 0b1011)
}

/// Hashing a code address: a multiply mixes it well enough, much faster than SipHash.
#[derive(Default)]
pub(crate) struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (v >> 2).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

#[derive(Clone, Default)]
pub struct Blocks {
    map: HashMap<u64, Box<[u32]>, BuildHasherDefault<AddrHasher>>,
    /// The memory's code generation these blocks were fetched at.
    generation: u64,
}

impl std::fmt::Debug for Blocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} blocks", self.map.len())
    }
}

impl Blocks {
    /// The block at `pc`, fetching it the first time. None if `pc` can't be
    /// fetched (the caller's single step reports the fault).
    #[inline]
    pub(crate) fn get(&mut self, pc: u64, mem: &mut Memory) -> Option<&[u32]> {
        if self.generation != mem.code_generation() {
            self.map.clear();
            self.generation = mem.code_generation();
        }
        if !self.map.contains_key(&pc) {
            let block = fetch(pc, mem)?;
            mem.mark_code(pc);
            self.map.insert(pc, block);
        }
        self.map.get(&pc).map(|b| &b[..])
    }
}

fn fetch(pc: u64, mem: &Memory) -> Option<Box<[u32]>> {
    let page_end = (pc | (PAGE_SIZE - 1)) + 1;
    let mut insns = Vec::new();
    let mut a = pc;
    while a < page_end && insns.len() < MAX_LEN {
        let insn = mem.read_u32(a).ok()?;
        insns.push(insn);
        if ends_block(insn) {
            break;
        }
        a += 4;
    }
    (!insns.is_empty()).then(|| insns.into_boxed_slice())
}
