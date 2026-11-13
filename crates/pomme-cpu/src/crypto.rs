//! The ARMv8 cryptographic extension, on every 64-bit iPhone: AES rounds
//! (AESE, AESD, AESMC, AESIMC), SHA-1 and SHA-256 hashing and message
//! schedules, and polynomial multiplication (PMULL, which AES-GCM's GHASH and
//! CRC folding use). TLS libraries, hash functions and random generators
//! such as Abseil's Randen are built on them.
//!
//! Each instruction follows the Arm ARM's pseudocode. A vector's byte `i` is
//! bits `8i..8i+8`; AES treats the 16 bytes as its column-major state.

use crate::{Cpu, Exit, Step, bits};

/// The AES S-box and its inverse, built from their definition: the inverse
/// in GF(2^8) modulo x^8 + x^4 + x^3 + x + 1, then the affine transform.
const fn sboxes() -> ([u8; 256], [u8; 256]) {
    let mut sbox = [0u8; 256];
    let mut inv = [0u8; 256];
    let mut x = 0;
    while x < 256 {
        // x^254 is x's inverse (and 0 for 0).
        let mut r = 1u8;
        let mut e = 0;
        while e < 254 {
            r = gmul(r, x as u8);
            e += 1;
        }
        let b = if x == 0 { 0 } else { r };
        let s = b ^ b.rotate_left(1) ^ b.rotate_left(2) ^ b.rotate_left(3) ^ b.rotate_left(4) ^ 0x63;
        sbox[x] = s;
        inv[s as usize] = x as u8;
        x += 1;
    }
    (sbox, inv)
}

const SBOXES: ([u8; 256], [u8; 256]) = sboxes();
const SBOX: [u8; 256] = SBOXES.0;
const INV_SBOX: [u8; 256] = SBOXES.1;

/// Multiplication in AES's field.
const fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut r = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a = (a << 1) ^ if a & 0x80 != 0 { 0x1b } else { 0 };
        b >>= 1;
    }
    r
}

fn bytes(v: u128) -> [u8; 16] {
    v.to_le_bytes()
}

fn vector(b: [u8; 16]) -> u128 {
    u128::from_le_bytes(b)
}

/// ShiftRows, or its inverse: row `r` of the column-major state turns by `r` columns.
fn shift_rows(s: [u8; 16], inverse: bool) -> [u8; 16] {
    let mut out = [0u8; 16];
    for c in 0..4 {
        for r in 0..4 {
            let from = if inverse { (c + 4 - r) % 4 } else { (c + r) % 4 };
            out[r + 4 * c] = s[r + 4 * from];
        }
    }
    out
}

/// MixColumns, or its inverse, on each column.
fn mix_columns(s: [u8; 16], inverse: bool) -> [u8; 16] {
    let m: [u8; 4] = if inverse { [14, 11, 13, 9] } else { [2, 3, 1, 1] };
    let mut out = [0u8; 16];
    for c in 0..4 {
        let col = &s[4 * c..4 * c + 4];
        for r in 0..4 {
            out[4 * c + r] = (0..4).fold(0, |acc, k| acc ^ gmul(col[k], m[(k + 4 - r) % 4]));
        }
    }
    out
}

/// AESE (`decrypt` false) or AESD: AddRoundKey, (Inv)ShiftRows, (Inv)SubBytes.
fn aes_round(state: u128, key: u128, decrypt: bool) -> u128 {
    let box_ = if decrypt { &INV_SBOX } else { &SBOX };
    vector(shift_rows(bytes(state ^ key), decrypt).map(|b| box_[b as usize]))
}

fn word(v: u128, i: u32) -> u32 {
    (v >> (32 * i)) as u32
}

fn words(w: [u32; 4]) -> u128 {
    w.iter().enumerate().fold(0, |acc, (i, &x)| acc | (x as u128) << (32 * i))
}

fn choose(x: u32, y: u32, z: u32) -> u32 {
    ((y ^ z) & x) ^ z
}

fn majority(x: u32, y: u32, z: u32) -> u32 {
    (x & y) | ((x | y) & z)
}

fn parity(x: u32, y: u32, z: u32) -> u32 {
    x ^ y ^ z
}

/// SHA1C, SHA1P and SHA1M: four rounds of SHA-1 on `abcd` and `e`.
fn sha1_rounds(mut x: u128, mut y: u32, w: u128, f: fn(u32, u32, u32) -> u32) -> u128 {
    for e in 0..4 {
        let t = f(word(x, 1), word(x, 2), word(x, 3));
        y = y.wrapping_add(word(x, 0).rotate_left(5)).wrapping_add(t).wrapping_add(word(w, e));
        let b = word(x, 1).rotate_left(30);
        x = (x & !(0xffff_ffffu128 << 32)) | (b as u128) << 32;
        // <y, X> = ROL(y : X, 32)
        let top = word(x, 3);
        x = (x << 32) | y as u128;
        y = top;
    }
    x
}

/// SHA256H (`part1`) and SHA256H2: four rounds of SHA-256.
fn sha256_hash(mut x: u128, mut y: u128, w: u128, part1: bool) -> u128 {
    for e in 0..4 {
        let chs = choose(word(y, 0), word(y, 1), word(y, 2));
        let maj = majority(word(x, 0), word(x, 1), word(x, 2));
        let y0 = word(y, 0);
        let sigma1 = y0.rotate_right(6) ^ y0.rotate_right(11) ^ y0.rotate_right(25);
        let t = word(y, 3).wrapping_add(sigma1).wrapping_add(chs).wrapping_add(word(w, e));
        let x0 = word(x, 0);
        let sigma0 = x0.rotate_right(2) ^ x0.rotate_right(13) ^ x0.rotate_right(22);
        let x3 = t.wrapping_add(word(x, 3));
        let y3 = t.wrapping_add(sigma0).wrapping_add(maj);
        // <Y, X> = ROL(Y : X, 32), with the new top words.
        let (nx, ny) = ((x << 32) | y3 as u128, (y << 32) | x3 as u128);
        x = nx;
        y = ny;
    }
    if part1 { x } else { y }
}

/// Carry-less multiplication.
pub(crate) fn clmul(a: u64, b: u64) -> u128 {
    (0..64).filter(|i| b >> i & 1 != 0).fold(0, |acc, i| acc ^ (a as u128) << i)
}

/// Whether `insn` is AES or SHA (PMULL lives among the three-different ops).
pub(crate) fn is_crypto(insn: u32) -> bool {
    insn & 0xffff_cc00 == 0x4e28_4800 || insn & 0xffe0_8c00 == 0x5e00_0000 || insn & 0xffff_cc00 == 0x5e28_0800
}

impl Cpu {
    pub(crate) fn crypto(&mut self, insn: u32) -> Step {
        let unimpl = Err(Exit::Unimplemented { insn });
        let (rd, rn, rm) = (bits(insn, 4, 0) as usize, bits(insn, 9, 5) as usize, bits(insn, 20, 16) as usize);
        let (d, n, m) = (self.q[rd], self.q[rn], self.q[rm]);
        let opc = bits(insn, 14, 12);
        let r = if insn & 0xffff_cc00 == 0x4e28_4800 {
            // AES: opcode 001xx.
            match opc & 3 {
                0 => aes_round(d, n, false),
                1 => aes_round(d, n, true),
                2 => vector(mix_columns(bytes(n), false)),
                _ => vector(mix_columns(bytes(n), true)),
            }
        } else if insn & 0xffe0_8c00 == 0x5e00_0000 {
            // SHA three-register.
            match opc {
                0 => sha1_rounds(d, word(n, 0), m, choose),
                1 => sha1_rounds(d, word(n, 0), m, parity),
                2 => sha1_rounds(d, word(n, 0), m, majority),
                3 => (((n & u64::MAX as u128) << 64) | (d >> 64)) ^ d ^ m,
                4 => sha256_hash(d, n, m, true),
                5 => sha256_hash(n, d, m, false),
                6 => {
                    // SHA256SU1
                    let t0 = (m << 96) | (n >> 32);
                    let s1 = |x: u32| x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10);
                    let low = [0, 1].map(|e| s1(word(m, 2 + e)).wrapping_add(word(d, e)).wrapping_add(word(t0, e)));
                    let high = [2, 3].map(|e| s1(low[e as usize - 2]).wrapping_add(word(d, e)).wrapping_add(word(t0, e)));
                    words([low[0], low[1], high[0], high[1]])
                }
                _ => return unimpl,
            }
        } else {
            // SHA two-register.
            match opc {
                0 => word(n, 0).rotate_left(30) as u128,
                1 => {
                    // SHA1SU1
                    let t = d ^ (n >> 32);
                    let w = [0, 1, 2, 3].map(|i| word(t, i).rotate_left(1));
                    words([w[0], w[1], w[2], w[3] ^ word(t, 0).rotate_left(2)])
                }
                2 => {
                    // SHA256SU0
                    let t = (n << 96) | (d >> 32);
                    let s0 = |x: u32| x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
                    words([0, 1, 2, 3].map(|i| s0(word(t, i)).wrapping_add(word(d, i))))
                }
                _ => return unimpl,
            }
        };
        self.q[rd] = r;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sbox_matches_fips_197() {
        assert_eq!(&SBOX[..4], &[0x63, 0x7c, 0x77, 0x7b]);
        assert_eq!(SBOX[0x53], 0xed);
        assert_eq!(INV_SBOX[0xed], 0x53);
    }

    #[test]
    fn mix_columns_inverts() {
        let s: [u8; 16] = core::array::from_fn(|i| (i * 37 + 5) as u8);
        assert_eq!(mix_columns(mix_columns(s, false), true), s);
        // FIPS-197's example column: db 13 53 45 -> 8e 4d a1 bc.
        let col = mix_columns([0xdb, 0x13, 0x53, 0x45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], false);
        assert_eq!(&col[..4], &[0x8e, 0x4d, 0xa1, 0xbc]);
    }

    #[test]
    fn clmul_is_carry_less() {
        assert_eq!(clmul(0b11, 0b11), 0b101);
        assert_eq!(clmul(u64::MAX, 2), (u64::MAX as u128) << 1);
    }
}
