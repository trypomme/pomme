//! The guest's address space: a sorted set of non-overlapping regions.
//!
//! Shared by the loader (which fills it) and the CPU (which executes from it).
//!
//! Deliberately simple for now (one `Vec<u8>` per region). The CPU will
//! eventually want a page-table-like fast path, but the API stays the same.

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

pub fn page_align_up(x: u64) -> u64 {
    (x + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
}

#[derive(Debug)]
pub struct Region {
    pub base: u64,
    pub data: Vec<u8>,
    pub prot: Prot,
    pub name: String,
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
}

#[derive(Debug, Default)]
pub struct Memory {
    regions: Vec<Region>,
}

impl Memory {
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
        self.regions.insert(idx, Region { base, data: vec![0; size as usize], prot, name });
        Ok(&mut self.regions[idx])
    }

    pub fn region_of(&self, addr: u64) -> Option<&Region> {
        let idx = self.regions.partition_point(|r| r.base <= addr).checked_sub(1)?;
        let r = &self.regions[idx];
        (addr < r.end()).then_some(r)
    }

    fn region_of_mut(&mut self, addr: u64) -> Option<&mut Region> {
        let idx = self.regions.partition_point(|r| r.base <= addr).checked_sub(1)?;
        let r = &mut self.regions[idx];
        (addr < r.end()).then_some(r)
    }

    pub fn read(&self, addr: u64, len: usize) -> Result<&[u8], MemError> {
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        r.data.get(off..off + len).ok_or(MemError::Unmapped(addr + len as u64))
    }

    pub fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), MemError> {
        let r = self.region_of_mut(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        let dst = r
            .data
            .get_mut(off..off + bytes.len())
            .ok_or(MemError::Unmapped(addr + bytes.len() as u64))?;
        dst.copy_from_slice(bytes);
        Ok(())
    }

    pub fn read_u8(&self, addr: u64) -> Result<u8, MemError> {
        Ok(self.read(addr, 1)?[0])
    }

    pub fn read_u16(&self, addr: u64) -> Result<u16, MemError> {
        Ok(u16::from_le_bytes(self.read(addr, 2)?.try_into().unwrap()))
    }

    pub fn read_u128(&self, addr: u64) -> Result<u128, MemError> {
        Ok(u128::from_le_bytes(self.read(addr, 16)?.try_into().unwrap()))
    }

    pub fn read_u32(&self, addr: u64) -> Result<u32, MemError> {
        Ok(u32::from_le_bytes(self.read(addr, 4)?.try_into().unwrap()))
    }

    pub fn read_u64(&self, addr: u64) -> Result<u64, MemError> {
        Ok(u64::from_le_bytes(self.read(addr, 8)?.try_into().unwrap()))
    }

    pub fn write_u64(&mut self, addr: u64, v: u64) -> Result<(), MemError> {
        self.write(addr, &v.to_le_bytes())
    }

    pub fn write_u8(&mut self, addr: u64, v: u8) -> Result<(), MemError> {
        self.write(addr, &[v])
    }

    pub fn write_u16(&mut self, addr: u64, v: u16) -> Result<(), MemError> {
        self.write(addr, &v.to_le_bytes())
    }

    pub fn write_u32(&mut self, addr: u64, v: u32) -> Result<(), MemError> {
        self.write(addr, &v.to_le_bytes())
    }

    pub fn write_u128(&mut self, addr: u64, v: u128) -> Result<(), MemError> {
        self.write(addr, &v.to_le_bytes())
    }

    pub fn read_cstr(&self, addr: u64, max: usize) -> Result<String, MemError> {
        let r = self.region_of(addr).ok_or(MemError::Unmapped(addr))?;
        let off = (addr - r.base) as usize;
        let bytes = &r.data[off..r.data.len().min(off + max)];
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
}
