//! The C library's seeded random numbers, as Darwin's libc (from FreeBSD)
//! computes them, so a seeded sequence is the one a phone gives:
//! - `rand`, `srand`, `rand_r`: the "minimal standard" generator (×16807 mod 2³¹−1).
//! - `random`, `srandom`: the additive feedback generator over 31 words,
//!   seeded by the minimal standard one; it starts as if `srandom(1)`.
//! - `drand48` and its family: the 48-bit linear congruential generator
//!   X' = (aX + c) mod 2⁴⁸, a = 0x5DEECE66D, c = 0xB.

use crate::{Emu, HostFn, Result};

const RAND_MAX: u64 = 0x7fff_ffff;

/// One step of the minimal standard generator (Schrage's method, as libc has it).
fn good_rand(x: i64) -> i64 {
    let x = if x == 0 { 123_459_876 } else { x };
    let (hi, lo) = (x / 127_773, x % 127_773);
    let x = 16_807 * lo - 2_836 * hi;
    if x < 0 { x + 0x7fff_ffff } else { x }
}

/// `random`'s state: 31 words, the front and rear indexes.
#[derive(Clone)]
pub(crate) struct Random {
    state: [u32; 31],
    f: usize,
    r: usize,
}

impl Random {
    fn seeded(seed: u32) -> Random {
        let mut state = [0u32; 31];
        state[0] = seed;
        for i in 1..31 {
            state[i] = good_rand(state[i - 1] as i32 as i64) as u32;
        }
        let mut r = Random { state, f: 3, r: 0 };
        for _ in 0..310 {
            r.next();
        }
        r
    }

    fn next(&mut self) -> u32 {
        self.state[self.f] = self.state[self.f].wrapping_add(self.state[self.r]);
        let v = (self.state[self.f] >> 1) & 0x7fff_ffff;
        self.f = (self.f + 1) % 31;
        self.r = (self.r + 1) % 31;
        v
    }
}

/// The 48-bit generator's parameters.
#[derive(Clone, Copy)]
pub(crate) struct Lcg48 {
    x: u64,
    a: u64,
    c: u64,
}

const MASK48: u64 = (1 << 48) - 1;
const A48: u64 = 0x5_DEEC_E66D;
const C48: u64 = 0xB;

impl Default for Lcg48 {
    fn default() -> Self {
        Lcg48 { x: 0x1234_ABCD_330E, a: A48, c: C48 }
    }
}

impl Lcg48 {
    fn step(&self, x: u64) -> u64 {
        (self.a.wrapping_mul(x).wrapping_add(self.c)) & MASK48
    }
}

#[derive(Clone)]
pub(crate) struct State {
    rand: i64,
    random: Random,
    lcg: Lcg48,
    /// seed48's static buffer for the previous state.
    seed48_buf: u64,
}

impl Default for State {
    fn default() -> Self {
        State { rand: 1, random: Random::seeded(1), lcg: Lcg48::default(), seed48_buf: 0 }
    }
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_f64(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

/// A 48-bit value from three little-endian 16-bit words (`unsigned short xsubi[3]`).
fn read48(emu: &Emu, p: u64) -> Result<u64> {
    let b = emu.p.memory.read(p, 6)?;
    Ok((0..3).fold(0, |x, i| x | (u16::from_le_bytes([b[2 * i], b[2 * i + 1]]) as u64) << (16 * i)))
}

fn write48(emu: &mut Emu, p: u64, x: u64) -> Result<()> {
    let bytes: Vec<u8> = (0..3).flat_map(|i| ((x >> (16 * i)) as u16).to_le_bytes()).collect();
    Ok(emu.p.memory.write(p, &bytes)?)
}

/// Advance the caller's state at `p` (erand48 and friends) or the shared one (p = 0).
fn next48(emu: &mut Emu, p: u64) -> Result<u64> {
    let lcg = emu.rand.lcg;
    if p == 0 {
        let x = lcg.step(lcg.x);
        emu.rand.lcg.x = x;
        Ok(x)
    } else {
        let x = lcg.step(read48(emu, p)?);
        write48(emu, p, x)?;
        Ok(x)
    }
}

fn as_double(x: u64) -> f64 {
    x as f64 / (1u64 << 48) as f64
}

fn seed48(emu: &mut Emu) -> Result<()> {
    if emu.rand.seed48_buf == 0 {
        emu.rand.seed48_buf = emu.alloc(8)?;
    }
    let (buf, old) = (emu.rand.seed48_buf, emu.rand.lcg.x);
    write48(emu, buf, old)?;
    let x = read48(emu, emu.arg(0))?;
    emu.rand.lcg = Lcg48 { x, ..Lcg48::default() };
    ret(emu, buf)
}

/// `lcong48(unsigned short param[7])`: X, then a, then c.
fn lcong48(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let x = read48(emu, p)?;
    let a = read48(emu, p + 6)?;
    let c = emu.p.memory.read_u16(p + 12)? as u64;
    emu.rand.lcg = Lcg48 { x, a, c };
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_rand", |e| {
        let x = good_rand(e.rand.rand);
        e.rand.rand = x;
        ret(e, x as u64 % (RAND_MAX + 1))
    }),
    ("_srand", |e| {
        e.rand.rand = e.arg(0) as u32 as i64;
        Ok(())
    }),
    ("_rand_r", |e| {
        let p = e.arg(0);
        let x = good_rand(e.p.memory.read_u32(p)? as i64);
        e.p.memory.write_u32(p, x as u32)?;
        ret(e, x as u64 % (RAND_MAX + 1))
    }),
    ("_random", |e| {
        let v = e.rand.random.next();
        ret(e, v as u64)
    }),
    ("_srandom", |e| {
        e.rand.random = Random::seeded(e.arg(0) as u32);
        Ok(())
    }),
    ("_drand48", |e| {
        let x = next48(e, 0)?;
        ret_f64(e, as_double(x))
    }),
    ("_erand48", |e| {
        let x = next48(e, e.arg(0))?;
        ret_f64(e, as_double(x))
    }),
    ("_lrand48", |e| {
        let x = next48(e, 0)?;
        ret(e, x >> 17)
    }),
    ("_nrand48", |e| {
        let x = next48(e, e.arg(0))?;
        ret(e, x >> 17)
    }),
    ("_mrand48", |e| {
        let x = next48(e, 0)?;
        ret(e, (x >> 16) as u32 as i32 as i64 as u64)
    }),
    ("_jrand48", |e| {
        let x = next48(e, e.arg(0))?;
        ret(e, (x >> 16) as u32 as i32 as i64 as u64)
    }),
    ("_srand48", |e| {
        let seed = e.arg(0) as u32 as u64;
        e.rand.lcg = Lcg48 { x: (seed << 16) | 0x330E, ..Lcg48::default() };
        Ok(())
    }),
    ("_seed48", seed48),
    ("_lcong48", lcong48),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_standard() {
        // From seed 1: 16807, 282475249, 1622650073 (Park and Miller's sequence).
        let mut x = 1;
        let got: Vec<i64> = (0..3)
            .map(|_| {
                x = good_rand(x);
                x
            })
            .collect();
        assert_eq!(got, vec![16_807, 282_475_249, 1_622_650_073]);
    }

    #[test]
    fn random_matches_bsd() {
        // srandom(1), the initial state: glibc and FreeBSD's TYPE_3 generator agree.
        let mut r = Random::seeded(1);
        let got: Vec<u32> = (0..3).map(|_| r.next()).collect();
        assert_eq!(got, vec![1_804_289_383, 846_930_886, 1_681_692_777]);
    }

    #[test]
    fn rand48() {
        // srand48(0): X = 0x330E. macOS's libc gives 0.170828036106289716 first.
        let lcg = Lcg48 { x: 0x330E, ..Lcg48::default() };
        let x = lcg.step(lcg.x);
        assert_eq!(as_double(x), 0.170_828_036_106_289_716);
        assert_eq!(x >> 17, 366_850_414); // lrand48 after srand48(0)
    }
}
