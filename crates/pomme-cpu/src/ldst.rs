//! Loads and stores: single registers, pairs, literals, exclusives, acquire/release
//! and LSE atomics. Integer and SIMD/FP registers share the address logic.

use crate::{Cpu, Exit, Step, bit, bits, sext};
use pomme_mem::Memory;

fn load(mem: &Memory, addr: u64, bytes: u32) -> Result<u128, Exit> {
    Ok(match bytes {
        1 => mem.read_u8(addr)? as u128,
        2 => mem.read_u16(addr)? as u128,
        4 => mem.read_u32(addr)? as u128,
        8 => mem.read_u64(addr)? as u128,
        _ => mem.read_u128(addr)?,
    })
}

fn store(mem: &mut Memory, addr: u64, bytes: u32, v: u128) -> Result<(), Exit> {
    match bytes {
        1 => mem.write_u8(addr, v as u8)?,
        2 => mem.write_u16(addr, v as u16)?,
        4 => mem.write_u32(addr, v as u32)?,
        8 => mem.write_u64(addr, v as u64)?,
        _ => mem.write_u128(addr, v)?,
    }
    Ok(())
}

fn mask(bytes: u32) -> u64 {
    if bytes >= 8 { u64::MAX } else { (1u64 << (bytes * 8)) - 1 }
}

/// What an integer load/store does with the register.
#[derive(Clone, Copy)]
enum Kind {
    Store,
    /// Load, zero-extending.
    Load,
    /// Load, sign-extending to 64 (`true`) or 32 bits.
    LoadSigned(bool),
    Prefetch,
}

impl Cpu {
    /// Post-index writeback for SIMD structure loads/stores: Rm = 31 means "by the transfer size".
    fn simd_writeback(&mut self, insn: u32, rn: u32, base: u64, total: u64) {
        if bit(insn, 23) {
            let rm = bits(insn, 20, 16);
            let off = if rm == 31 { total } else { self.xr(rm) };
            self.set_xsp(rn, base.wrapping_add(off));
        }
    }

    /// LD1–LD4 / ST1–ST4 (multiple structures), per the Arm pseudocode.
    fn simd_ldst_multiple(&mut self, insn: u32, mem: &mut Memory) -> Step {
        let (rt, rn) = (bits(insn, 4, 0), bits(insn, 9, 5));
        let (q, is_load) = (bit(insn, 30), bit(insn, 22));
        let ebytes = 1u32 << bits(insn, 11, 10);
        let (rpt, selem) = match bits(insn, 15, 12) {
            0b0000 => (1, 4),
            0b0010 => (4, 1),
            0b0100 => (1, 3),
            0b0110 => (3, 1),
            0b0111 => (1, 1),
            0b1000 => (1, 2),
            0b1010 => (2, 1),
            _ => return Err(Exit::Unimplemented { insn }),
        };
        if ebytes == 8 && !q && selem > 1 {
            return Err(Exit::Unimplemented { insn }); // reserved
        }
        let elements = (if q { 16 } else { 8 }) / ebytes;
        let base = self.xsp(rn);
        let mut offs = 0u64;
        if is_load && !q {
            // A 64-bit form writes the whole register: the top half becomes zero.
            for r in 0..rpt * selem {
                self.q[((rt + r) % 32) as usize] = 0;
            }
        }
        for r in 0..rpt {
            for e in 0..elements {
                let mut tt = (rt + r) % 32;
                for _ in 0..selem {
                    let addr = base.wrapping_add(offs);
                    let shift = e * ebytes * 8;
                    let emask = if ebytes == 16 { u128::MAX } else { ((1u128 << (ebytes * 8)) - 1) << shift };
                    if is_load {
                        let v = load(mem, addr, ebytes)?;
                        let reg = &mut self.q[tt as usize];
                        *reg = (*reg & !emask) | ((v << shift) & emask);
                    } else {
                        let v = (self.q[tt as usize] & emask) >> shift;
                        store(mem, addr, ebytes, v)?;
                    }
                    offs += ebytes as u64;
                    tt = (tt + 1) % 32;
                }
            }
        }
        self.simd_writeback(insn, rn, base, offs);
        Ok(())
    }

    /// LD1–LD4 / ST1–ST4 (single lane) and LD1R–LD4R (load and replicate).
    fn simd_ldst_single(&mut self, insn: u32, mem: &mut Memory) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let (rt, rn) = (bits(insn, 4, 0), bits(insn, 9, 5));
        let (q, is_load) = (bit(insn, 30), bit(insn, 22));
        let opcode = bits(insn, 15, 13);
        let (s, size) = (bit(insn, 12) as u32, bits(insn, 11, 10));
        let selem = (((opcode & 1) << 1) | bit(insn, 21) as u32) + 1;
        let qv = q as u32;
        let (ebytes, index, replicate) = match opcode >> 1 {
            0 => (1, (qv << 3) | (s << 2) | size, false),
            1 if size & 1 == 0 => (2, (qv << 2) | (s << 1) | (size >> 1), false),
            2 if size == 0 => (4, (qv << 1) | s, false),
            2 if size == 1 && s == 0 => (8, qv, false),
            3 if is_load && s == 0 => (1 << size, 0, true),
            _ => return unimpl,
        };
        let base = self.xsp(rn);
        let mut offs = 0u64;
        for k in 0..selem {
            let tt = ((rt + k) % 32) as usize;
            let addr = base.wrapping_add(offs);
            if replicate {
                let v = load(mem, addr, ebytes)?;
                let lanes = (if q { 16 } else { 8 }) / ebytes;
                let mut r = 0u128;
                for i in 0..lanes {
                    r |= v << (i * ebytes * 8);
                }
                self.q[tt] = r;
            } else {
                let shift = index * ebytes * 8;
                let emask = ((1u128 << (ebytes * 8)) - 1) << shift;
                if is_load {
                    let v = load(mem, addr, ebytes)?;
                    self.q[tt] = (self.q[tt] & !emask) | (v << shift);
                } else {
                    store(mem, addr, ebytes, (self.q[tt] & emask) >> shift)?;
                }
            }
            offs += ebytes as u64;
        }
        self.simd_writeback(insn, rn, base, offs);
        Ok(())
    }

    pub(crate) fn ldst(&mut self, insn: u32, mem: &mut Memory) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let rt = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let vector = bit(insn, 26);

        // Load register (literal)
        if insn & 0x3b00_0000 == 0x1800_0000 {
            let addr = self.pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64);
            let opc = bits(insn, 31, 30);
            if vector {
                let bytes = 4 << opc;
                if opc == 3 {
                    return unimpl;
                }
                self.q[rt as usize] = load(mem, addr, bytes)?;
            } else {
                match opc {
                    0 => self.set_xr(rt, mem.read_u32(addr)? as u64),
                    1 => self.set_xr(rt, mem.read_u64(addr)?),
                    2 => self.set_xr(rt, mem.read_u32(addr)? as i32 as i64 as u64),
                    _ => {} // PRFM
                }
            }
            return Ok(());
        }

        // Load/store exclusive, acquire/release, compare-and-swap
        if insn & 0x3f00_0000 == 0x0800_0000 {
            return self.ldst_exclusive(insn, mem);
        }

        // Advanced SIMD load/store multiple structures (LD1–LD4 / ST1–ST4), optionally post-indexed
        if insn & 0xbfbf_0000 == 0x0c00_0000 || insn & 0xbfa0_0000 == 0x0c80_0000 {
            return self.simd_ldst_multiple(insn, mem);
        }
        // Advanced SIMD load/store single structure (lane / replicate), optionally post-indexed
        if insn & 0xbf9f_0000 == 0x0d00_0000 || insn & 0xbf80_0000 == 0x0d80_0000 {
            return self.simd_ldst_single(insn, mem);
        }

        // Load/store pair
        if insn & 0x3a00_0000 == 0x2800_0000 {
            let opc = bits(insn, 31, 30);
            let load_ = bit(insn, 22);
            let mode = bits(insn, 24, 23);
            let rt2 = bits(insn, 14, 10);
            let (bytes, signed) = if vector {
                if opc == 3 {
                    return unimpl;
                }
                (4u32 << opc, false)
            } else {
                match (opc, load_) {
                    (0, _) => (4, false),
                    (1, true) => (4, true), // LDPSW
                    (2, _) => (8, false),
                    _ => return unimpl, // STGP etc.
                }
            };
            let offset = sext(bits(insn, 21, 15) as u64, 7) * bytes as i64;
            let base = self.xsp(rn);
            let addr = match mode {
                0b01 => base, // post-index
                _ => base.wrapping_add(offset as u64),
            };
            if load_ {
                let a = load(mem, addr, bytes)?;
                let b = load(mem, addr.wrapping_add(bytes as u64), bytes)?;
                if vector {
                    self.q[rt as usize] = a;
                    self.q[rt2 as usize] = b;
                } else if signed {
                    self.set_xr(rt, a as u32 as i32 as i64 as u64);
                    self.set_xr(rt2, b as u32 as i32 as i64 as u64);
                } else {
                    self.set_xr(rt, a as u64);
                    self.set_xr(rt2, b as u64);
                }
            } else {
                let (a, b) = if vector {
                    (self.q[rt as usize], self.q[rt2 as usize])
                } else {
                    (self.xr(rt) as u128, self.xr(rt2) as u128)
                };
                store(mem, addr, bytes, a)?;
                store(mem, addr.wrapping_add(bytes as u64), bytes, b)?;
            }
            match mode {
                0b01 | 0b11 => self.set_xsp(rn, base.wrapping_add(offset as u64)),
                _ => {}
            }
            return Ok(());
        }

        // Load/store register (all single-register forms)
        if insn & 0x3a00_0000 == 0x3800_0000 {
            let size = bits(insn, 31, 30);
            let opc = bits(insn, 23, 22);

            // Atomic memory operations (LSE)
            if !bit(insn, 24) && bit(insn, 21) && bits(insn, 11, 10) == 0b00 {
                if vector {
                    return unimpl;
                }
                return self.atomic(insn, mem, 1 << size);
            }

            let (bytes, kind) = if vector {
                let bytes = if opc & 2 != 0 {
                    if size != 0 {
                        return unimpl;
                    }
                    16
                } else {
                    1 << size
                };
                (bytes, if opc & 1 == 1 { Kind::Load } else { Kind::Store })
            } else {
                let kind = match (size, opc) {
                    (_, 0) => Kind::Store,
                    (_, 1) => Kind::Load,
                    (3, 2) => Kind::Prefetch,
                    (2, 3) | (3, 3) => return unimpl,
                    (2, 2) => Kind::LoadSigned(true), // LDRSW
                    (_, 2) => Kind::LoadSigned(true),
                    (_, _) => Kind::LoadSigned(false),
                };
                (1u32 << size, kind)
            };

            let base = self.xsp(rn);
            let (addr, writeback) = if bit(insn, 24) {
                // Unsigned scaled 12-bit offset
                (base.wrapping_add(bits(insn, 21, 10) as u64 * bytes as u64), None)
            } else if bit(insn, 21) {
                // Register offset
                if bits(insn, 11, 10) != 0b10 {
                    return unimpl;
                }
                let option = bits(insn, 15, 13);
                if option & 2 == 0 {
                    return unimpl;
                }
                let shift = if bit(insn, 12) { bytes.trailing_zeros() } else { 0 };
                let m = self.xr(bits(insn, 20, 16));
                let off = match option {
                    2 => m as u32 as u64,
                    6 => m as u32 as i32 as i64 as u64,
                    _ => m,
                } << shift;
                (base.wrapping_add(off), None)
            } else {
                let imm = sext(bits(insn, 20, 12) as u64, 9) as u64;
                match bits(insn, 11, 10) {
                    0b00 | 0b10 => (base.wrapping_add(imm), None), // unscaled / unprivileged
                    0b01 => (base, Some(base.wrapping_add(imm))),  // post-index
                    _ => (base.wrapping_add(imm), Some(base.wrapping_add(imm))), // pre-index
                }
            };

            if vector {
                match kind {
                    Kind::Load => self.q[rt as usize] = load(mem, addr, bytes)?,
                    _ => {
                        let v = self.q[rt as usize];
                        let v = if bytes == 16 { v } else { v & ((1u128 << (bytes * 8)) - 1) };
                        store(mem, addr, bytes, v)?;
                    }
                }
            } else {
                match kind {
                    Kind::Store => store(mem, addr, bytes, (self.xr(rt) & mask(bytes)) as u128)?,
                    Kind::Load => {
                        let v = load(mem, addr, bytes)? as u64;
                        self.set_xr(rt, v);
                    }
                    Kind::LoadSigned(to64) => {
                        let v = sext(load(mem, addr, bytes)? as u64, bytes * 8) as u64;
                        self.set_xr(rt, if to64 { v } else { v as u32 as u64 });
                    }
                    Kind::Prefetch => {}
                }
            }
            if let Some(wb) = writeback {
                self.set_xsp(rn, wb);
            }
            return Ok(());
        }

        unimpl
    }

    fn ldst_exclusive(&mut self, insn: u32, mem: &mut Memory) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let size = bits(insn, 31, 30);
        let bytes = 1u32 << size;
        let o2 = bit(insn, 23);
        let l = bit(insn, 22);
        let o1 = bit(insn, 21);
        let rs = bits(insn, 20, 16);
        let rt2 = bits(insn, 14, 10);
        let rn = bits(insn, 9, 5);
        let rt = bits(insn, 4, 0);
        let addr = self.xsp(rn);

        match (o2, o1) {
            // LDXR/LDAXR, STXR/STLXR
            (false, false) => {
                if l {
                    self.set_xr(rt, load(mem, addr, bytes)? as u64);
                    self.exclusive = Some(addr);
                } else {
                    // One guest core and nothing else writes memory, so the
                    // monitor only fails if nobody did a matching load.
                    if self.exclusive == Some(addr) {
                        store(mem, addr, bytes, (self.xr(rt) & mask(bytes)) as u128)?;
                        self.set_xr(rs, 0);
                    } else {
                        self.set_xr(rs, 1);
                    }
                    self.exclusive = None;
                }
            }
            // CASP / CASPA / CASPL / CASPAL: compare-and-swap a register pair.
            // Bit 31 is 0 and bit 30 picks 32- or 64-bit elements (not the size field).
            (false, true) if insn >> 31 == 0 && rt2 == 0b11111 => {
                let eb = if bit(insn, 30) { 8 } else { 4 };
                let m = mask(eb);
                let next = |r: u32| if r == 31 { 31 } else { r + 1 };
                let old_lo = load(mem, addr, eb)? as u64;
                let old_hi = load(mem, addr.wrapping_add(eb as u64), eb)? as u64;
                if old_lo == self.xr(rs) & m && old_hi == self.xr(next(rs)) & m {
                    store(mem, addr, eb, (self.xr(rt) & m) as u128)?;
                    store(mem, addr.wrapping_add(eb as u64), eb, (self.xr(next(rt)) & m) as u128)?;
                }
                self.set_xr(rs, old_lo);
                self.set_xr(next(rs), old_hi);
            }
            // LDXP/LDAXP, STXP/STLXP (32- and 64-bit only)
            (false, true) if size >= 2 => {
                if l {
                    self.set_xr(rt, load(mem, addr, bytes)? as u64);
                    self.set_xr(rt2, load(mem, addr.wrapping_add(bytes as u64), bytes)? as u64);
                    self.exclusive = Some(addr);
                } else {
                    if self.exclusive == Some(addr) {
                        store(mem, addr, bytes, (self.xr(rt) & mask(bytes)) as u128)?;
                        store(mem, addr.wrapping_add(bytes as u64), bytes, (self.xr(rt2) & mask(bytes)) as u128)?;
                        self.set_xr(rs, 0);
                    } else {
                        self.set_xr(rs, 1);
                    }
                    self.exclusive = None;
                }
            }
            // LDAR/STLR and the LO variants: ordering is free on one core.
            (true, false) => {
                if l {
                    self.set_xr(rt, load(mem, addr, bytes)? as u64);
                } else {
                    store(mem, addr, bytes, (self.xr(rt) & mask(bytes)) as u128)?;
                }
            }
            // CAS / CASA / CASL / CASAL
            (true, true) if rt2 == 0b11111 => {
                let old = load(mem, addr, bytes)? as u64;
                if old == self.xr(rs) & mask(bytes) {
                    store(mem, addr, bytes, (self.xr(rt) & mask(bytes)) as u128)?;
                }
                self.set_xr(rs, old);
            }
            _ => return unimpl,
        }
        Ok(())
    }

    fn atomic(&mut self, insn: u32, mem: &mut Memory, bytes: u32) -> Step {
        let rs = bits(insn, 20, 16);
        let rn = bits(insn, 9, 5);
        let rt = bits(insn, 4, 0);
        let o3 = bit(insn, 15);
        let opc = bits(insn, 14, 12);
        let addr = self.xsp(rn);
        let m = mask(bytes);
        let old = load(mem, addr, bytes)? as u64;
        let s = self.xr(rs) & m;
        let w = bytes * 8;
        let signed = |v: u64| sext(v, w);

        let new = match (o3, opc) {
            (false, 0b000) => old.wrapping_add(s),
            (false, 0b001) => old & !s,
            (false, 0b010) => old ^ s,
            (false, 0b011) => old | s,
            (false, 0b100) => if signed(old) > signed(s) { old } else { s },
            (false, 0b101) => if signed(old) < signed(s) { old } else { s },
            (false, 0b110) => old.max(s),
            (false, 0b111) => old.min(s),
            (true, 0b000) => s, // SWP
            (true, 0b100) => {
                // LDAPR: plain load
                self.set_xr(rt, old);
                return Ok(());
            }
            _ => return Err(Exit::Unimplemented { insn }),
        };
        store(mem, addr, bytes, (new & m) as u128)?;
        self.set_xr(rt, old);
        Ok(())
    }
}
