//! NSValue: boxed pointers, non-retained objects and geometry structs.
//! Instances are {isa, kind, four 8-byte words}.

use crate::objc::host_class;
use crate::{Emu, HostFn, Result};

const POINTER: u64 = 1;
const OBJECT: u64 = 2;
const POINT: u64 = 3;
const SIZE: u64 = 4;
const RECT: u64 = 5;

fn new_value(emu: &mut Emu, kind: u64, words: &[u64]) -> Result<u64> {
    let cls = host_class(emu, "NSValue")?;
    let v = crate::objc::create_instance(emu, cls, 0)?;
    emu.write_u64(v + 8, kind)?;
    for (i, w) in words.iter().enumerate() {
        emu.write_u64(v + 16 + i as u64 * 8, *w)?;
    }
    Ok(v)
}

fn words(emu: &Emu, v: u64) -> Result<(u64, [u64; 4])> {
    let kind = emu.read_u64(v + 8)?;
    let mut w = [0u64; 4];
    for (i, x) in w.iter_mut().enumerate() {
        *x = emu.read_u64(v + 16 + i as u64 * 8)?;
    }
    Ok((kind, w))
}

fn ret_new(emu: &mut Emu, kind: u64, w: &[u64]) -> Result<()> {
    let v = new_value(emu, kind, w)?;
    emu.ret(v);
    Ok(())
}

/// Floats from d0.. as raw bits.
fn floats(emu: &Emu, n: usize) -> Vec<u64> {
    (0..n).map(|i| emu.cpu.q[i] as u64).collect()
}

fn ret_floats(emu: &mut Emu, n: usize) -> Result<()> {
    let (_, w) = words(emu, emu.arg(0))?;
    for (i, x) in w.iter().take(n).enumerate() {
        emu.cpu.q[i] = *x as u128;
    }
    Ok(())
}

fn first_word(emu: &mut Emu) -> Result<()> {
    let (_, w) = words(emu, emu.arg(0))?;
    emu.ret(w[0]);
    Ok(())
}

fn is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let eq = a == b || (b != 0 && emu.read_u64(b).ok() == emu.read_u64(a).ok() && words(emu, a)? == words(emu, b)?);
    emu.ret(eq as u64);
    Ok(())
}

fn hash(emu: &mut Emu) -> Result<()> {
    let (k, w) = words(emu, emu.arg(0))?;
    let h = w.iter().fold(k, |h, x| h.rotate_left(13) ^ x);
    emu.ret(h);
    Ok(())
}

const V: &str = "NSValue";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (V, true, "valueWithPointer:", |e| {
        let p = e.arg(2);
        ret_new(e, POINTER, &[p])
    }),
    (V, true, "valueWithNonretainedObject:", |e| {
        let o = e.arg(2);
        ret_new(e, OBJECT, &[o])
    }),
    (V, true, "valueWithCGPoint:", |e| {
        let w = floats(e, 2);
        ret_new(e, POINT, &w)
    }),
    (V, true, "valueWithCGSize:", |e| {
        let w = floats(e, 2);
        ret_new(e, SIZE, &w)
    }),
    (V, true, "valueWithCGRect:", |e| {
        let w = floats(e, 4);
        ret_new(e, RECT, &w)
    }),
    (V, false, "pointerValue", first_word),
    (V, false, "nonretainedObjectValue", first_word),
    (V, false, "CGPointValue", |e| ret_floats(e, 2)),
    (V, false, "CGSizeValue", |e| ret_floats(e, 2)),
    (V, false, "CGRectValue", |e| ret_floats(e, 4)),
    (V, false, "isEqual:", is_equal),
    (V, false, "isEqualToValue:", is_equal),
    (V, false, "hash", hash),
    (V, false, "copy", |_| Ok(())),
    (V, false, "copyWithZone:", |_| Ok(())),
];
