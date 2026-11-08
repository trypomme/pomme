//! Advanced SIMD (NEON), as a lane engine.
//!
//! Covers the groups compilers emit for auto-vectorised integer and FP code:
//! three-same, three-different (widening), two-register misc, across-lanes,
//! shift-by-immediate, by-element, permutes, EXT, TBL/TBX, copies and modified
//! immediates, plus their common scalar forms. Saturating ops are partial.

use crate::{Cpu, Exit, Step, bit, bits, ones, sext};

#[inline]
fn el(v: u128, esize: u32, i: u32) -> u64 {
    ((v >> (i * esize)) as u64) & ones(esize)
}

#[inline]
fn sel(v: u128, esize: u32, i: u32) -> i64 {
    sext(el(v, esize, i), esize)
}

#[inline]
fn put(r: u128, esize: u32, i: u32, x: u64) -> u128 {
    let m = (ones(esize) as u128) << (i * esize);
    (r & !m) | (((x & ones(esize)) as u128) << (i * esize))
}

fn rep(v: u64, esize: u32) -> u128 {
    (0..128 / esize).fold(0u128, |r, i| put(r, esize, i, v))
}

/// Upper 64 bits for "2" (second-half) variants, else the lower 64.
fn half(v: u128, upper: bool) -> u128 {
    if upper { v >> 64 } else { v & u64::MAX as u128 }
}

fn mask_q(v: u128, q: bool) -> u128 {
    if q { v } else { v & u64::MAX as u128 }
}

fn all(b: bool) -> u64 {
    if b { u64::MAX } else { 0 }
}

fn sat_s(v: i128, esize: u32) -> u64 {
    let max = (1i128 << (esize - 1)) - 1;
    let min = -(1i128 << (esize - 1));
    v.clamp(min, max) as u64
}

fn sat_u(v: i128, esize: u32) -> u64 {
    v.clamp(0, ones(esize) as i128) as u64
}

/// Apply an f32/f64 expression lane-wise. `$e` sees `a`, `b`, `d` as floats.
macro_rules! fp_lanes {
    ($esize:expr, $count:expr, $va:expr, $vb:expr, $vd:expr, |$a:ident, $b:ident, $d:ident| $e:expr) => {{
        let mut r = 0u128;
        for i in 0..$count {
            let x = if $esize == 32 {
                let $a = f32::from_bits(el($va, 32, i) as u32);
                let $b = f32::from_bits(el($vb, 32, i) as u32);
                let $d = f32::from_bits(el($vd, 32, i) as u32);
                let _ = (&$b, &$d);
                ($e).to_bits() as u64
            } else {
                let $a = f64::from_bits(el($va, 64, i));
                let $b = f64::from_bits(el($vb, 64, i));
                let $d = f64::from_bits(el($vd, 64, i));
                let _ = (&$b, &$d);
                ($e).to_bits()
            };
            r = put(r, $esize, i, x);
        }
        r
    }};
}

/// Lane-wise FP compare producing all-ones/zero masks.
macro_rules! fp_cmp_lanes {
    ($esize:expr, $count:expr, $va:expr, $vb:expr, |$a:ident, $b:ident| $e:expr) => {{
        let mut r = 0u128;
        for i in 0..$count {
            let t = if $esize == 32 {
                let $a = f32::from_bits(el($va, 32, i) as u32);
                let $b = f32::from_bits(el($vb, 32, i) as u32);
                $e
            } else {
                let $a = f64::from_bits(el($va, 64, i));
                let $b = f64::from_bits(el($vb, 64, i));
                $e
            };
            r = put(r, $esize, i, all(t));
        }
        r
    }};
}

impl Cpu {
    pub(crate) fn neon(&mut self, insn: u32) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let q = bit(insn, 30);
        let u = bit(insn, 29);
        let size = bits(insn, 23, 22);
        let rd = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let rm = bits(insn, 20, 16);
        let (vn, vm, vd) = (self.q[rn as usize], self.q[rm as usize], self.q[rd as usize]);
        let datasize = if q { 128 } else { 64 };
        let scalar = bit(insn, 28);

        // ---- vector forms (bit 28 = 0) ----
        if !scalar {
            // Permute: UZP/TRN/ZIP
            if insn & 0xbf20_8c00 == 0x0e00_0800 {
                let esize = 8 << size;
                let n = datasize / esize;
                let opc = bits(insn, 14, 12);
                let mut r = 0u128;
                for i in 0..n {
                    let x = match opc {
                        0b001 | 0b101 => {
                            let idx = 2 * i + (opc >> 2);
                            if idx < n { el(vn, esize, idx) } else { el(vm, esize, idx - n) }
                        }
                        0b010 | 0b110 => {
                            let p = i / 2;
                            let src = if i % 2 == 0 { vn } else { vm };
                            el(src, esize, 2 * p + (opc >> 2))
                        }
                        0b011 | 0b111 => {
                            let base = if opc == 0b111 { n / 2 } else { 0 };
                            let src = if i % 2 == 0 { vn } else { vm };
                            el(src, esize, base + i / 2)
                        }
                        _ => return unimpl,
                    };
                    r = put(r, esize, i, x);
                }
                self.q[rd as usize] = r;
                return Ok(());
            }
            // EXT
            if insn & 0xbfe0_8400 == 0x2e00_0000 {
                let pos = bits(insn, 14, 11) * 8;
                let (lo, hi) = (mask_q(vn, q), mask_q(vm, q));
                let concat = if q {
                    if pos == 0 { lo } else { (lo >> pos) | (hi << (128 - pos)) }
                } else {
                    ((hi << 64 | lo) >> pos) & u64::MAX as u128
                };
                self.q[rd as usize] = mask_q(concat, q);
                return Ok(());
            }
            // TBL / TBX
            if insn & 0xbfe0_9c00 == 0x0e00_0000 || insn & 0xbfe0_9c00 == 0x0e00_1000 {
                let len = bits(insn, 14, 13) + 1;
                let tbx = bit(insn, 12);
                let mut table = Vec::with_capacity(16 * len as usize);
                for t in 0..len {
                    table.extend_from_slice(&self.q[((rn + t) % 32) as usize].to_le_bytes());
                }
                let mut r = 0u128;
                for i in 0..datasize / 8 {
                    let idx = el(vm, 8, i) as usize;
                    let x = match table.get(idx) {
                        Some(&b) => b as u64,
                        None if tbx => el(vd, 8, i),
                        None => 0,
                    };
                    r = put(r, 8, i, x);
                }
                self.q[rd as usize] = r;
                return Ok(());
            }
            // Shift by immediate (immh != 0); immh == 0 is modified-immediate, handled in fp.rs
            if insn & 0x9f80_0400 == 0x0f00_0400 && bits(insn, 22, 19) != 0 {
                return self.neon_shift_imm(insn, q, u, rd, rn, vn, vd);
            }
            // Vector x indexed element
            if insn & 0x9f00_0400 == 0x0f00_0000 {
                return self.neon_by_element(insn, q, u, size, rd, rn, vn, vd);
            }
            // Three different (widening / narrowing)
            if insn & 0x9f20_0c00 == 0x0e20_0000 {
                return self.neon_three_different(insn, q, u, size, rd, vn, vm, vd);
            }
            // Two-register misc
            if insn & 0x9f3e_0c00 == 0x0e20_0800 {
                return self.neon_two_misc(insn, q, u, size, rd, vn, vd, false);
            }
            // Across lanes
            if insn & 0x9f3e_0c00 == 0x0e30_0800 {
                let esize = 8 << size;
                let n = datasize / esize;
                let opcode = bits(insn, 16, 12);
                let vals_u = (0..n).map(|i| el(vn, esize, i));
                let vals_s = (0..n).map(|i| sel(vn, esize, i));
                let r: u64 = match (opcode, u) {
                    (0b11011, false) => vals_u.fold(0u64, |a, b| a.wrapping_add(b)) & ones(esize), // ADDV
                    (0b00011, false) => vals_s.sum::<i64>() as u64 & ones(esize * 2),           // SADDLV
                    (0b00011, true) => vals_u.sum::<u64>() & ones(esize * 2),                  // UADDLV
                    (0b01010, false) => vals_s.max().unwrap() as u64 & ones(esize),
                    (0b01010, true) => vals_u.max().unwrap(),
                    (0b11010, false) => vals_s.min().unwrap() as u64 & ones(esize),
                    (0b11010, true) => vals_u.min().unwrap(),
                    // FMAXNMV / FMINNMV / FMAXV / FMINV (single precision, Q=1)
                    (0b01100 | 0b01111, true) if size & 1 == 0 => {
                        let f = (0..4).map(|i| f32::from_bits(el(vn, 32, i) as u32));
                        let min = size & 2 != 0;
                        let r = f.reduce(|a, b| if min { a.min(b) } else { a.max(b) }).unwrap();
                        r.to_bits() as u64
                    }
                    _ => return unimpl,
                };
                self.q[rd as usize] = r as u128;
                return Ok(());
            }
            // Three same
            if insn & 0x9f20_0400 == 0x0e20_0400 {
                return self.neon_three_same(insn, q, u, size, rd, vn, vm, vd, false);
            }
            return unimpl;
        }

        // ---- scalar forms (bit 28 = 1, bit 30 = 1) ----
        // DUP (element), scalar: MOV Sd, Vn.S[i]
        if insn & 0xffe0_fc00 == 0x5e00_0400 {
            let imm5 = bits(insn, 20, 16);
            if imm5 & 0xf == 0 {
                return unimpl;
            }
            let s = imm5.trailing_zeros();
            let esize = 8 << s;
            self.q[rd as usize] = el(vn, esize, imm5 >> (s + 1)) as u128;
            return Ok(());
        }
        // Scalar pairwise
        if insn & 0xdf3e_0c00 == 0x5e30_0800 {
            let opcode = bits(insn, 16, 12);
            let r = match (opcode, u) {
                (0b11011, false) if size == 3 => el(vn, 64, 0).wrapping_add(el(vn, 64, 1)) as u128,
                (0b01101, true) => {
                    if size & 1 == 0 {
                        (f32::from_bits(el(vn, 32, 0) as u32) + f32::from_bits(el(vn, 32, 1) as u32)).to_bits() as u128
                    } else {
                        (f64::from_bits(el(vn, 64, 0)) + f64::from_bits(el(vn, 64, 1))).to_bits() as u128
                    }
                }
                _ => return unimpl,
            };
            self.q[rd as usize] = r;
            return Ok(());
        }
        // Scalar shift by immediate
        if insn & 0xdf80_0400 == 0x5f00_0400 && bits(insn, 22, 19) != 0 {
            let immh = bits(insn, 22, 19);
            if immh & 0b1000 == 0 {
                // Only 64-bit scalar shifts (and fixed-point converts on 32) are common.
                if bits(insn, 15, 11) == 0b11100 || bits(insn, 15, 11) == 0b11111 {
                    return self.neon_shift_imm(insn, true, u, rd, rn, vn & ones(32) as u128, 0).map(|_| {
                        self.q[rd as usize] &= ones(32) as u128;
                    });
                }
                return unimpl;
            }
            let r = self.neon_shift_imm(insn, true, u, rd, rn, vn & u64::MAX as u128, vd & u64::MAX as u128);
            self.q[rd as usize] &= u64::MAX as u128;
            return r;
        }
        // Scalar x indexed element: FMLA / FMLS / FMUL / FMULX on one S or D element
        if insn & 0xdf00_0400 == 0x5f00_0000 {
            let opcode = bits(insn, 15, 12);
            let fp = matches!((opcode, u), (0b0001, false) | (0b0101, false) | (0b1001, _));
            if !fp || size >> 1 != 1 {
                return unimpl;
            }
            let esize = if size & 1 == 1 { 64 } else { 32 };
            self.neon_by_element(insn, false, u, size, rd, rn, vn, vd)?;
            self.q[rd as usize] &= ones(esize) as u128;
            return Ok(());
        }
        // Scalar two-register misc
        if insn & 0xdf3e_0c00 == 0x5e20_0800 {
            return self.neon_two_misc(insn, false, u, size, rd, vn, vd, true);
        }
        // Scalar three same
        if insn & 0xdf20_0400 == 0x5e20_0400 {
            return self.neon_three_same(insn, false, u, size, rd, vn, vm, vd, true);
        }
        unimpl
    }

    #[allow(clippy::too_many_arguments)]
    fn neon_three_same(&mut self, insn: u32, q: bool, u: bool, size: u32, rd: u32, vn: u128, vm: u128, vd: u128, scalar: bool) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let opcode = bits(insn, 15, 11);
        let datasize = if q { 128 } else { 64 };

        // FP three-same: opcodes 11000..11111, size<1> is part of the opcode, size<0> is sz.
        if opcode >= 0b11000 {
            let a = size >> 1 == 1;
            let esize = if size & 1 == 1 { 64 } else { 32 };
            let n = if scalar { 1 } else { datasize / esize };
            let r = match (u, a, opcode) {
                (false, false, 0b11010) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x + y),
                (false, true, 0b11010) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x - y),
                (true, false, 0b11011) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x * y),
                (true, false, 0b11111) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x / y),
                (true, true, 0b11010) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| (x - y).abs()),
                (false, false, 0b11001) => fp_lanes!(esize, n, vn, vm, vd, |x, y, d| x.mul_add(y, d)),
                (false, true, 0b11001) => fp_lanes!(esize, n, vn, vm, vd, |x, y, d| (-x).mul_add(y, d)),
                (false, false, 0b11110) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| if x.is_nan() || y.is_nan() { x + y } else { x.max(y) }),
                (false, true, 0b11110) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| if x.is_nan() || y.is_nan() { x + y } else { x.min(y) }),
                (false, false, 0b11000) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x.max(y)),
                (false, true, 0b11000) => fp_lanes!(esize, n, vn, vm, vd, |x, y, _d| x.min(y)),
                (false, false, 0b11100) => fp_cmp_lanes!(esize, n, vn, vm, |x, y| x == y),
                (true, false, 0b11100) => fp_cmp_lanes!(esize, n, vn, vm, |x, y| x >= y),
                (true, true, 0b11100) => fp_cmp_lanes!(esize, n, vn, vm, |x, y| x > y),
                (true, false, 0b11010) if !scalar => {
                    // FADDP: pairwise over the concatenation n:m
                    let half_n = n / 2;
                    let mut r = 0u128;
                    for i in 0..n {
                        let (src, j) = if i < half_n { (vn, 2 * i) } else { (vm, 2 * (i - half_n)) };
                        let x = if esize == 32 {
                            (f32::from_bits(el(src, 32, j) as u32) + f32::from_bits(el(src, 32, j + 1) as u32)).to_bits() as u64
                        } else {
                            (f64::from_bits(el(src, 64, j)) + f64::from_bits(el(src, 64, j + 1))).to_bits()
                        };
                        r = put(r, esize, i, x);
                    }
                    r
                }
                _ => return unimpl,
            };
            self.q[rd as usize] = if scalar { r & ones(esize) as u128 } else { mask_q(r, q) };
            return Ok(());
        }

        if opcode == 0b00011 && !scalar {
            // Logical: size picks the op.
            let r = match (u, size) {
                (false, 0) => vn & vm,
                (false, 1) => vn & !vm,
                (false, 2) => vn | vm,
                (false, _) => vn | !vm,
                (true, 0) => vn ^ vm,
                (true, 1) => (vd & vn) | (!vd & vm),
                (true, 2) => (vn & vm) | (vd & !vm),
                (true, _) => (vd & vm) | (vn & !vm),
            };
            self.q[rd as usize] = mask_q(r, q);
            return Ok(());
        }

        let esize = 8 << size;
        let n = if scalar { 1 } else { datasize / esize };
        // Pairwise ops read from the concatenation n:m.
        let pairwise = matches!(opcode, 0b10100 | 0b10101 | 0b10111);
        let mut r = 0u128;
        for i in 0..n {
            let (a_u, b_u, a_s, b_s) = if pairwise {
                let (src, j) = if i < n / 2 { (vn, 2 * i) } else { (vm, 2 * (i - n / 2)) };
                (el(src, esize, j), el(src, esize, j + 1), sel(src, esize, j), sel(src, esize, j + 1))
            } else {
                (el(vn, esize, i), el(vm, esize, i), sel(vn, esize, i), sel(vm, esize, i))
            };
            let d = el(vd, esize, i);
            let x = match (opcode, u) {
                (0b00000, false) => ((a_s as i128 + b_s as i128) >> 1) as u64,
                (0b00000, true) => ((a_u as u128 + b_u as u128) >> 1) as u64,
                (0b00010, false) => ((a_s as i128 + b_s as i128 + 1) >> 1) as u64,
                (0b00010, true) => ((a_u as u128 + b_u as u128 + 1) >> 1) as u64,
                (0b00100, false) => ((a_s as i128 - b_s as i128) >> 1) as u64,
                (0b00100, true) => ((a_u as i128 - b_u as i128) >> 1) as u64,
                (0b00001, false) => sat_s(a_s as i128 + b_s as i128, esize),
                (0b00001, true) => sat_u(a_u as i128 + b_u as i128, esize),
                (0b00101, false) => sat_s(a_s as i128 - b_s as i128, esize),
                (0b00101, true) => sat_u(a_u as i128 - b_u as i128, esize),
                (0b00110, false) => all(a_s > b_s),
                (0b00110, true) => all(a_u > b_u),
                (0b00111, false) => all(a_s >= b_s),
                (0b00111, true) => all(a_u >= b_u),
                (0b01000, _) => {
                    // SSHL / USHL: shift by signed low byte of b
                    let sh = b_u as u8 as i8 as i32;
                    if sh >= 0 {
                        if sh as u32 >= esize { 0 } else { a_u << sh }
                    } else {
                        let sh = (-sh) as u32;
                        if u {
                            if sh >= esize { 0 } else { a_u >> sh }
                        } else {
                            (a_s >> sh.min(63)) as u64
                        }
                    }
                }
                (0b01100, false) | (0b10100, false) => a_s.max(b_s) as u64,
                (0b01100, true) | (0b10100, true) => a_u.max(b_u),
                (0b01101, false) | (0b10101, false) => a_s.min(b_s) as u64,
                (0b01101, true) | (0b10101, true) => a_u.min(b_u),
                (0b01110, false) => (a_s - b_s).unsigned_abs(),
                (0b01110, true) => a_u.abs_diff(b_u),
                (0b01111, false) => d.wrapping_add((a_s - b_s).unsigned_abs()),
                (0b01111, true) => d.wrapping_add(a_u.abs_diff(b_u)),
                (0b10000, false) => a_u.wrapping_add(b_u),
                (0b10000, true) => a_u.wrapping_sub(b_u),
                (0b10001, false) => all(a_u & b_u != 0),
                (0b10001, true) => all(a_u == b_u),
                (0b10010, false) => d.wrapping_add(a_u.wrapping_mul(b_u)),
                (0b10010, true) => d.wrapping_sub(a_u.wrapping_mul(b_u)),
                (0b10011, false) => a_u.wrapping_mul(b_u),
                (0b10111, false) => a_u.wrapping_add(b_u), // ADDP
                _ => return unimpl,
            };
            r = put(r, esize, i, x);
        }
        self.q[rd as usize] = if scalar { r & ones(esize) as u128 } else { mask_q(r, q) };
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn neon_three_different(&mut self, insn: u32, q: bool, u: bool, size: u32, rd: u32, vn: u128, vm: u128, vd: u128) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        if size == 3 {
            return unimpl;
        }
        let esize = 8 << size;
        let wide = esize * 2;
        let n = 64 / esize;
        let opcode = bits(insn, 15, 12);
        let ext = |v: u128, i: u32| -> i128 {
            if u { el(v, esize, i) as i128 } else { sel(v, esize, i) as i128 }
        };
        let ext_wide = |v: u128, i: u32| -> i128 {
            if u { el(v, wide, i) as i128 } else { sel(v, wide, i) as i128 }
        };
        let hn = half(vn, q);
        let hm = half(vm, q);

        // Narrowing high-half ops write into the lower or upper half of Rd.
        if opcode == 0b0100 || opcode == 0b0110 {
            let round = if u { 1u128 << (esize - 1) } else { 0 };
            let mut r = 0u128;
            for i in 0..n {
                let (a, b) = (el(vn, wide, i) as u128, el(vm, wide, i) as u128);
                let s = if opcode == 0b0100 { a.wrapping_add(b) } else { a.wrapping_sub(b) };
                r = put(r, esize, i, (s.wrapping_add(round) >> esize) as u64);
            }
            self.q[rd as usize] = if q { (vd & u64::MAX as u128) | (r << 64) } else { r };
            return Ok(());
        }

        let mut r = 0u128;
        for i in 0..n {
            let a = ext(hn, i);
            let b = ext(hm, i);
            let d = ext_wide(vd, i);
            let x: i128 = match opcode {
                0b0000 => a + b,
                0b0001 => ext_wide(vn, i) + b,
                0b0010 => a - b,
                0b0011 => ext_wide(vn, i) - b,
                0b0101 => d + (a - b).abs(),
                0b0111 => (a - b).abs(),
                0b1000 => d + a * b,
                0b1010 => d - a * b,
                0b1100 => a * b,
                _ => return unimpl,
            };
            r = put(r, wide, i, x as u64);
        }
        self.q[rd as usize] = r;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn neon_two_misc(&mut self, insn: u32, q: bool, u: bool, size: u32, rd: u32, vn: u128, vd: u128, scalar: bool) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let opcode = bits(insn, 16, 12);
        let datasize = if q { 128 } else { 64 };

        // FP conversions and FP ops: size<0> is sz, size<1> selects the second table.
        let fp_esize = if size & 1 == 1 { 64 } else { 32 };
        let fp_n = if scalar { 1 } else { datasize / fp_esize };
        let hi = size >> 1 == 1;
        let fp_result = match (u, hi, opcode) {
            (false, false, 0b11101) => Some(self.convert_lanes(vn, fp_esize, fp_n, true, true)),
            (true, false, 0b11101) => Some(self.convert_lanes(vn, fp_esize, fp_n, true, false)),
            (false, true, 0b11011) => Some(self.convert_lanes(vn, fp_esize, fp_n, false, true)),
            (true, true, 0b11011) => Some(self.convert_lanes(vn, fp_esize, fp_n, false, false)),
            // FCVTNS/NU (nearest, ties even), FCVTMS/MU (−∞), FCVTAS/AU (ties away), FCVTPS/PU (+∞)
            (_, false, 0b11010) => Some(convert_rounded(vn, fp_esize, fp_n, !u, f64::round_ties_even)),
            (_, false, 0b11011) => Some(convert_rounded(vn, fp_esize, fp_n, !u, f64::floor)),
            (_, false, 0b11100) => Some(convert_rounded(vn, fp_esize, fp_n, !u, f64::round)),
            (_, true, 0b11010) => Some(convert_rounded(vn, fp_esize, fp_n, !u, f64::ceil)),
            (false, true, 0b01111) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.abs())),
            (true, true, 0b01111) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| -a)),
            (true, true, 0b11111) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.sqrt())),
            (false, false, 0b11000) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.round_ties_even())),
            (false, false, 0b11001) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.floor())),
            (false, true, 0b11000) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.ceil())),
            (false, true, 0b11001) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.trunc())),
            // FRINTA (ties away), FRINTX / FRINTI (current mode: nearest-even, the default FPCR)
            (true, false, 0b11000) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.round())),
            (true, false, 0b11001) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.round_ties_even())),
            (true, true, 0b11001) => Some(fp_lanes!(fp_esize, fp_n, vn, vn, vd, |a, _b, _d| a.round_ties_even())),
            (false, true, 0b01100) => Some(fp_cmp_lanes!(fp_esize, fp_n, vn, vn, |a, _b| a > 0.0)),
            (false, true, 0b01101) => Some(fp_cmp_lanes!(fp_esize, fp_n, vn, vn, |a, _b| a == 0.0)),
            (false, true, 0b01110) => Some(fp_cmp_lanes!(fp_esize, fp_n, vn, vn, |a, _b| a < 0.0)),
            (true, true, 0b01100) => Some(fp_cmp_lanes!(fp_esize, fp_n, vn, vn, |a, _b| a >= 0.0)),
            (true, true, 0b01101) => Some(fp_cmp_lanes!(fp_esize, fp_n, vn, vn, |a, _b| a <= 0.0)),
            // FCVTL: widen half -> single
            (false, false, 0b10111) if size & 1 == 0 && !scalar => {
                let src = half(vn, q);
                let mut r = 0u128;
                for i in 0..4 {
                    r = put(r, 32, i, f16_to_f32(el(src, 16, i) as u16).to_bits() as u64);
                }
                Some(r)
            }
            // FCVTN: narrow single -> half
            (false, false, 0b10110) if size & 1 == 0 && !scalar => {
                let mut r = 0u128;
                for i in 0..4 {
                    r = put(r, 16, i, f32_to_f16(f32::from_bits(el(vn, 32, i) as u32)) as u64);
                }
                Some(if q { (vd & u64::MAX as u128) | (r << 64) } else { r })
            }
            // FCVTL: widen single -> double
            (false, false, 0b10111) if size & 1 == 1 && !scalar => {
                let src = half(vn, q);
                let mut r = 0u128;
                for i in 0..2 {
                    r = put(r, 64, i, (f32::from_bits(el(src, 32, i) as u32) as f64).to_bits());
                }
                Some(r)
            }
            // FCVTN: narrow double -> single
            (false, false, 0b10110) if size & 1 == 1 && !scalar => {
                let mut r = 0u128;
                for i in 0..2 {
                    r = put(r, 32, i, (f64::from_bits(el(vn, 64, i)) as f32).to_bits() as u64);
                }
                Some(if q { (vd & u64::MAX as u128) | (r << 64) } else { r })
            }
            _ => None,
        };
        if let Some(r) = fp_result {
            // FCVTL always writes all 128 bits (Q only picks the source half).
            let widening = opcode == 0b10111 && !u && size >> 1 == 0;
            self.q[rd as usize] = if scalar {
                r & ones(fp_esize) as u128
            } else if widening {
                r
            } else {
                mask_q(r, q)
            };
            return Ok(());
        }

        let esize = 8 << size;
        let n = if scalar { 1 } else { datasize / esize };

        // SHLL / SHLL2: widen each element of a half of Rn and shift it left by its size.
        if (opcode, u) == (0b10011, true) && !scalar {
            if size == 3 {
                return unimpl;
            }
            let src = if q { vn >> 64 } else { vn };
            let wide = esize * 2;
            let mut r = 0u128;
            for i in 0..64 / esize {
                r = put(r, wide, i, el(src, esize, i) << esize);
            }
            self.q[rd as usize] = r;
            return Ok(());
        }
        // XTN / SQXTN / UQXTN / SQXTUN: narrow 2*esize -> esize into a half of Rd
        if matches!((opcode, u), (0b10010, _) | (0b10100, _)) && !scalar {
            if size == 3 {
                return unimpl;
            }
            let wide = esize * 2;
            let mut r = 0u128;
            for i in 0..64 / esize {
                let x = match (opcode, u) {
                    (0b10010, false) => el(vn, wide, i),
                    (0b10010, true) => sat_u(sel(vn, wide, i) as i128, esize), // SQXTUN
                    (0b10100, false) => sat_s(sel(vn, wide, i) as i128, esize),
                    _ => sat_u(el(vn, wide, i) as i128, esize),
                };
                r = put(r, esize, i, x);
            }
            self.q[rd as usize] = if q { (vd & u64::MAX as u128) | (r << 64) } else { r };
            return Ok(());
        }
        // SADDLP / UADDLP / SADALP / UADALP
        if matches!(opcode, 0b00010 | 0b00110) && !scalar {
            if size == 3 {
                return unimpl;
            }
            let wide = esize * 2;
            let mut r = 0u128;
            for i in 0..datasize / wide {
                let s = if u {
                    el(vn, esize, 2 * i).wrapping_add(el(vn, esize, 2 * i + 1))
                } else {
                    sel(vn, esize, 2 * i).wrapping_add(sel(vn, esize, 2 * i + 1)) as u64
                };
                let s = if opcode == 0b00110 { s.wrapping_add(el(vd, wide, i)) } else { s };
                r = put(r, wide, i, s);
            }
            self.q[rd as usize] = mask_q(r, q);
            return Ok(());
        }

        let r = if matches!((opcode, u), (0b00000, _) | (0b00001, false)) {
            // REV64 / REV32 / REV16: reverse elements within containers
            let container = match (opcode, u) {
                (0b00000, false) => 64,
                (0b00000, true) => 32,
                _ => 16,
            };
            if esize >= container {
                return unimpl;
            }
            let per = container / esize;
            let mut r = 0u128;
            for i in 0..n {
                let base = i / per * per;
                let j = base + (per - 1 - (i - base));
                r = put(r, esize, i, el(vn, esize, j));
            }
            r
        } else if opcode == 0b00101 && u {
            // NOT (size 0) / RBIT (size 1)
            match size {
                0 => !vn,
                1 => (0..datasize / 8).fold(0u128, |r, i| put(r, 8, i, (el(vn, 8, i) as u8).reverse_bits() as u64)),
                _ => return unimpl,
            }
        } else {
            let mut r = 0u128;
            for i in 0..n {
                let a_u = el(vn, esize, i);
                let a_s = sel(vn, esize, i);
                let x = match (opcode, u) {
                    (0b00101, false) if size == 0 => a_u.count_ones() as u64, // CNT
                    (0b00100, true) => (a_u.leading_zeros() - (64 - esize)) as u64,
                    (0b00100, false) => (((a_s ^ (a_s >> 63)) as u64).leading_zeros() - (64 - esize) - 1) as u64,
                    (0b01000, false) => all(a_s > 0),
                    (0b01000, true) => all(a_s >= 0),
                    (0b01001, false) => all(a_s == 0),
                    (0b01001, true) => all(a_s <= 0),
                    (0b01010, false) => all(a_s < 0),
                    (0b01011, false) => a_s.unsigned_abs(),
                    (0b01011, true) => a_s.wrapping_neg() as u64,
                    _ => return unimpl,
                };
                r = put(r, esize, i, x);
            }
            r
        };
        self.q[rd as usize] = if scalar { r & ones(esize) as u128 } else { mask_q(r, q) };
        Ok(())
    }

    /// SCVTF/UCVTF (to_fp) or FCVTZS/FCVTZU, lane-wise.
    fn convert_lanes(&self, v: u128, esize: u32, n: u32, to_fp: bool, signed: bool) -> u128 {
        let mut r = 0u128;
        for i in 0..n {
            let x = el(v, esize, i);
            let out = match (to_fp, esize, signed) {
                (true, 32, true) => (x as u32 as i32 as f32).to_bits() as u64,
                (true, 32, false) => (x as u32 as f32).to_bits() as u64,
                (true, _, true) => (x as i64 as f64).to_bits(),
                (true, _, false) => (x as f64).to_bits(),
                (false, 32, true) => f32::from_bits(x as u32) as i32 as u32 as u64,
                (false, 32, false) => f32::from_bits(x as u32) as u32 as u64,
                (false, _, true) => f64::from_bits(x) as i64 as u64,
                (false, _, false) => f64::from_bits(x) as u64,
            };
            r = put(r, esize, i, out);
        }
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn neon_shift_imm(&mut self, insn: u32, q: bool, u: bool, rd: u32, _rn: u32, vn: u128, vd: u128) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let immh = bits(insn, 22, 19);
        let immhb = bits(insn, 22, 16);
        let esize = 8 << (31 - immh.leading_zeros());
        let opcode = bits(insn, 15, 11);
        let datasize = if q { 128 } else { 64 };
        let n = datasize / esize;
        let shr = 2 * esize - immhb;
        let shl = immhb - esize;

        let r = match opcode {
            // SSHR/USHR, SSRA/USRA, SRSHR/URSHR, SRSRA/URSRA
            0b00000 | 0b00010 | 0b00100 | 0b00110 => {
                let accumulate = opcode & 0b00010 != 0;
                let round = opcode & 0b00100 != 0;
                let mut r = 0u128;
                for i in 0..n {
                    let a: i128 = if u { el(vn, esize, i) as i128 } else { sel(vn, esize, i) as i128 };
                    let a = if round { a + (1i128 << (shr - 1)) } else { a };
                    let mut x = (a >> shr) as u64;
                    if accumulate {
                        x = x.wrapping_add(el(vd, esize, i));
                    }
                    r = put(r, esize, i, x);
                }
                r
            }
            // SHL
            0b01010 if !u => (0..n).fold(0u128, |r, i| put(r, esize, i, el(vn, esize, i) << shl)),
            // SLI
            0b01010 => (0..n).fold(0u128, |r, i| {
                let m = ones(esize) << shl & ones(esize);
                put(r, esize, i, (el(vd, esize, i) & !m) | (el(vn, esize, i) << shl & m))
            }),
            // SRI
            0b01000 if u => (0..n).fold(0u128, |r, i| {
                let m = if shr >= esize { 0 } else { ones(esize) >> shr };
                let x = if shr >= esize { 0 } else { el(vn, esize, i) >> shr };
                put(r, esize, i, (el(vd, esize, i) & !m) | x)
            }),
            // SHRN / RSHRN: narrow from 2*esize
            0b10000 | 0b10001 if !u => {
                if esize == 64 {
                    return unimpl;
                }
                let wide = esize * 2;
                let shr = wide - immhb;
                let round = if opcode == 0b10001 { 1u128 << (shr - 1) } else { 0 };
                let mut r = 0u128;
                for i in 0..64 / esize {
                    r = put(r, esize, i, ((el(vn, wide, i) as u128 + round) >> shr) as u64);
                }
                let r = if q { (vd & u64::MAX as u128) | (r << 64) } else { r };
                self.q[rd as usize] = r;
                return Ok(());
            }
            // SSHLL / USHLL (SXTL / UXTL)
            0b10100 => {
                if esize == 64 {
                    return unimpl;
                }
                let src = half(vn, q);
                let wide = esize * 2;
                let mut r = 0u128;
                for i in 0..64 / esize {
                    let x = if u { el(src, esize, i) } else { sel(src, esize, i) as u64 };
                    r = put(r, wide, i, x << shl);
                }
                self.q[rd as usize] = r;
                return Ok(());
            }
            // SCVTF/UCVTF (fixed-point) and FCVTZS/FCVTZU (fixed-point)
            0b11100 | 0b11111 if esize >= 32 => {
                let frac = shr;
                let scale = 2f64.powi(frac as i32);
                let mut r = 0u128;
                for i in 0..n {
                    let x = el(vn, esize, i);
                    let out = if opcode == 0b11100 {
                        let v = if u { x as f64 } else { sext(x, esize) as f64 } / scale;
                        if esize == 32 { (v as f32).to_bits() as u64 } else { v.to_bits() }
                    } else {
                        let f = if esize == 32 { f32::from_bits(x as u32) as f64 } else { f64::from_bits(x) } * scale;
                        match (u, esize) {
                            (false, 32) => f as i32 as u32 as u64,
                            (true, 32) => f as u32 as u64,
                            (false, _) => f as i64 as u64,
                            (true, _) => f as u64,
                        }
                    };
                    r = put(r, esize, i, out);
                }
                r
            }
            _ => return unimpl,
        };
        self.q[rd as usize] = mask_q(r, q);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn neon_by_element(&mut self, insn: u32, q: bool, u: bool, size: u32, rd: u32, _rn: u32, vn: u128, vd: u128) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let opcode = bits(insn, 15, 12);
        let (h, l, m) = (bits(insn, 11, 11), bits(insn, 21, 21), bits(insn, 20, 20));
        let datasize = if q { 128 } else { 64 };
        let fp = matches!(opcode, 0b0001 | 0b0101 | 0b1001) && !u || (opcode == 0b1001 && u);

        if fp {
            let esize = if size & 1 == 1 { 64 } else { 32 };
            if size >> 1 != 1 {
                return unimpl;
            }
            let (index, rm) = if esize == 64 { (h, bits(insn, 20, 16)) } else { ((h << 1) | l, bits(insn, 20, 16)) };
            let elem = el(self.q[rm as usize], esize, index);
            let vm = rep(elem, esize);
            let n = datasize / esize;
            let r = match opcode {
                0b1001 => fp_lanes!(esize, n, vn, vm, vd, |a, b, _d| a * b),
                0b0001 => fp_lanes!(esize, n, vn, vm, vd, |a, b, d| a.mul_add(b, d)),
                _ => fp_lanes!(esize, n, vn, vm, vd, |a, b, d| (-a).mul_add(b, d)),
            };
            self.q[rd as usize] = mask_q(r, q);
            return Ok(());
        }

        let esize = 8 << size;
        let (index, rm) = match size {
            1 => ((h << 2) | (l << 1) | m, bits(insn, 19, 16)),
            2 => ((h << 1) | l, bits(insn, 20, 16)),
            _ => return unimpl,
        };
        let b = el(self.q[rm as usize], esize, index);
        let bs = sext(b, esize);
        match opcode {
            // MUL / MLA / MLS
            0b1000 | 0b0000 | 0b0100 => {
                let n = datasize / esize;
                let mut r = 0u128;
                for i in 0..n {
                    let p = el(vn, esize, i).wrapping_mul(b);
                    let x = match (opcode, u) {
                        (0b1000, false) => p,
                        (0b0000, true) => el(vd, esize, i).wrapping_add(p),
                        (0b0100, true) => el(vd, esize, i).wrapping_sub(p),
                        _ => return unimpl,
                    };
                    r = put(r, esize, i, x);
                }
                self.q[rd as usize] = mask_q(r, q);
            }
            // SMULL/UMULL, SMLAL/UMLAL, SMLSL/UMLSL (by element)
            0b1010 | 0b0010 | 0b0110 => {
                let wide = esize * 2;
                let src = half(vn, q);
                let mut r = 0u128;
                for i in 0..64 / esize {
                    let p: i128 = if u {
                        el(src, esize, i) as i128 * b as i128
                    } else {
                        sel(src, esize, i) as i128 * bs as i128
                    };
                    let d = el(vd, wide, i) as i128;
                    let x = match opcode {
                        0b1010 => p,
                        0b0010 => d + p,
                        _ => d - p,
                    };
                    r = put(r, wide, i, x as u64);
                }
                self.q[rd as usize] = r;
            }
            _ => return unimpl,
        }
        Ok(())
    }
}

/// IEEE half → single (exact).
pub(crate) fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) as u32) << 31;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = match (exp, frac) {
        (0, 0) => sign,
        (0, f) => {
            // Subnormal: normalize.
            let shift = f.leading_zeros() - 21;
            let f = (f << shift) & 0x3ff;
            sign | ((127 - 15 + 1 - shift) << 23) | (f << 13)
        }
        (0x1f, f) => sign | 0x7f80_0000 | (f << 13) | if f != 0 { 0x40_0000 } else { 0 },
        (e, f) => sign | ((e + 127 - 15) << 23) | (f << 13),
    };
    f32::from_bits(bits)
}

/// Single → IEEE half, rounding to nearest even (overflow → infinity).
pub(crate) fn f32_to_f16(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32;
    let frac = b & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if frac != 0 { 0x200 | (frac >> 13) as u16 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    // Mantissa with the implicit bit, and how far to shift it down to 10 bits.
    let (m, shift) = if e <= 0 {
        if e < -10 {
            return sign;
        }
        (frac | 0x80_0000, (14 - e) as u32)
    } else {
        (frac, 13)
    };
    let mut v = m >> shift;
    let rem = m & ((1 << shift) - 1);
    let halfway = 1 << (shift - 1);
    if rem > halfway || (rem == halfway && v & 1 == 1) {
        v += 1;
    }
    let out = if e <= 0 { v } else { ((e as u32) << 10) + v };
    sign | out as u16
}

/// FP → integer lane-wise after `round` (saturating, NaN → 0, like the hardware).
fn convert_rounded(v: u128, esize: u32, n: u32, signed: bool, round: fn(f64) -> f64) -> u128 {
    let mut r = 0u128;
    for i in 0..n {
        let x = el(v, esize, i);
        let f = round(if esize == 32 { f32::from_bits(x as u32) as f64 } else { f64::from_bits(x) });
        let out = match (esize, signed) {
            (32, true) => f as i32 as u32 as u64,
            (32, false) => f as u32 as u64,
            (_, true) => f as i64 as u64,
            (_, false) => f as u64,
        };
        r = put(r, esize, i, out);
    }
    r
}

