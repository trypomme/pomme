//! SIMD & floating point: scalar FP, modified immediates and the copy group.
//! The rest of Advanced SIMD lives in `neon.rs`.
//!
//! FP semantics use the host's IEEE arithmetic with round-to-nearest; FPCR
//! rounding modes and exception flags are not modelled yet.

use crate::{Cpu, Exit, Step, bit, bits};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    S,
    D,
}

fn ty(insn: u32) -> Option<Ty> {
    match bits(insn, 23, 22) {
        0 => Some(Ty::S),
        1 => Some(Ty::D),
        _ => None, // half precision: not yet
    }
}

fn vfp_expand_imm(imm8: u32, ty: Ty) -> u64 {
    let sign = (imm8 >> 7) as u64;
    let b6 = (imm8 >> 6) & 1;
    let low = ((imm8 >> 4) & 3) as u64;
    let frac = (imm8 & 0xf) as u64;
    match ty {
        Ty::D => {
            let exp = (((b6 ^ 1) as u64) << 10) | (if b6 == 1 { 0xff } else { 0 } << 2) | low;
            (sign << 63) | (exp << 52) | (frac << 48)
        }
        Ty::S => {
            let exp = (((b6 ^ 1) as u64) << 7) | (if b6 == 1 { 0x1f } else { 0 } << 2) | low;
            (sign << 31) | (exp << 23) | (frac << 19)
        }
    }
}

fn rep(v: u64, esize: u32) -> u128 {
    let mut out = 0u128;
    let mut i = 0;
    while i < 128 {
        out |= (v as u128) << i;
        i += esize;
    }
    out
}

fn adv_simd_expand_imm(op: bool, cmode: u32, imm8: u64) -> Option<u128> {
    Some(match cmode >> 1 {
        0b000 => rep(imm8, 32),
        0b001 => rep(imm8 << 8, 32),
        0b010 => rep(imm8 << 16, 32),
        0b011 => rep(imm8 << 24, 32),
        0b100 => rep(imm8, 16),
        0b101 => rep(imm8 << 8, 16),
        0b110 => {
            if cmode & 1 == 0 {
                rep((imm8 << 8) | 0xff, 32)
            } else {
                rep((imm8 << 16) | 0xffff, 32)
            }
        }
        _ => match (cmode & 1, op) {
            (0, false) => rep(imm8, 8),
            (0, true) => {
                let mut v = 0u64;
                for i in 0..8 {
                    if (imm8 >> i) & 1 == 1 {
                        v |= 0xff << (i * 8);
                    }
                }
                rep(v, 64)
            }
            (1, false) => rep(vfp_expand_imm(imm8 as u32, Ty::S), 32),
            _ => rep(vfp_expand_imm(imm8 as u32, Ty::D), 64),
        },
    })
}

fn lane(v: u128, esize: u32, i: u32) -> u64 {
    ((v >> (i * esize)) as u64) & crate::ones(esize)
}

fn set_lane(v: u128, esize: u32, i: u32, x: u64) -> u128 {
    let m = (crate::ones(esize) as u128) << (i * esize);
    (v & !m) | (((x & crate::ones(esize)) as u128) << (i * esize))
}

impl Cpu {
    fn s(&self, r: u32) -> f32 {
        f32::from_bits(self.q[r as usize] as u32)
    }

    fn d(&self, r: u32) -> f64 {
        f64::from_bits(self.q[r as usize] as u64)
    }

    fn set_s(&mut self, r: u32, v: f32) {
        self.q[r as usize] = v.to_bits() as u128;
    }

    fn set_d(&mut self, r: u32, v: f64) {
        self.q[r as usize] = v.to_bits() as u128;
    }

    fn fp(&self, t: Ty, r: u32) -> f64 {
        match t {
            Ty::S => self.s(r) as f64,
            Ty::D => self.d(r),
        }
    }

    fn set_fp(&mut self, t: Ty, r: u32, v: f64) {
        match t {
            Ty::S => self.set_s(r, v as f32),
            Ty::D => self.set_d(r, v),
        }
    }

    fn fcmp_flags(a: f64, b: f64) -> u32 {
        if a.is_nan() || b.is_nan() {
            0b0011
        } else if a == b {
            0b0110
        } else if a < b {
            0b1000
        } else {
            0b0010
        }
    }

    /// SCVTF/UCVTF/FCVTZS/FCVTZU with a fixed-point scale: value = int / 2^fbits.
    fn fp_fixed(&mut self, insn: u32) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let (rd, rn) = (bits(insn, 4, 0), bits(insn, 9, 5));
        let sf = bit(insn, 31);
        let Some(t) = ty(insn) else { return unimpl };
        let (rmode, opcode) = (bits(insn, 20, 19), bits(insn, 18, 16));
        let fbits = 64 - bits(insn, 15, 10) as i32;
        if !sf && fbits > 32 {
            return unimpl;
        }
        let scale = 2f64.powi(fbits);
        match (rmode, opcode) {
            (0, 0b010) | (0, 0b011) => {
                let x = self.xr(rn);
                let v = match (opcode == 0b010, sf) {
                    (true, true) => x as i64 as f64,
                    (true, false) => x as u32 as i32 as f64,
                    (false, true) => x as f64,
                    (false, false) => x as u32 as f64,
                } / scale;
                match t {
                    Ty::S => self.set_s(rd, v as f32),
                    _ => self.set_d(rd, v),
                }
            }
            (3, 0b000) | (3, 0b001) => {
                let f = self.fp(t, rn) * scale;
                let r = f.trunc();
                let v = match (opcode == 0b001, sf) {
                    (false, true) => r as i64 as u64,
                    (false, false) => r as i32 as u32 as u64,
                    (true, true) => r as u64,
                    (true, false) => r as u32 as u64,
                };
                self.set_xr(rd, v);
            }
            _ => return unimpl,
        }
        Ok(())
    }

    pub(crate) fn simd_fp(&mut self, insn: u32) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let rd = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let rm = bits(insn, 20, 16);

        // ---- Scalar floating point (bit 28 = 1, bit 30 = 0) ----
        if insn & 0x5f00_0000 == 0x1e00_0000 && bit(insn, 21) {
            return self.fp_scalar(insn);
        }
        // Conversion between floating point and fixed point (bit 21 = 0).
        if insn & 0x5f20_0000 == 0x1e00_0000 {
            return self.fp_fixed(insn);
        }
        // FP data-processing (3 source)
        if insn & 0x5f00_0000 == 0x1f00_0000 {
            let Some(t) = ty(insn) else { return unimpl };
            let ra = bits(insn, 14, 10);
            // Arithmetic in the operand's own precision so S results round once.
            let (o1, o0) = (bit(insn, 21), bit(insn, 15));
            match t {
                Ty::D => {
                    let (a, n, m) = (self.d(ra), self.d(rn), self.d(rm));
                    let r = match (o1, o0) {
                        (false, false) => n.mul_add(m, a),
                        (false, true) => (-n).mul_add(m, a),
                        (true, false) => (-n).mul_add(m, -a),
                        (true, true) => n.mul_add(m, -a),
                    };
                    self.set_d(rd, r);
                }
                Ty::S => {
                    let (a, n, m) = (self.s(ra), self.s(rn), self.s(rm));
                    let r = match (o1, o0) {
                        (false, false) => n.mul_add(m, a),
                        (false, true) => (-n).mul_add(m, a),
                        (true, false) => (-n).mul_add(m, -a),
                        (true, true) => n.mul_add(m, -a),
                    };
                    self.set_s(rd, r);
                }
            }
            return Ok(());
        }

        // ---- Advanced SIMD ----
        let q = bit(insn, 30);
        let finish = |v: u128| if q { v } else { v & (u64::MAX as u128) };

        // MOVI / MVNI / ORR / BIC (vector, immediate) and FMOV (vector, immediate)
        if insn & 0x9ff8_0400 == 0x0f00_0400 {
            let op = bit(insn, 29);
            let cmode = bits(insn, 15, 12);
            let imm8 = ((bits(insn, 18, 16) << 5) | bits(insn, 9, 5)) as u64;
            let Some(imm) = adv_simd_expand_imm(op, cmode, imm8) else { return unimpl };
            let is_orr_bic = cmode < 0b1100 && cmode & 1 == 1;
            let old = self.q[rd as usize];
            let r = if is_orr_bic {
                if op { old & !imm } else { old | imm }
            } else if op && cmode != 0b1110 && cmode != 0b1111 {
                !imm // MVNI
            } else {
                imm
            };
            self.q[rd as usize] = finish(r);
            return Ok(());
        }

        // Copy group: DUP/INS/UMOV/SMOV
        if insn & 0x9fe0_8400 == 0x0e00_0400 {
            let imm5 = bits(insn, 20, 16);
            let imm4 = bits(insn, 14, 11);
            if imm5 & 0xf == 0 {
                return unimpl;
            }
            let size = imm5.trailing_zeros(); // 0=B 1=H 2=S 3=D
            let esize = 8 << size;
            let index = imm5 >> (size + 1);
            let op = bit(insn, 29);
            match (op, imm4) {
                // DUP (element)
                (false, 0b0000) => {
                    let x = lane(self.q[rn as usize], esize, index);
                    self.q[rd as usize] = finish(rep(x, esize));
                }
                // DUP (general)
                (false, 0b0001) => {
                    let x = self.xr(rn) & crate::ones(esize);
                    self.q[rd as usize] = finish(rep(x, esize));
                }
                // SMOV
                (false, 0b0101) => {
                    let x = crate::sext(lane(self.q[rn as usize], esize, index), esize) as u64;
                    self.set_xr(rd, if q { x } else { x as u32 as u64 });
                }
                // UMOV
                (false, 0b0111) => {
                    let x = lane(self.q[rn as usize], esize, index);
                    self.set_xr(rd, x);
                }
                // INS (general)
                (false, 0b0011) if q => {
                    let x = self.xr(rn);
                    self.q[rd as usize] = set_lane(self.q[rd as usize], esize, index, x);
                }
                // INS (element)
                (true, _) if q => {
                    let src = imm4 >> size;
                    let x = lane(self.q[rn as usize], esize, src);
                    self.q[rd as usize] = set_lane(self.q[rd as usize], esize, index, x);
                }
                _ => return unimpl,
            }
            return Ok(());
        }

        // Everything else in Advanced SIMD.
        self.neon(insn)
    }

    fn fp_scalar(&mut self, insn: u32) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let rd = bits(insn, 4, 0);
        let rn = bits(insn, 9, 5);
        let rm = bits(insn, 20, 16);
        let sf = bit(insn, 31);

        // Conversion between FP and integer (bits 15:10 == 0)
        if bits(insn, 15, 10) == 0 {
            let rmode = bits(insn, 20, 19);
            let opcode = bits(insn, 18, 16);
            let tybits = bits(insn, 23, 22);
            // FMOV (general)
            match (sf, tybits, rmode, opcode) {
                (false, 0, 0, 0b110) => {
                    self.set_xr(rd, self.q[rn as usize] as u32 as u64);
                    return Ok(());
                }
                (false, 0, 0, 0b111) => {
                    self.q[rd as usize] = self.xr(rn) as u32 as u128;
                    return Ok(());
                }
                (true, 1, 0, 0b110) => {
                    self.set_xr(rd, self.q[rn as usize] as u64);
                    return Ok(());
                }
                (true, 1, 0, 0b111) => {
                    self.q[rd as usize] = self.xr(rn) as u128;
                    return Ok(());
                }
                (true, 2, 1, 0b110) => {
                    self.set_xr(rd, (self.q[rn as usize] >> 64) as u64);
                    return Ok(());
                }
                (true, 2, 1, 0b111) => {
                    let lo = self.q[rd as usize] as u64 as u128;
                    self.q[rd as usize] = lo | ((self.xr(rn) as u128) << 64);
                    return Ok(());
                }
                _ => {}
            }
            let Some(t) = ty(insn) else { return unimpl };
            match opcode {
                // SCVTF / UCVTF
                0b010 | 0b011 if rmode == 0 => {
                    let x = self.xr(rn);
                    let v = match (opcode == 0b010, sf) {
                        (true, true) => x as i64 as f64,
                        (true, false) => x as u32 as i32 as f64,
                        (false, true) => x as f64,
                        (false, false) => x as u32 as f64,
                    };
                    // Convert directly to f32 where it matters for rounding.
                    if t == Ty::S {
                        let v32 = match (opcode == 0b010, sf) {
                            (true, true) => x as i64 as f32,
                            (false, true) => x as f32,
                            _ => v as f32,
                        };
                        self.set_s(rd, v32);
                    } else {
                        self.set_d(rd, v);
                    }
                }
                // FCVT{N,P,M,Z,A}{S,U}
                0b000 | 0b001 | 0b100 | 0b101 => {
                    let f = self.fp(t, rn);
                    let r = match (opcode & 0b100 != 0, rmode) {
                        (false, 0) => f.round_ties_even(), // N
                        (false, 1) => f.ceil(),            // P
                        (false, 2) => f.floor(),           // M
                        (false, _) => f.trunc(),           // Z
                        (true, 0) => f.round(),            // A (ties away)
                        _ => return unimpl,
                    };
                    let unsigned = opcode & 1 == 1;
                    let v = match (unsigned, sf) {
                        (false, true) => r as i64 as u64,
                        (false, false) => r as i32 as u32 as u64,
                        (true, true) => r as u64,
                        (true, false) => r as u32 as u64,
                    };
                    self.set_xr(rd, v);
                }
                _ => return unimpl,
            }
            return Ok(());
        }

        let Some(t) = ty(insn) else { return unimpl };

        // FP data-processing (1 source)
        if bits(insn, 14, 10) == 0b10000 {
            let opcode = bits(insn, 20, 15);
            let a = self.fp(t, rn);
            match opcode {
                0b000000 => self.q[rd as usize] = if t == Ty::S { self.q[rn as usize] as u32 as u128 } else { self.q[rn as usize] as u64 as u128 },
                0b000001 => self.set_fp(t, rd, a.abs()),
                0b000010 => self.set_fp(t, rd, -a),
                0b000011 => match t {
                    Ty::S => self.set_s(rd, self.s(rn).sqrt()),
                    Ty::D => self.set_d(rd, a.sqrt()),
                },
                // FCVT between precisions
                0b000100 => self.set_s(rd, a as f32),
                0b000101 => self.set_d(rd, a),
                // FCVT Hd, Sn/Dn
                0b000111 => self.q[rd as usize] = crate::neon::f32_to_f16(a as f32) as u128,
                // FRINT{N,P,M,Z,A,X,I}
                0b001000 => self.set_fp(t, rd, a.round_ties_even()),
                0b001001 => self.set_fp(t, rd, a.ceil()),
                0b001010 => self.set_fp(t, rd, a.floor()),
                0b001011 => self.set_fp(t, rd, a.trunc()),
                0b001100 => self.set_fp(t, rd, a.round()),
                0b001110 | 0b001111 => self.set_fp(t, rd, a.round_ties_even()),
                _ => return unimpl,
            }
            return Ok(());
        }

        // FP compare
        if bits(insn, 13, 10) == 0b1000 {
            let a = self.fp(t, rn);
            let b = if bit(insn, 3) { 0.0 } else { self.fp(t, rm) };
            self.set_nzcv4(Self::fcmp_flags(a, b));
            return Ok(());
        }

        // FMOV (scalar, immediate)
        if bits(insn, 12, 10) == 0b100 && bits(insn, 9, 5) == 0 {
            let imm = vfp_expand_imm(bits(insn, 20, 13), t);
            self.q[rd as usize] = imm as u128;
            return Ok(());
        }

        match bits(insn, 11, 10) {
            // FCCMP / FCCMPE
            0b01 => {
                if self.cond(bits(insn, 15, 12)) {
                    let f = Self::fcmp_flags(self.fp(t, rn), self.fp(t, rm));
                    self.set_nzcv4(f);
                } else {
                    self.set_nzcv4(bits(insn, 3, 0));
                }
            }
            // FP data-processing (2 source)
            0b10 => {
                let opcode = bits(insn, 15, 12);
                macro_rules! arith {
                    ($a:expr, $b:expr) => {{
                        let (a, b) = ($a, $b);
                        match opcode {
                            0b0000 => a * b,
                            0b0001 => a / b,
                            0b0010 => a + b,
                            0b0011 => a - b,
                            0b0100 => if a.is_nan() || b.is_nan() { a + b } else { a.max(b) },
                            0b0101 => if a.is_nan() || b.is_nan() { a + b } else { a.min(b) },
                            0b0110 => a.max(b),
                            0b0111 => a.min(b),
                            0b1000 => -(a * b),
                            _ => return unimpl,
                        }
                    }};
                }
                match t {
                    Ty::S => {
                        let r = arith!(self.s(rn), self.s(rm));
                        self.set_s(rd, r);
                    }
                    Ty::D => {
                        let r = arith!(self.d(rn), self.d(rm));
                        self.set_d(rd, r);
                    }
                }
            }
            // FCSEL
            _ => {
                let src = if self.cond(bits(insn, 15, 12)) { rn } else { rm };
                self.q[rd as usize] = match t {
                    Ty::S => self.q[src as usize] as u32 as u128,
                    Ty::D => self.q[src as usize] as u64 as u128,
                };
            }
        }
        Ok(())
    }
}
