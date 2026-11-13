//! Integer data processing: immediate and register forms.

use crate::{Cpu, Exit, Step, bit, bits, decode_bit_masks, ones, ror, sext};

impl Cpu {
    pub(crate) fn dp_imm(&mut self, insn: u32) -> Step {
        let sf = bit(insn, 31);
        let width = if sf { 64 } else { 32 };
        let rd = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let unimpl = Err(Exit::Unimplemented { insn });

        match bits(insn, 25, 23) {
            // ADR / ADRP
            0b000 | 0b001 => {
                let imm = (bits(insn, 23, 5) << 2 | bits(insn, 30, 29)) as u64;
                let imm = sext(imm, 21);
                let v = if bit(insn, 31) {
                    (self.pc & !0xfff).wrapping_add((imm << 12) as u64)
                } else {
                    self.pc.wrapping_add(imm as u64)
                };
                self.set_xr(rd, v);
            }
            // ADD/SUB (immediate)
            0b010 => {
                let sub = bit(insn, 30);
                let s = bit(insn, 29);
                let mut imm = bits(insn, 21, 10) as u64;
                if bit(insn, 22) {
                    imm <<= 12;
                }
                let a = self.xsp(rn);
                let a = if sf { a } else { a as u32 as u64 };
                let r = if sub {
                    self.add_with_carry(a, !imm, true, sf, s)
                } else {
                    self.add_with_carry(a, imm, false, sf, s)
                };
                if s { self.wr(rd, sf, r) } else { self.wr_sp(rd, sf, r) }
            }
            // Logical (immediate)
            0b100 => {
                let n = bits(insn, 22, 22);
                if !sf && n == 1 {
                    return unimpl;
                }
                let Some((imm, _)) = decode_bit_masks(n, bits(insn, 15, 10), bits(insn, 21, 16), true, width) else {
                    return unimpl;
                };
                let a = self.rd(rn, sf);
                match bits(insn, 30, 29) {
                    0b00 => self.wr_sp(rd, sf, a & imm),
                    0b01 => self.wr_sp(rd, sf, a | imm),
                    0b10 => self.wr_sp(rd, sf, a ^ imm),
                    _ => {
                        let r = a & imm;
                        self.set_nz(r, sf);
                        self.c = false;
                        self.v = false;
                        self.wr(rd, sf, r);
                    }
                }
            }
            // Move wide
            0b101 => {
                let hw = bits(insn, 22, 21);
                if !sf && hw >= 2 {
                    return unimpl;
                }
                let shift = hw * 16;
                let imm = (bits(insn, 20, 5) as u64) << shift;
                match bits(insn, 30, 29) {
                    0b00 => self.wr(rd, sf, !imm),
                    0b10 => self.wr(rd, sf, imm),
                    0b11 => {
                        let old = self.xr(rd);
                        self.wr(rd, sf, (old & !(0xffffu64 << shift)) | imm);
                    }
                    _ => return unimpl,
                }
            }
            // Bitfield
            0b110 => {
                let n = bits(insn, 22, 22);
                if (sf as u32) != n {
                    return unimpl;
                }
                let immr = bits(insn, 21, 16);
                let imms = bits(insn, 15, 10);
                let Some((wmask, tmask)) = decode_bit_masks(n, imms, immr, false, width) else {
                    return unimpl;
                };
                let src = self.rd(rn, sf);
                let (inzero, extend) = match bits(insn, 30, 29) {
                    0b00 => (true, true),   // SBFM
                    0b01 => (false, false), // BFM
                    0b10 => (true, false),  // UBFM
                    _ => return unimpl,
                };
                let dst = if inzero { 0 } else { self.rd(rd, sf) };
                let bot = (dst & !wmask) | (ror(src, immr, width) & wmask);
                let top = if extend {
                    if (src >> imms) & 1 == 1 { ones(width) } else { 0 }
                } else {
                    dst
                };
                self.wr(rd, sf, (top & !tmask) | (bot & tmask));
            }
            // EXTR
            0b111 => {
                let rm = bits(insn, 20, 16);
                let lsb = bits(insn, 15, 10);
                if lsb >= width || bit(insn, 22) != sf {
                    return unimpl;
                }
                let hi = self.rd(rn, sf) as u128;
                let lo = self.rd(rm, sf) as u128;
                let concat = (hi << width) | lo;
                self.wr(rd, sf, (concat >> lsb) as u64 & ones(width));
            }
            _ => return unimpl,
        }
        Ok(())
    }

    fn shift_reg(&self, v: u64, kind: u32, amount: u32, sf: bool) -> u64 {
        let width = if sf { 64 } else { 32 };
        let v = v & ones(width);
        let r = match kind {
            0 => v << amount,
            1 => v >> amount,
            2 => {
                if sf {
                    ((v as i64) >> amount) as u64
                } else {
                    ((v as u32 as i32) >> amount) as u32 as u64
                }
            }
            _ => ror(v, amount, width),
        };
        r & ones(width)
    }

    fn extend_reg(&self, rm: u32, option: u32, shift: u32) -> u64 {
        let v = self.xr(rm);
        let e = match option {
            0 => v as u8 as u64,
            1 => v as u16 as u64,
            2 => v as u32 as u64,
            3 => v,
            4 => v as u8 as i8 as i64 as u64,
            5 => v as u16 as i16 as i64 as u64,
            6 => v as u32 as i32 as i64 as u64,
            _ => v,
        };
        e << shift
    }

    pub(crate) fn dp_reg(&mut self, insn: u32) -> Step {
        let sf = bit(insn, 31);
        let width = if sf { 64 } else { 32 };
        let rd = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let rm = bits(insn, 20, 16);
        let unimpl = Err(Exit::Unimplemented { insn });

        // Logical (shifted register)
        if insn & 0x1f00_0000 == 0x0a00_0000 {
            let amount = bits(insn, 15, 10);
            if !sf && amount >= 32 {
                return unimpl;
            }
            let mut b = self.shift_reg(self.xr(rm), bits(insn, 23, 22), amount, sf);
            if bit(insn, 21) {
                b = !b & ones(width);
            }
            let a = self.rd(rn, sf);
            let r = match bits(insn, 30, 29) {
                0b00 | 0b11 => a & b,
                0b01 => a | b,
                _ => a ^ b,
            };
            if bits(insn, 30, 29) == 0b11 {
                self.set_nz(r, sf);
                self.c = false;
                self.v = false;
            }
            self.wr(rd, sf, r);
            return Ok(());
        }

        // Add/sub (shifted register)
        if insn & 0x1f20_0000 == 0x0b00_0000 {
            let kind = bits(insn, 23, 22);
            let amount = bits(insn, 15, 10);
            if kind == 3 || (!sf && amount >= 32) {
                return unimpl;
            }
            let a = self.rd(rn, sf);
            let b = self.shift_reg(self.xr(rm), kind, amount, sf);
            let s = bit(insn, 29);
            let r = if bit(insn, 30) {
                self.add_with_carry(a, !b, true, sf, s)
            } else {
                self.add_with_carry(a, b, false, sf, s)
            };
            self.wr(rd, sf, r);
            return Ok(());
        }

        // Add/sub (extended register)
        if insn & 0x1f20_0000 == 0x0b20_0000 {
            let shift = bits(insn, 12, 10);
            if bits(insn, 23, 22) != 0 || shift > 4 {
                return unimpl;
            }
            let a = self.xsp(rn);
            let a = if sf { a } else { a as u32 as u64 };
            let b = self.extend_reg(rm, bits(insn, 15, 13), shift) & ones(width);
            let s = bit(insn, 29);
            let r = if bit(insn, 30) {
                self.add_with_carry(a, !b, true, sf, s)
            } else {
                self.add_with_carry(a, b, false, sf, s)
            };
            if s { self.wr(rd, sf, r) } else { self.wr_sp(rd, sf, r) }
            return Ok(());
        }

        match insn & 0x1fe0_0000 {
            // ADC / ADCS / SBC / SBCS
            0x1a00_0000 if bits(insn, 15, 10) == 0 => {
                let a = self.rd(rn, sf);
                let mut b = self.rd(rm, sf);
                if bit(insn, 30) {
                    b = !b & ones(width);
                }
                let carry = self.c;
                let r = self.add_with_carry(a, b, carry, sf, bit(insn, 29));
                self.wr(rd, sf, r);
                return Ok(());
            }
            // CCMN / CCMP (register or immediate)
            0x1a40_0000 if bit(insn, 29) && !bit(insn, 10) && !bit(insn, 4) => {
                if self.cond(bits(insn, 15, 12)) {
                    let a = self.rd(rn, sf);
                    let b = if bit(insn, 11) { bits(insn, 20, 16) as u64 } else { self.rd(rm, sf) };
                    if bit(insn, 30) {
                        self.add_with_carry(a, !b & ones(width), true, sf, true);
                    } else {
                        self.add_with_carry(a, b, false, sf, true);
                    }
                } else {
                    self.set_nzcv4(bits(insn, 3, 0));
                }
                return Ok(());
            }
            // CSEL / CSINC / CSINV / CSNEG
            0x1a80_0000 if !bit(insn, 29) && !bit(insn, 11) => {
                let r = if self.cond(bits(insn, 15, 12)) {
                    self.rd(rn, sf)
                } else {
                    let b = self.rd(rm, sf);
                    match (bit(insn, 30), bit(insn, 10)) {
                        (false, false) => b,
                        (false, true) => b.wrapping_add(1),
                        (true, false) => !b,
                        (true, true) => b.wrapping_neg(),
                    }
                };
                self.wr(rd, sf, r);
                return Ok(());
            }
            _ => {}
        }

        // Data-processing (2 source)
        if insn & 0x5fe0_0000 == 0x1ac0_0000 && !bit(insn, 29) {
            let a = self.rd(rn, sf);
            let b = self.rd(rm, sf);
            let r = match bits(insn, 15, 10) {
                // UDIV/SDIV: divide by zero yields 0 on arm64.
                0b000010 => a.checked_div(b).unwrap_or(0),
                0b000011 => {
                    if b == 0 {
                        0
                    } else if sf {
                        (a as i64).wrapping_div(b as i64) as u64
                    } else {
                        (a as u32 as i32).wrapping_div(b as u32 as i32) as u32 as u64
                    }
                }
                op @ 0b001000..=0b001011 => self.shift_reg(a, op & 3, (b % width as u64) as u32, sf),
                // CRC32B/H/W/X and CRC32CB/CH/CW/CX: Wd = CRC of Wn updated with the
                // low 1/2/4/8 bytes of Rm (X only with sf); C picks Castagnoli's polynomial.
                op @ 0b010000..=0b010111 => {
                    let size = op & 3;
                    if (size == 3) != sf {
                        return unimpl;
                    }
                    let castagnoli = op & 4 != 0;
                    let r = crc32(a as u32, b, 1 << size, castagnoli) as u64;
                    self.wr(rd, false, r);
                    return Ok(());
                }
                _ => return unimpl,
            };
            self.wr(rd, sf, r);
            return Ok(());
        }

        // Data-processing (1 source)
        if insn & 0x5fe0_0000 == 0x5ac0_0000 && bits(insn, 20, 16) == 0 && !bit(insn, 29) {
            let a = self.rd(rn, sf);
            let r = match bits(insn, 15, 10) {
                0b000000 => {
                    if sf { a.reverse_bits() } else { (a as u32).reverse_bits() as u64 }
                }
                0b000001 => {
                    // REV16: swap bytes in each halfword
                    ((a & 0x00ff_00ff_00ff_00ff) << 8) | ((a >> 8) & 0x00ff_00ff_00ff_00ff)
                }
                0b000010 => {
                    if sf {
                        // REV32: swap bytes in each word
                        let lo = (a as u32).swap_bytes() as u64;
                        let hi = ((a >> 32) as u32).swap_bytes() as u64;
                        (hi << 32) | lo
                    } else {
                        (a as u32).swap_bytes() as u64
                    }
                }
                0b000011 if sf => a.swap_bytes(),
                0b000100 => {
                    if sf { a.leading_zeros() as u64 } else { (a as u32).leading_zeros() as u64 }
                }
                // CLS: leading bits equal to the sign bit, not counting the sign bit.
                0b000101 => {
                    if sf {
                        let s = a as i64;
                        ((s ^ (s >> 63)) as u64).leading_zeros() as u64 - 1
                    } else {
                        let s = a as u32 as i32;
                        ((s ^ (s >> 31)) as u32).leading_zeros() as u64 - 1
                    }
                }
                _ => return unimpl,
            };
            self.wr(rd, sf, r);
            return Ok(());
        }

        // Data-processing (3 source)
        if insn & 0x1f00_0000 == 0x1b00_0000 {
            let ra = bits(insn, 14, 10);
            let o0 = bit(insn, 15);
            let a = self.xr(ra);
            let n = self.xr(rn);
            let m = self.xr(rm);
            let r = match (bits(insn, 30, 29), bits(insn, 23, 21)) {
                (0, 0b000) => {
                    let p = n.wrapping_mul(m);
                    if o0 { a.wrapping_sub(p) } else { a.wrapping_add(p) }
                }
                (0, 0b001) if sf => {
                    let p = (n as u32 as i32 as i64).wrapping_mul(m as u32 as i32 as i64) as u64;
                    if o0 { a.wrapping_sub(p) } else { a.wrapping_add(p) }
                }
                (0, 0b101) if sf => {
                    let p = (n as u32 as u64).wrapping_mul(m as u32 as u64);
                    if o0 { a.wrapping_sub(p) } else { a.wrapping_add(p) }
                }
                (0, 0b010) if sf && !o0 => ((n as i64 as i128 * m as i64 as i128) >> 64) as u64,
                (0, 0b110) if sf && !o0 => ((n as u128 * m as u128) >> 64) as u64,
                _ => return unimpl,
            };
            self.wr(rd, sf, r);
            return Ok(());
        }

        unimpl
    }
}

/// The ARMv8 CRC32 instructions' step: `crc` (no pre- or post-inversion)
/// updated with the low `bytes` bytes of `data`, least significant first, in
/// reflected form: CRC-32 (0x04C11DB7) or CRC-32C (0x1EDC6F41).
pub(crate) fn crc32(mut crc: u32, data: u64, bytes: u32, castagnoli: bool) -> u32 {
    let poly = if castagnoli { 0x82F6_3B78 } else { 0xEDB8_8320 };
    for i in 0..bytes {
        crc ^= (data >> (8 * i)) as u8 as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ poly } else { crc >> 1 };
        }
    }
    crc
}

#[cfg(test)]
mod crc_tests {
    use super::crc32;

    /// zlib's crc32("123456789") is 0xCBF43926 and CRC-32C's is 0xE3069283,
    /// with the inversions the instructions leave to software.
    #[test]
    fn check_values() {
        for (castagnoli, expect) in [(false, 0xCBF4_3926u32), (true, 0xE306_9283)] {
            let crc = b"123456789".iter().fold(!0u32, |c, &b| crc32(c, b as u64, 1, castagnoli));
            assert_eq!(!crc, expect);
            // Eight bytes at once agree with one at a time.
            let word = u64::from_le_bytes(*b"12345678");
            let one = b"12345678".iter().fold(!0u32, |c, &b| crc32(c, b as u64, 1, castagnoli));
            assert_eq!(crc32(!0, word, 8, castagnoli), one);
        }
    }
}
