//! The guest heap: a deterministic `malloc` over guest memory.
//!
//! Size-class free lists in power-of-two buckets, bump allocation from 64 MiB
//! chunks. Each block has a 16-byte header holding its usable size, so
//! `free`/`realloc`/`malloc_size` work. Same calls in the same order always
//! produce the same addresses.

use pomme_mem::{MemError, Memory, Prot, page_align_up};

pub const HEAP_BASE: u64 = 0x4_0000_0000;
const CHUNK: u64 = 64 << 20;
const HEADER: u64 = 16;

#[derive(Debug, Default)]
pub struct Heap {
    /// Bump pointer and end of the current chunk.
    next: u64,
    end: u64,
    /// Where the next chunk goes.
    top: u64,
    /// Free blocks by bucket (block size 16 << i).
    free: Vec<Vec<u64>>,
    pub allocated: u64,
}

fn bucket(size: u64) -> (usize, u64) {
    let size = size.max(16).next_power_of_two();
    (size.trailing_zeros() as usize - 4, size)
}

impl Heap {
    pub fn new() -> Self {
        Heap { top: HEAP_BASE, free: vec![Vec::new(); 48], ..Default::default() }
    }

    /// Allocate `size` zeroed bytes, 16-byte aligned.
    pub fn alloc(&mut self, mem: &mut Memory, size: u64) -> Result<u64, MemError> {
        let (b, block) = bucket(size);
        let addr = if let Some(a) = self.free[b].pop() {
            mem.write(a, &vec![0; block as usize])?;
            a
        } else {
            if self.next + HEADER + block > self.end {
                let len = page_align_up((HEADER + block).max(CHUNK));
                mem.map(self.top, len, Prot::RW, "[heap]")?;
                self.next = self.top;
                self.end = self.top + len;
                self.top += len + pomme_mem::PAGE_SIZE; // guard page between chunks
            }
            let a = self.next + HEADER;
            self.next = a + block;
            a
        };
        mem.write_u64(addr - HEADER, block)?;
        self.allocated += block;
        Ok(addr)
    }

    /// Usable size of a block from [`alloc`](Self::alloc).
    pub fn size(&self, mem: &Memory, addr: u64) -> Result<u64, MemError> {
        mem.read_u64(addr - HEADER)
    }

    pub fn free(&mut self, mem: &Memory, addr: u64) -> Result<(), MemError> {
        if addr == 0 {
            return Ok(());
        }
        let block = self.size(mem, addr)?;
        self.allocated -= block;
        self.free[bucket(block).0].push(addr);
        Ok(())
    }

    pub fn realloc(&mut self, mem: &mut Memory, addr: u64, size: u64) -> Result<u64, MemError> {
        if addr == 0 {
            return self.alloc(mem, size);
        }
        let old = self.size(mem, addr)?;
        if size <= old {
            return Ok(addr);
        }
        let new = self.alloc(mem, size)?;
        let bytes = mem.read(addr, old as usize)?.to_vec();
        mem.write(new, &bytes)?;
        self.free(mem, addr)?;
        Ok(new)
    }

    /// Allocate a NUL-terminated copy of `s`.
    pub fn cstr(&mut self, mem: &mut Memory, s: &str) -> Result<u64, MemError> {
        let a = self.alloc(mem, s.len() as u64 + 1)?;
        mem.write(a, s.as_bytes())?;
        Ok(a)
    }
}
