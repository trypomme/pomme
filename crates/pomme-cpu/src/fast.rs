//! Instructions decoded once, when their block is fetched: the forms that
//! make up most of what apps run (register moves, loads and stores with an
//! immediate offset, pairs, immediate arithmetic and compares, branches),
//! with their fields pulled out ahead of time. Everything else stays
//! [`Op::Generic`] and runs through the full decoder.
//!
//! Each fast form does exactly what the full decoder does for that encoding,
//! using the same helpers; `tests/fast.rs` checks them against it on random
//! encodings and register states.

use crate::{Cpu, Step, bit, bits, sext};
use pomme_mem::Memory;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
    /// Any instruction, decoded when it runs.
    Generic(u32),
    /// `mov xd, xm` / `mov wd, wm` (ORR with the zero register, no shift).
    Mov { rd: u8, rm: u8, sf: bool },
    /// `movz`: a 16-bit immediate, shifted.
    Movz { rd: u8, imm: u64, sf: bool },
    /// ADD/SUB (immediate), setting flags when `s` (CMP, CMN): `b` is the
    /// immediate, already inverted for a subtraction (with carry in).
    AddImm { rd: u8, rn: u8, b: u64, carry: bool, sf: bool, s: bool },
    /// LDR (immediate, unsigned offset), zero-extending: x, w, h, b.
    Ldr { rt: u8, rn: u8, off: u64, bytes: u8 },
    /// STR (immediate, unsigned offset): x, w, h, b.
    Str { rt: u8, rn: u8, off: u64, bytes: u8 },
    /// LDP / STP of x or w registers: `mode` 0b01 post-index, 0b11 pre-index, else offset.
    Pair { rt: u8, rt2: u8, rn: u8, off: i64, bytes: u8, load: bool, mode: u8 },
    /// B / BL to a target (BL links the return address).
    B { target: u64, link: bool },
    /// B.cond.
    BCond { cond: u8, target: u64 },
    /// CBZ / CBNZ.
    Cbz { rt: u8, sf: bool, nonzero: bool, target: u64 },
    /// RET Xn.
    Ret { rn: u8 },
}

/// Decode `insn` at `pc` into a fast form if it has one.
pub(crate) fn decode(insn: u32, pc: u64) -> Op {
    let (rd, rn) = (bits(insn, 4, 0) as u8, bits(insn, 9, 5) as u8);
    let sf = bit(insn, 31);
    // ORR (shifted register) with rn = zr, no shift: MOV.
    if insn & 0x7fe0_ffe0 == 0x2a00_03e0 {
        return Op::Mov { rd, rm: bits(insn, 20, 16) as u8, sf };
    }
    // MOVZ.
    if insn & 0x7f80_0000 == 0x5280_0000 && (sf || !bit(insn, 22)) {
        return Op::Movz { rd, imm: (bits(insn, 20, 5) as u64) << (bits(insn, 22, 21) * 16), sf };
    }
    // ADD/SUB (immediate).
    if insn & 0x1f80_0000 == 0x1100_0000 {
        let mut imm = bits(insn, 21, 10) as u64;
        if bit(insn, 22) {
            imm <<= 12;
        }
        let sub = bit(insn, 30);
        return Op::AddImm { rd, rn, b: if sub { !imm } else { imm }, carry: sub, sf, s: bit(insn, 29) };
    }
    // LDR/STR (immediate, unsigned offset), integer registers, plain load or store.
    if insn & 0x3f00_0000 == 0x3900_0000 {
        let size = bits(insn, 31, 30);
        let bytes = 1u8 << size;
        let off = bits(insn, 21, 10) as u64 * bytes as u64;
        let rt = rd;
        match bits(insn, 23, 22) {
            0 => return Op::Str { rt, rn, off, bytes },
            1 => return Op::Ldr { rt, rn, off, bytes },
            _ => {}
        }
    }
    // LDP/STP of integer registers (not LDPSW, not SIMD): opc 00 (w) or 10 (x).
    if insn & 0x3a00_0000 == 0x2800_0000 && !bit(insn, 26) && matches!(bits(insn, 31, 30), 0 | 2) {
        let bytes = if bits(insn, 31, 30) == 2 { 8u8 } else { 4 };
        return Op::Pair {
            rt: rd,
            rt2: bits(insn, 14, 10) as u8,
            rn,
            off: sext(bits(insn, 21, 15) as u64, 7) * bytes as i64,
            bytes,
            load: bit(insn, 22),
            mode: bits(insn, 24, 23) as u8,
        };
    }
    // B / BL.
    if insn & 0x7c00_0000 == 0x1400_0000 {
        return Op::B { target: pc.wrapping_add((sext(bits(insn, 25, 0) as u64, 26) * 4) as u64), link: bit(insn, 31) };
    }
    // B.cond.
    if insn & 0xff00_0010 == 0x5400_0000 {
        return Op::BCond { cond: bits(insn, 3, 0) as u8, target: pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64) };
    }
    // CBZ / CBNZ.
    if insn & 0x7e00_0000 == 0x3400_0000 {
        return Op::Cbz { rt: rd, sf, nonzero: bit(insn, 24), target: pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64) };
    }
    // RET.
    if insn & 0xffff_fc1f == 0xd65f_0000 {
        return Op::Ret { rn };
    }
    Op::Generic(insn)
}

impl Cpu {
    /// Run one decoded instruction (at pc): like [`Cpu::exec`] on its encoding.
    #[inline(always)]
    pub(crate) fn exec_op(&mut self, op: &Op, mem: &mut Memory) -> Step {
        let next = match *op {
            Op::Generic(insn) => return self.exec(insn, mem),
            Op::Mov { rd, rm, sf } => {
                let v = self.rd(rm as u32, sf);
                self.wr(rd as u32, sf, v);
                self.pc.wrapping_add(4)
            }
            Op::Movz { rd, imm, sf } => {
                self.wr(rd as u32, sf, imm);
                self.pc.wrapping_add(4)
            }
            Op::AddImm { rd, rn, b, carry, sf, s } => {
                let a = self.xsp(rn as u32);
                let a = if sf { a } else { a as u32 as u64 };
                let r = self.add_with_carry(a, b, carry, sf, s);
                if s { self.wr(rd as u32, sf, r) } else { self.wr_sp(rd as u32, sf, r) }
                self.pc.wrapping_add(4)
            }
            Op::Ldr { rt, rn, off, bytes } => {
                let addr = self.xsp(rn as u32).wrapping_add(off);
                let v = match bytes {
                    1 => mem.read_u8(addr)? as u64,
                    2 => mem.read_u16(addr)? as u64,
                    4 => mem.read_u32(addr)? as u64,
                    _ => mem.read_u64(addr)?,
                };
                self.set_xr(rt as u32, v);
                self.pc.wrapping_add(4)
            }
            Op::Str { rt, rn, off, bytes } => {
                let addr = self.xsp(rn as u32).wrapping_add(off);
                let v = self.xr(rt as u32);
                match bytes {
                    1 => mem.write_u8(addr, v as u8)?,
                    2 => mem.write_u16(addr, v as u16)?,
                    4 => mem.write_u32(addr, v as u32)?,
                    _ => mem.write_u64(addr, v)?,
                }
                self.pc.wrapping_add(4)
            }
            Op::Pair { rt, rt2, rn, off, bytes, load, mode } => {
                let base = self.xsp(rn as u32);
                let addr = if mode == 0b01 { base } else { base.wrapping_add(off as u64) };
                let second = addr.wrapping_add(bytes as u64);
                if load {
                    let (a, b) = if bytes == 8 {
                        (mem.read_u64(addr)?, mem.read_u64(second)?)
                    } else {
                        (mem.read_u32(addr)? as u64, mem.read_u32(second)? as u64)
                    };
                    self.set_xr(rt as u32, a);
                    self.set_xr(rt2 as u32, b);
                } else {
                    let (a, b) = (self.xr(rt as u32), self.xr(rt2 as u32));
                    if bytes == 8 {
                        mem.write_u64(addr, a)?;
                        mem.write_u64(second, b)?;
                    } else {
                        mem.write_u32(addr, a as u32)?;
                        mem.write_u32(second, b as u32)?;
                    }
                }
                if mode & 1 == 1 {
                    self.set_xsp(rn as u32, base.wrapping_add(off as u64));
                }
                self.pc.wrapping_add(4)
            }
            Op::B { target, link } => {
                if link {
                    self.x[30] = self.pc + 4;
                }
                target
            }
            Op::BCond { cond, target } => {
                if self.cond(cond as u32) { target } else { self.pc.wrapping_add(4) }
            }
            Op::Cbz { rt, sf, nonzero, target } => {
                if (self.rd(rt as u32, sf) == 0) != nonzero { target } else { self.pc.wrapping_add(4) }
            }
            Op::Ret { rn } => self.xr(rn as u32),
        };
        self.pc = next;
        self.steps += 1;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use pomme_mem::Prot;

    const BASE: u64 = 0x1000_0000;
    const SIZE: u64 = 0x10000;
    const PC: u64 = 0x2000_0000;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    fn memory(seed: u64) -> Memory {
        let mut mem = Memory::default();
        mem.map(BASE, SIZE, Prot::RW, "data").unwrap();
        let mut r = Rng(seed);
        for a in (BASE..BASE + SIZE).step_by(8) {
            mem.write_u64(a, r.next()).unwrap();
        }
        mem
    }

    fn cpu(r: &mut Rng) -> Cpu {
        let mut c = Cpu::new();
        for x in c.x.iter_mut() {
            // Mostly addresses in the data region (so loads and stores land), sometimes anything.
            *x = if r.next() % 4 == 0 { r.next() } else { BASE + (r.next() % (SIZE / 2)) & !7 };
        }
        c.sp = BASE + (r.next() % (SIZE / 2)) & !15;
        c.set_nzcv(r.next() & 0xf000_0000);
        c.pc = PC;
        c
    }

    fn state(c: &Cpu) -> (Vec<u64>, u64, u64, u64, u64) {
        (c.x.to_vec(), c.sp, c.pc, c.nzcv(), c.steps)
    }

    /// Every encoding with a fast form does what the full decoder does, from random states.
    #[test]
    fn fast_forms_match_the_decoder() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        let mut checked = std::collections::HashMap::<&str, u32>::new();
        // Encodings random bits rarely hit: (fixed bits, bits left random).
        const TEMPLATES: &[(u32, u32)] = &[
            (0x2a00_03e0, 0x801f_001f), // mov
            (0xd65f_0000, 0x0000_03e0), // ret
            (0x1100_0000, 0xe07f_ffff), // add/sub imm
            (0x3900_0000, 0xc0ff_ffff), // ldr/str imm
            (0x2800_0000, 0x81ff_ffff), // ldp/stp
        ];
        for i in 0..400_000u32 {
            let insn = match i % 3 {
                0 => r.next() as u32,
                _ => {
                    let (fixed, free) = TEMPLATES[r.next() as usize % TEMPLATES.len()];
                    fixed | (r.next() as u32 & free)
                }
            };
            let op = decode(insn, PC);
            let kind = match op {
                Op::Generic(_) => continue,
                Op::Mov { .. } => "mov",
                Op::Movz { .. } => "movz",
                Op::AddImm { .. } => "add/sub imm",
                Op::Ldr { .. } => "ldr",
                Op::Str { .. } => "str",
                Op::Pair { .. } => "ldp/stp",
                Op::B { .. } => "b/bl",
                Op::BCond { .. } => "b.cond",
                Op::Cbz { .. } => "cbz",
                Op::Ret { .. } => "ret",
            };
            *checked.entry(kind).or_default() += 1;
            let seed = r.next();
            let (mut a, mut b) = (cpu(&mut Rng(seed | 1)), cpu(&mut Rng(seed | 1)));
            let (mut ma, mut mb) = (memory(seed), memory(seed));
            let ra = a.exec(insn, &mut ma);
            let rb = b.exec_op(&op, &mut mb);
            assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{insn:#010x} {op:?}");
            assert_eq!(state(&a), state(&b), "{insn:#010x} {op:?}");
            assert_eq!(ma.read(BASE, SIZE as usize).unwrap(), mb.read(BASE, SIZE as usize).unwrap(), "{insn:#010x} {op:?}");
        }
        // Every form was exercised.
        for kind in ["mov", "movz", "add/sub imm", "ldr", "str", "ldp/stp", "b/bl", "b.cond", "cbz", "ret"] {
            assert!(checked.get(kind).copied().unwrap_or(0) > 0, "{kind} never generated: {checked:?}");
        }
    }
}
