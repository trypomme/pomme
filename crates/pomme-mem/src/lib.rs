//! The guest's address space: a sorted set of non-overlapping regions.
//!
//! Shared by the loader (which fills it) and the CPU (which executes from it).
//!
//! One `Vec<u8>` per region, found by binary search, with a small cache in
//! front (a software TLB): most accesses hit a page used a moment ago, and
//! a hit on a page that lies wholly in one region reads or writes its bytes
//! directly, without looking at the region again.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prot {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

impl Prot {
    pub const RW: Prot = Prot { read: true, write: true, exec: false };
    pub const RX: Prot = Prot { read: true, write: false, exec: true };
}

impl std::fmt::Display for Prot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = |b, ch| if b { ch } else { '-' };
        write!(f, "{}{}{}", c(self.read, 'r'), c(self.write, 'w'), c(self.exec, 'x'))
    }
}

pub const PAGE_SIZE: u64 = 0x4000; // arm64 iOS uses 16 KiB pages.

/// arm64 Darwin runs user space with Top Byte Ignore: data accesses ignore
/// address bits 56–63, and compilers rely on it (Swift keeps spare bits there
/// and LLVM leaves out the masking). Every data access goes through this.
#[inline(always)]
pub fn untag(addr: u64) -> u64 {
    addr & 0x00ff_ffff_ffff_ffff
}

pub fn page_align_up(x: u64) -> u64 {
    (x + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

#[derive(Debug, Clone)]
pub struct Region {
    pub base: u64,
    pub data: Vec<u8>,
    pub prot: Prot,
    pub name: String,
    /// The CPU has cached decoded code from here: writing here changes the code generation.
    has_code: bool,
}

impl Region {
    pub fn end(&self) -> u64 {
        self.base + self.data.len() as u64
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemError {
    #[error("unmapped address {0:#x}")]
    Unmapped(u64),
    #[error("region {name} at {base:#x}+{size:#x} overlaps {other}")]
    Overlap { name: String, base: u64, size: u64, other: String },
    #[error("an allocation of {0:#x} bytes is more than the device has")]
    TooLarge(u64),
}

/// TLB slots (direct-mapped by page number).
const TLB_SLOTS: usize = 1024;

/// A page recently found in a region.
#[derive(Debug, Clone, Copy)]
struct TlbEntry {
    /// The page number (untagged address / [`PAGE_SIZE`]); `u64::MAX` when empty.
    page: u64,
    region: u32,
    /// Where the page's first byte is in the region's data, when the whole
    /// page lies in the region (0 when it doesn't): loads go straight there.
    host: usize,
    /// `host` came from a mutable borrow and the region holds no cached
    /// code: stores may go straight there too.
    writable: bool,
}

impl TlbEntry {
    const EMPTY: TlbEntry = TlbEntry { page: u64::MAX, region: 0, host: 0, writable: false };
}

#[derive(Debug)]
pub struct Memory {
    regions: Vec<Region>,
    /// Page → region index (and the page's bytes) for recent accesses. A hit
    /// is checked against the region's bounds, so pages shared by two regions
    /// stay correct. Emptied whenever regions move (a map) or a region starts
    /// holding cached code. Regions' data never grows, shrinks or goes away,
    /// so the bytes an entry points at stay where they are.
    tlb: Box<[std::cell::Cell<TlbEntry>]>,
    /// Bumped whenever code the CPU may have cached is written (see [`Memory::mark_code`]).
    code_generation: u64,
}

/// A copy of the address space (a snapshot's): the regions and their
/// contents, with the same generations, so code the CPU cached for one is
/// valid for the other; the TLB starts empty.
impl Clone for Memory {
    fn clone(&self) -> Self {
        Memory { regions: self.regions.clone(), ..Memory { code_generation: self.code_generation, ..Default::default() } }
    }
}

impl Default for Memory {
    fn default() -> Self {
        Memory { regions: Vec::new(), tlb: vec![std::cell::Cell::new(TlbEntry::EMPTY); TLB_SLOTS].into_boxed_slice(), code_generation: 0 }
    }
}

impl Memory {
    #[inline(always)]
    fn slot(&self, addr: u64) -> &std::cell::Cell<TlbEntry> {
        &self.tlb[(addr / PAGE_SIZE) as usize & (TLB_SLOTS - 1)]
    }

    /// The index of the region holding `addr` (untagged).
    #[inline(always)]
    fn find(&self, addr: u64) -> Option<usize> {
        let page = addr / PAGE_SIZE;
        let slot = self.slot(addr);
        let e = slot.get();
        if e.page == page {
            let r = &self.regions[e.region as usize];
            if addr >= r.base && addr < r.end() {
                return Some(e.region as usize);
            }
        }
        let i = self.regions.partition_point(|r| r.base <= addr).checked_sub(1)?;
        let r = &self.regions[i];
        if addr >= r.end() {
            return None;
        }
        let host = if page * PAGE_SIZE >= r.base && (page + 1) * PAGE_SIZE <= r.end() {
            r.data.as_ptr() as usize + (page * PAGE_SIZE - r.base) as usize
        } else {
            0
        };
        slot.set(TlbEntry { page, region: i as u32, host, writable: false });
        Some(i)
    }

    /// The host address of `addr` (untagged) when the TLB has its page and
    /// `N` bytes from it stay in the page (and, for a store, it may be written directly).
    #[inline(always)]
    fn direct<const N: usize>(&self, addr: u64, store: bool) -> Option<usize> {
        let e = self.slot(addr).get();
        let off = (addr % PAGE_SIZE) as usize;
        (e.page == addr / PAGE_SIZE && e.host != 0 && off + N <= PAGE_SIZE as usize && (!store || e.writable)).then_some(e.host + off)
    }

    fn flush_tlb(&self) {
        for slot in self.tlb.iter() {
            slot.set(TlbEntry::EMPTY);
        }
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    pub fn mapped_bytes(&self) -> u64 {
        self.regions.iter().map(|r| r.data.len() as u64).sum()
    }

    /// Map `size` zeroed bytes at `base`.
    pub fn map(&mut self, base: u64, size: u64, prot: Prot, name: impl Into<String>) -> Result<&mut Region, MemError> {
        let name = name.into();
        let idx = self.regions.partition_point(|r| r.base < base);
        let clash = [idx.checked_sub(1), Some(idx)]
            .into_iter()
            .flatten()
            .filter_map(|i| self.regions.get(i))
            .find(|r| base < r.end() && r.base < base + size);
        if let Some(other) = clash {
            return Err(MemError::Overlap { name, base, size, other: other.name.clone() });
        }
        self.regions.insert(idx, Region { base, data: vec![0; size as usize], prot, name, has_code: false });
        // Indices after it moved.
        self.flush_tlb();
        Ok(&mut self.regions[idx])
    }

    /// The CPU caches decoded code from `addr`'s region: writes there must
    /// tell it (they bump [`Memory::code_generation`]).
    pub fn mark_code(&mut self, addr: u64) {
        if let Some(r) = self.region_of_mut(untag(addr))
            && !r.has_code
        {
            r.has_code = true;
            // Its pages' stores must go the slow way now, to see the code.
            self.flush_tlb();
        }
    }

    /// Changes whenever code the CPU may have cached was written (a hook, a trampoline).
    #[inline(always)]
    pub fn code_generation(&self) -> u64 {
        self.code_generation
    }

    #[inline(always)]
    pub fn region_of(&self, addr: u64) -> Option<&Region> {
        self.find(addr).map(|i| &self.regions[i])
    }

    #[inline(always)]
    fn region_of_mut(&mut self, addr: u64) -> Option<&mut Region> {
        self.find(addr).map(|i| &mut self.regions[i])
    }

    #[inline]
    pub fn read(&self, addr: u64, len: usize) -> Result<&[u8], MemError> {
        let addr = untag(addr);
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        r.data.get(off..off + len).ok_or(MemError::Unmapped(addr + len as u64))
    }

    #[inline]
    pub fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MemError> {
        let addr = untag(addr);
        let i = self.find(addr).ok_or(MemError::Unmapped(addr))?;
        let r = &mut self.regions[i];
        let off = (addr - r.base) as usize;
        let dst = r
            .data
            .get_mut(off..off + bytes.len())
            .ok_or(MemError::Unmapped(addr + bytes.len() as u64))?;
        dst.copy_from_slice(bytes);
        if r.has_code {
            self.code_generation += 1;
        }
        Ok(())
    }

    /// `N` bytes at `addr`, as an array: fixed-size accesses compile to plain loads.
    #[inline(always)]
    fn read_n<const N: usize>(&self, addr: u64) -> Result<[u8; N], MemError> {
        let addr = untag(addr);
        if let Some(p) = self.direct::<N>(addr, false) {
            // SAFETY: `direct` found the page in the TLB, which points at a
            // whole page of a region's data (regions' data never moves or
            // goes away), and the N bytes stay inside that page.
            return Ok(unsafe { std::ptr::read_unaligned(p as *const [u8; N]) });
        }
        self.read_n_slow(addr)
    }

    #[inline(never)]
    fn read_n_slow<const N: usize>(&self, addr: u64) -> Result<[u8; N], MemError> {
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        match r.data.get(off..off + N) {
            Some(b) => Ok(b.try_into().unwrap()),
            None => Err(MemError::Unmapped(addr + N as u64)),
        }
    }

    /// Write `N` bytes at `addr` (fixed size: plain stores).
    #[inline(always)]
    fn write_n<const N: usize>(&mut self, addr: u64, bytes: [u8; N]) -> Result<(), MemError> {
        let addr = untag(addr);
        if let Some(p) = self.direct::<N>(addr, true) {
            // SAFETY: as in `read_n`; the entry is writable, so the pointer
            // came from the region's data mutably and the region has no code.
            unsafe { std::ptr::write_unaligned(p as *mut [u8; N], bytes) };
            return Ok(());
        }
        self.write_n_slow(addr, bytes)
    }

    #[inline(never)]
    fn write_n_slow<const N: usize>(&mut self, addr: u64, bytes: [u8; N]) -> Result<(), MemError> {
        let i = self.find(addr).ok_or(MemError::Unmapped(addr))?;
        let r = &mut self.regions[i];
        let off = (addr - r.base) as usize;
        let dst: &mut [u8; N] = r.data.get_mut(off..off + N).ok_or(MemError::Unmapped(addr + N as u64))?.try_into().unwrap();
        *dst = bytes;
        if r.has_code {
            self.code_generation += 1;
        } else {
            // Later stores to this page can go straight to it.
            let page = addr / PAGE_SIZE;
            if page * PAGE_SIZE >= r.base && (page + 1) * PAGE_SIZE <= r.end() {
                let host = r.data.as_mut_ptr() as usize + (page * PAGE_SIZE - r.base) as usize;
                self.slot(addr).set(TlbEntry { page, region: i as u32, host, writable: true });
            }
        }
        Ok(())
    }

    #[inline]
    pub fn read_u8(&self, addr: u64) -> Result<u8, MemError> {
        Ok(self.read_n::<1>(addr)?[0])
    }

    #[inline]
    pub fn read_u16(&self, addr: u64) -> Result<u16, MemError> {
        Ok(u16::from_le_bytes(self.read_n(addr)?))
    }

    #[inline]
    pub fn read_u32(&self, addr: u64) -> Result<u32, MemError> {
        Ok(u32::from_le_bytes(self.read_n(addr)?))
    }

    #[inline]
    pub fn read_u64(&self, addr: u64) -> Result<u64, MemError> {
        Ok(u64::from_le_bytes(self.read_n(addr)?))
    }

    #[inline]
    pub fn read_u128(&self, addr: u64) -> Result<u128, MemError> {
        Ok(u128::from_le_bytes(self.read_n(addr)?))
    }

    #[inline]
    pub fn write_u8(&mut self, addr: u64, v: u8) -> Result<(), MemError> {
        self.write_n(addr, [v])
    }

    #[inline]
    pub fn write_u16(&mut self, addr: u64, v: u16) -> Result<(), MemError> {
        self.write_n(addr, v.to_le_bytes())
    }

    #[inline]
    pub fn write_u32(&mut self, addr: u64, v: u32) -> Result<(), MemError> {
        self.write_n(addr, v.to_le_bytes())
    }

    #[inline]
    pub fn write_u64(&mut self, addr: u64, v: u64) -> Result<(), MemError> {
        self.write_n(addr, v.to_le_bytes())
    }

    #[inline]
    pub fn write_u128(&mut self, addr: u64, v: u128) -> Result<(), MemError> {
        self.write_n(addr, v.to_le_bytes())
    }

    pub fn read_cstr(&self, addr: u64, max: usize) -> Result<String, MemError> {
        Ok(String::from_utf8_lossy(&self.read_cstr_bytes(addr, max)?).into_owned())
    }

    /// The length of the NUL-terminated string at `addr` (to its region's end if unterminated).
    pub fn cstr_len(&self, addr: u64) -> Result<usize, MemError> {
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let bytes = &r.data[(addr - r.base) as usize..];
        Ok(bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len()))
    }

    /// A NUL-terminated string's bytes (without the NUL), as they are.
    pub fn read_cstr_bytes(&self, addr: u64, max: usize) -> Result<Vec<u8>, MemError> {
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        let bytes = &r.data[off..r.data.len().min(off.saturating_add(max))];
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(bytes[..end].to_vec())
    }
}
