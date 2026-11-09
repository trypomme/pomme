//! NSValue: boxed pointers, non-retained objects, geometry structs and
//! arbitrary `@encode`d values (`valueWithBytes:objCType:`).
//! Instances are {isa, kind, four 8-byte words}; for bytes, word 0 is the
//! type encoding (a C string) and words 1–3 hold up to 24 bytes.

use crate::objc::host_class;
use crate::{Emu, HostFn, Result};

const POINTER: u64 = 1;
const OBJECT: u64 = 2;
const POINT: u64 = 3;
const SIZE: u64 = 4;
const RECT: u64 = 5;
const BYTES: u64 = 6;
/// A CATransform3D (16 doubles, kept in a buffer of their own: word 0 points to it).
const TRANSFORM3D: u64 = 7;
/// A CGAffineTransform (6 doubles, likewise).
const AFFINE: u64 = 8;

/// The largest `@encode`d value stored inline.
const MAX_BYTES: usize = 24;

/// Size of a value with Objective-C type encoding `enc` (natural alignment).
fn encoded_size(enc: &[u8]) -> Option<usize> {
    fn one(e: &[u8], i: &mut usize) -> Option<(usize, usize)> {
        // Skip qualifiers (const, in/out, ...).
        while *i < e.len() && b"rnNoORV".contains(&e[*i]) {
            *i += 1;
        }
        let c = *e.get(*i)?;
        *i += 1;
        Some(match c {
            b'c' | b'C' | b'B' => (1, 1),
            b's' | b'S' => (2, 2),
            b'i' | b'I' | b'f' => (4, 4),
            b'l' | b'L' | b'q' | b'Q' | b'd' | b'*' | b'@' | b'#' | b':' => {
                // `@"Class"` and `@?` (blocks) carry extra characters.
                if c == b'@' && e.get(*i) == Some(&b'"') {
                    *i += 1 + e[*i + 1..].iter().position(|&b| b == b'"')? + 1;
                } else if c == b'@' && e.get(*i) == Some(&b'?') {
                    *i += 1;
                }
                (8, 8)
            }
            b'^' => {
                one(e, i)?;
                (8, 8)
            }
            b'[' => {
                let digits = e[*i..].iter().take_while(|b| b.is_ascii_digit()).count();
                let n: usize = std::str::from_utf8(&e[*i..*i + digits]).ok()?.parse().ok()?;
                *i += digits;
                let (size, align) = one(e, i)?;
                (e.get(*i) == Some(&b']')).then_some(())?;
                *i += 1;
                (size * n, align)
            }
            b'{' | b'(' => {
                let close = if c == b'{' { b'}' } else { b')' };
                // Skip the name up to '=' (or an opaque `{name}`).
                while *i < e.len() && e[*i] != b'=' && e[*i] != close {
                    *i += 1;
                }
                let (mut size, mut align) = (0usize, 1usize);
                if e.get(*i) == Some(&b'=') {
                    *i += 1;
                    while *i < e.len() && e[*i] != close {
                        let (fs, fa) = one(e, i)?;
                        align = align.max(fa);
                        size = if c == b'{' { size.next_multiple_of(fa) + fs } else { size.max(fs) };
                    }
                }
                *i += 1;
                (size.next_multiple_of(align), align)
            }
            _ => return None,
        })
    }
    let mut i = 0;
    let (size, _) = one(enc, &mut i)?;
    Some(size)
}

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

/// A value as Core Animation animates it, if it's a geometry or transform value.
pub(crate) fn animatable(emu: &mut Emu, v: u64) -> Result<Option<crate::animation::Val>> {
    use crate::animation::Val;
    let cls = host_class(emu, "NSValue")?;
    if !crate::objc::is_kind_of(emu, v, cls)? {
        return Ok(None);
    }
    let (kind, w) = words(emu, v)?;
    let f = |i: usize| f64::from_bits(w[i]);
    Ok(match kind {
        POINT | SIZE => Some(Val::P([f(0), f(1)])),
        RECT => Some(Val::R([f(0), f(1), f(2), f(3)])),
        TRANSFORM3D => Some(Val::M(read_doubles::<16>(emu, w[0])?)),
        AFFINE => Some(Val::M(crate::calayer::transform_of_affine(read_doubles::<6>(emu, w[0])?))),
        _ => None,
    })
}

fn read_doubles<const N: usize>(emu: &Emu, p: u64) -> Result<[f64; N]> {
    let mut out = [0.0; N];
    for (i, x) in out.iter_mut().enumerate() {
        *x = f64::from_bits(emu.read_u64(p + i as u64 * 8)?);
    }
    Ok(out)
}

/// An NSValue holding a struct of `n` doubles at `src` (a transform), in its own buffer.
fn new_struct_value(emu: &mut Emu, kind: u64, src: u64, n: u64) -> Result<u64> {
    let buf = emu.alloc(n * 8)?;
    for i in 0..n {
        let x = emu.read_u64(src + i * 8)?;
        emu.write_u64(buf + i * 8, x)?;
    }
    new_value(emu, kind, &[buf])
}

/// Copy a struct value's doubles out to `dst` (an indirect result, x8).
fn copy_struct_out(emu: &mut Emu, v: u64, n: u64) -> Result<()> {
    let (_, w) = words(emu, v)?;
    let dst = emu.cpu.x[8];
    for i in 0..n {
        let x = emu.read_u64(w[0] + i * 8)?;
        emu.write_u64(dst + i * 8, x)?;
    }
    Ok(())
}

/// An NSValue holding a CGPoint.
pub(crate) fn new_point_value(emu: &mut Emu, p: [f64; 2]) -> Result<u64> {
    new_value(emu, POINT, &p.map(f64::to_bits))
}

/// An NSValue holding a CGRect.
pub(crate) fn new_rect_value(emu: &mut Emu, r: [f64; 4]) -> Result<u64> {
    new_value(emu, RECT, &r.map(f64::to_bits))
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

/// `valueWithBytes:objCType:` / `initWithBytes:objCType:`.
fn with_bytes(emu: &mut Emu) -> Result<()> {
    let (bytes, ty) = (emu.arg(2), emu.arg(3));
    let enc = emu.cstr(ty)?;
    let size = encoded_size(enc.as_bytes()).ok_or_else(|| crate::Stop::Unsupported(format!("NSValue: can't size @encode type {enc}")))?;
    if size > MAX_BYTES {
        return Err(crate::Stop::Unsupported(format!("NSValue: {enc} is {size} bytes (pomme stores up to {MAX_BYTES})")));
    }
    let mut buf = [0u8; MAX_BYTES];
    buf[..size].copy_from_slice(emu.p.memory.read(bytes, size)?);
    let ty = emu.alloc_cstr(&enc)?;
    let mut words = vec![ty];
    words.extend(buf.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())));
    ret_new(emu, BYTES, &words)
}

/// `getValue:` / `getValue:size:`: copy the stored value out.
fn get_value(emu: &mut Emu) -> Result<()> {
    let (v, out) = (emu.arg(0), emu.arg(2));
    let (kind, w) = words(emu, v)?;
    let (data, size): (Vec<u8>, usize) = match kind {
        BYTES => {
            let enc = emu.cstr(w[0])?;
            (w[1..].iter().flat_map(|x| x.to_le_bytes()).collect(), encoded_size(enc.as_bytes()).unwrap_or(0))
        }
        RECT => (w.iter().flat_map(|x| x.to_le_bytes()).collect(), 32),
        POINT | SIZE => (w.iter().flat_map(|x| x.to_le_bytes()).collect(), 16),
        _ => (w[0].to_le_bytes().to_vec(), 8),
    };
    emu.p.memory.write(out, &data[..size])?;
    Ok(())
}

fn objc_type(emu: &mut Emu) -> Result<()> {
    let (kind, w) = words(emu, emu.arg(0))?;
    let ty = match kind {
        BYTES => w[0],
        _ => {
            let enc = match kind {
                POINT => "{CGPoint=dd}",
                SIZE => "{CGSize=dd}",
                RECT => "{CGRect={CGPoint=dd}{CGSize=dd}}",
                OBJECT => "@",
                _ => "^v",
            };
            emu.alloc_cstr(enc)?
        }
    };
    emu.ret(ty);
    Ok(())
}

/// Kind and contents for comparing: byte values compare by type string and bytes.
fn identity(emu: &Emu, v: u64) -> Result<(u64, String, [u64; 4])> {
    let (kind, mut w) = words(emu, v)?;
    let ty = if kind == BYTES { emu.cstr(std::mem::take(&mut w[0]))? } else { String::new() };
    Ok((kind, ty, w))
}

fn first_word(emu: &mut Emu) -> Result<()> {
    let (_, w) = words(emu, emu.arg(0))?;
    emu.ret(w[0]);
    Ok(())
}

fn is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let eq = a == b || (b != 0 && emu.read_u64(b).ok() == emu.read_u64(a).ok() && identity(emu, a)? == identity(emu, b)?);
    emu.ret(eq as u64);
    Ok(())
}

fn hash(emu: &mut Emu) -> Result<()> {
    let (k, _, w) = identity(emu, emu.arg(0))?;
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
    (V, true, "valueWithBytes:objCType:", with_bytes),
    // CATransform3D is 128 bytes and CGAffineTransform 48: both passed by reference (x2), returned through x8.
    (V, true, "valueWithCATransform3D:", |e| {
        let v = new_struct_value(e, TRANSFORM3D, e.arg(2), 16)?;
        e.ret(v);
        Ok(())
    }),
    (V, false, "CATransform3DValue", |e| copy_struct_out(e, e.arg(0), 16)),
    (V, true, "valueWithCGAffineTransform:", |e| {
        let v = new_struct_value(e, AFFINE, e.arg(2), 6)?;
        e.ret(v);
        Ok(())
    }),
    (V, false, "CGAffineTransformValue", |e| copy_struct_out(e, e.arg(0), 6)),
    (V, true, "value:withObjCType:", with_bytes),
    (V, false, "initWithBytes:objCType:", |e| {
        // Replace the alloc'd instance with a filled one (the alloc is leaked).
        with_bytes(e)
    }),
    (V, false, "getValue:", get_value),
    (V, false, "getValue:size:", get_value),
    (V, false, "objCType", objc_type),
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

#[cfg(test)]
mod tests {
    use super::encoded_size;

    #[test]
    fn sizes_of_encodings() {
        let size = |e: &str| encoded_size(e.as_bytes());
        assert_eq!(size("d"), Some(8));
        assert_eq!(size("{CacheKey=dd}"), Some(16));
        assert_eq!(size("{CGRect={CGPoint=dd}{CGSize=dd}}"), Some(32));
        assert_eq!(size("{Mixed=cid}"), Some(16)); // char, pad, int, double
        assert_eq!(size("[4i]"), Some(16));
        assert_eq!(size("^{opaque}"), Some(8));
        assert_eq!(size("@\"NSString\""), Some(8));
        assert_eq!(size("(U=ci)"), Some(4));
        assert_eq!(size("{_NSRange=QQ}"), Some(16));
        assert_eq!(size("?"), None);
    }
}
