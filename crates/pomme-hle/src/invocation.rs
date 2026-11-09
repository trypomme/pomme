//! NSMethodSignature and NSInvocation: Objective-C's dynamic calls.
//!
//! A signature is parsed from the method's type encoding (`v24@0:8@16`). An
//! invocation holds its arguments as raw bytes, and `-invoke` places them per
//! the arm64 Apple calling convention (AAPCS64 with Apple's variadic rules):
//! integers/pointers in x0–x7, floats/doubles in v0–v7, homogeneous float
//! aggregates (CGRect, CGPoint, ...) in consecutive v registers, other structs
//! up to 16 bytes in x registers, bigger ones by reference; what doesn't fit
//! goes on the stack. Large struct returns use x8.

use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result, Stop};

/// One parsed type: its encoding, size and alignment, and how it's passed.
#[derive(Debug, Clone)]
pub struct ArgType {
    pub encoding: String,
    pub size: u64,
    pub align: u64,
    /// Homogeneous float aggregate: (element size, count).
    pub hfa: Option<(u64, u64)>,
}

/// Skip qualifiers (const, in, out, ...) and parse one type at `s[i..]`; returns it and the next index.
fn parse_type(s: &[u8], mut i: usize) -> (ArgType, usize) {
    while i < s.len() && b"rnNoORVA".contains(&s[i]) {
        i += 1;
    }
    let start = i;
    let simple = |size: u64, _float: bool| ArgType { encoding: String::new(), size, align: size.max(1), hfa: None };
    let (mut t, next) = match s.get(i) {
        Some(b'c' | b'C' | b'B') => (simple(1, false), i + 1),
        Some(b's' | b'S') => (simple(2, false), i + 1),
        Some(b'i' | b'I') => (simple(4, false), i + 1),
        Some(b'l' | b'L') => (simple(8, false), i + 1),
        Some(b'q' | b'Q' | b'*' | b'#' | b':' | b'?') => (simple(8, false), i + 1),
        Some(b'f') => (ArgType { hfa: Some((4, 1)), ..simple(4, true) }, i + 1),
        Some(b'd') => (ArgType { hfa: Some((8, 1)), ..simple(8, true) }, i + 1),
        Some(b'v') => (simple(0, false), i + 1),
        Some(b'@') => {
            // `@?` is a block; `@"Class"` names the class.
            let mut j = i + 1;
            if s.get(j) == Some(&b'?') {
                j += 1;
            } else if s.get(j) == Some(&b'"') {
                j += 1;
                while j < s.len() && s[j] != b'"' {
                    j += 1;
                }
                j += 1;
            }
            (simple(8, false), j)
        }
        Some(b'^') => {
            let (_, j) = parse_type(s, i + 1);
            (simple(8, false), j)
        }
        Some(b'b') => {
            let mut j = i + 1;
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            (simple(4, false), j)
        }
        Some(b'[') => {
            let mut j = i + 1;
            let mut n = 0u64;
            while j < s.len() && s[j].is_ascii_digit() {
                n = n * 10 + (s[j] - b'0') as u64;
                j += 1;
            }
            let (elem, k) = parse_type(s, j);
            let t = ArgType { encoding: String::new(), size: elem.size * n, align: elem.align, hfa: None };
            (t, k + 1) // ']'
        }
        Some(b'{' | b'(') => {
            let union = s[i] == b'(';
            let close = if union { b')' } else { b'}' };
            let mut j = i + 1;
            while j < s.len() && s[j] != b'=' && s[j] != close {
                j += 1;
            }
            let mut fields = Vec::new();
            if s.get(j) == Some(&b'=') {
                j += 1;
                while j < s.len() && s[j] != close {
                    let (f, k) = parse_type(s, j);
                    fields.push(f);
                    j = k;
                }
            }
            let mut size = 0u64;
            let mut align = 1u64;
            for f in &fields {
                align = align.max(f.align);
                if union {
                    size = size.max(f.size);
                } else {
                    size = size.div_ceil(f.align.max(1)) * f.align.max(1) + f.size;
                }
            }
            size = size.div_ceil(align) * align;
            // HFA: 1–4 members, all the same float type (nested HFAs flattened).
            let mut elems = Vec::new();
            let mut ok = !fields.is_empty();
            for f in &fields {
                match f.hfa {
                    Some((esize, count)) => (0..count).for_each(|_| elems.push(esize)),
                    None => ok = false,
                }
            }
            let hfa = (ok && (1..=4).contains(&elems.len()) && elems.iter().all(|&e| e == elems[0])).then(|| (elems[0], elems.len() as u64));
            (ArgType { encoding: String::new(), size, align, hfa }, j + 1)
        }
        _ => (simple(8, false), i + 1),
    };
    t.encoding = String::from_utf8_lossy(&s[start..next.min(s.len())]).into_owned();
    // Skip the frame offset digits that follow each type in method encodings.
    let mut j = next;
    while j < s.len() && (s[j].is_ascii_digit() || s[j] == b'-') {
        j += 1;
    }
    (t, j)
}

/// Parse a method type encoding into (return type, argument types).
pub fn parse_signature(types: &str) -> (ArgType, Vec<ArgType>) {
    let s = types.as_bytes();
    let (ret, mut i) = parse_type(s, 0);
    let mut args = Vec::new();
    while i < s.len() {
        let (a, j) = parse_type(s, i);
        if j <= i {
            break;
        }
        args.push(a);
        i = j;
    }
    (ret, args)
}

// ---- NSMethodSignature {isa, const char *types} ----

fn signature_types(emu: &Emu, sig: u64) -> Result<String> {
    emu.cstr(emu.read_u64(sig + 8)?)
}

fn new_signature(emu: &mut Emu, types: &str) -> Result<u64> {
    let cls = host_class(emu, "NSMethodSignature")?;
    let sig = objc::create_instance(emu, cls, 0)?;
    let t = emu.alloc_cstr(types)?;
    emu.write_u64(sig + 8, t)?;
    Ok(sig)
}

/// The type encoding of the method `sel` for class `cls` (instance methods).
fn method_types(emu: &mut Emu, cls: u64, sel: u64) -> Result<Option<String>> {
    if objc::lookup(emu, cls, sel)?.is_none() {
        return Ok(None);
    }
    Ok(objc::method_type_encoding(emu, sel).map(|t| t.to_string()))
}

fn ret_signature_for(emu: &mut Emu, cls: u64) -> Result<()> {
    let sel = emu.arg(2);
    let sig = match method_types(emu, cls, sel)? {
        Some(t) => new_signature(emu, &t)?,
        None => 0,
    };
    emu.ret(sig);
    Ok(())
}

/// `-methodSignatureForSelector:`
fn method_signature_for_selector(emu: &mut Emu) -> Result<()> {
    let cls = objc::class_of(emu, emu.arg(0))?;
    ret_signature_for(emu, cls)
}

/// `+instanceMethodSignatureForSelector:`
fn instance_method_signature_for_selector(emu: &mut Emu) -> Result<()> {
    ret_signature_for(emu, emu.arg(0))
}

/// `+signatureWithObjCTypes:`
fn signature_with_types(emu: &mut Emu) -> Result<()> {
    let t = emu.cstr(emu.arg(2))?;
    let sig = new_signature(emu, &t)?;
    emu.ret(sig);
    Ok(())
}

fn sig_parse(emu: &Emu, sig: u64) -> Result<(ArgType, Vec<ArgType>)> {
    Ok(parse_signature(&signature_types(emu, sig)?))
}

fn number_of_arguments(emu: &mut Emu) -> Result<()> {
    let (_, args) = sig_parse(emu, emu.arg(0))?;
    emu.ret(args.len() as u64);
    Ok(())
}

/// `-getArgumentTypeAtIndex:` → const char * (the encoding, without offsets)
fn argument_type_at(emu: &mut Emu) -> Result<()> {
    let (_, args) = sig_parse(emu, emu.arg(0))?;
    let i = emu.arg(2) as usize;
    let enc = args.get(i).map(|a| a.encoding.clone()).ok_or_else(|| Stop::Unsupported(format!("getArgumentTypeAtIndex: {i} out of range")))?;
    let p = emu.alloc_cstr(&enc)?;
    emu.ret(p);
    Ok(())
}

fn method_return_type(emu: &mut Emu) -> Result<()> {
    let (ret, _) = sig_parse(emu, emu.arg(0))?;
    let p = emu.alloc_cstr(&ret.encoding)?;
    emu.ret(p);
    Ok(())
}

fn method_return_length(emu: &mut Emu) -> Result<()> {
    let (ret, _) = sig_parse(emu, emu.arg(0))?;
    emu.ret(ret.size);
    Ok(())
}

fn frame_length(emu: &mut Emu) -> Result<()> {
    let (_, args) = sig_parse(emu, emu.arg(0))?;
    emu.ret(args.iter().map(|a| a.size.div_ceil(8) * 8).sum());
    Ok(())
}

// ---- NSInvocation {isa, signature, argument buffer, return buffer} ----
// Argument i lives at buffer + i * 32 (32 bytes covers CGRect, the largest
// common by-value argument); larger structs are kept by reference.

const SLOT: u64 = 32;

fn invocation_with_signature(emu: &mut Emu) -> Result<()> {
    let sig = emu.arg(2);
    let (ret, args) = sig_parse(emu, sig)?;
    let cls = host_class(emu, "NSInvocation")?;
    let inv = objc::create_instance(emu, cls, 0)?;
    let buf = emu.alloc((args.len() as u64).max(2) * SLOT)?;
    let rbuf = emu.alloc(ret.size.max(SLOT))?;
    emu.write_u64(inv + 8, sig)?;
    emu.write_u64(inv + 16, buf)?;
    emu.write_u64(inv + 24, rbuf)?;
    emu.ret(inv);
    Ok(())
}

fn inv_parts(emu: &Emu, inv: u64) -> Result<(u64, u64, u64)> {
    Ok((emu.read_u64(inv + 8)?, emu.read_u64(inv + 16)?, emu.read_u64(inv + 24)?))
}

/// `-setArgument:(void *)value atIndex:(NSInteger)i` — copies the value's bytes.
fn set_argument(emu: &mut Emu) -> Result<()> {
    let (inv, src, i) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let (sig, buf, _) = inv_parts(emu, inv)?;
    let (_, args) = sig_parse(emu, sig)?;
    let size = args.get(i as usize).map_or(8, |a| a.size).min(SLOT);
    let bytes = emu.p.memory.read(src, size as usize)?.to_vec();
    emu.p.memory.write(buf + i * SLOT, &bytes)?;
    Ok(())
}

fn get_argument(emu: &mut Emu) -> Result<()> {
    let (inv, dst, i) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let (sig, buf, _) = inv_parts(emu, inv)?;
    let (_, args) = sig_parse(emu, sig)?;
    let size = args.get(i as usize).map_or(8, |a| a.size).min(SLOT);
    let bytes = emu.p.memory.read(buf + i * SLOT, size as usize)?.to_vec();
    emu.p.memory.write(dst, &bytes)?;
    Ok(())
}

fn set_target(emu: &mut Emu) -> Result<()> {
    let (inv, t) = (emu.arg(0), emu.arg(2));
    let (_, buf, _) = inv_parts(emu, inv)?;
    emu.write_u64(buf, t)
}

fn set_selector(emu: &mut Emu) -> Result<()> {
    let (inv, s) = (emu.arg(0), emu.arg(2));
    let (_, buf, _) = inv_parts(emu, inv)?;
    emu.write_u64(buf + SLOT, s)
}

fn get_target(emu: &mut Emu) -> Result<()> {
    let (_, buf, _) = inv_parts(emu, emu.arg(0))?;
    let t = emu.read_u64(buf)?;
    emu.ret(t);
    Ok(())
}

fn get_selector(emu: &mut Emu) -> Result<()> {
    let (_, buf, _) = inv_parts(emu, emu.arg(0))?;
    let s = emu.read_u64(buf + SLOT)?;
    emu.ret(s);
    Ok(())
}

fn get_signature(emu: &mut Emu) -> Result<()> {
    let (sig, _, _) = inv_parts(emu, emu.arg(0))?;
    emu.ret(sig);
    Ok(())
}

/// `-getReturnValue:(void *)dst`
fn get_return_value(emu: &mut Emu) -> Result<()> {
    let (inv, dst) = (emu.arg(0), emu.arg(2));
    let (sig, _, rbuf) = inv_parts(emu, inv)?;
    let (ret, _) = sig_parse(emu, sig)?;
    if ret.size > 0 {
        let bytes = emu.p.memory.read(rbuf, ret.size as usize)?.to_vec();
        emu.p.memory.write(dst, &bytes)?;
    }
    Ok(())
}

fn set_return_value(emu: &mut Emu) -> Result<()> {
    let (inv, src) = (emu.arg(0), emu.arg(2));
    let (sig, _, rbuf) = inv_parts(emu, inv)?;
    let (ret, _) = sig_parse(emu, sig)?;
    if ret.size > 0 {
        let bytes = emu.p.memory.read(src, ret.size as usize)?.to_vec();
        emu.p.memory.write(rbuf, &bytes)?;
    }
    Ok(())
}

/// Lay out the arguments per the calling convention and call through objc_msgSend.
fn invoke_with(emu: &mut Emu, inv: u64, target: u64) -> Result<()> {
    let (sig, buf, rbuf) = inv_parts(emu, inv)?;
    let (ret, args) = sig_parse(emu, sig)?;
    if target == 0 {
        // Messaging nil: zero return.
        emu.p.memory.write(rbuf, &vec![0u8; ret.size.max(8) as usize])?;
        return Ok(());
    }
    let mut x: Vec<u64> = Vec::new();
    let mut v: Vec<u128> = Vec::new();
    let mut stack: Vec<u64> = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let at = buf + i as u64 * SLOT;
        let value = if i == 0 { target } else { emu.read_u64(at)? };
        if let Some((esize, count)) = a.hfa {
            // Floats, doubles and HFAs: one v register per element.
            if v.len() + count as usize <= 8 {
                for e in 0..count {
                    let bits = if esize == 4 { emu.p.memory.read_u32(at + e * 4)? as u128 } else { emu.read_u64(at + e * 8)? as u128 };
                    v.push(bits);
                }
            } else {
                for w in 0..a.size.div_ceil(8) {
                    stack.push(emu.read_u64(at + w * 8)?);
                }
            }
        } else if a.size > 16 {
            // Big structs: by reference (the slot holds a copy we own).
            let copy = emu.alloc(a.size)?;
            let bytes = emu.p.memory.read(at, a.size.min(SLOT) as usize)?.to_vec();
            emu.p.memory.write(copy, &bytes)?;
            if x.len() < 8 { x.push(copy) } else { stack.push(copy) }
        } else {
            let words = a.size.div_ceil(8).max(1) as usize;
            let mut parts = Vec::new();
            for w in 0..words {
                let mut word = emu.read_u64(at + w as u64 * 8)?;
                if a.size < 8 && words == 1 {
                    word &= (1u64 << (a.size * 8)) - 1;
                    // Sign-extend small signed integers, as the callee may expect.
                    if matches!(a.encoding.as_str(), "c" | "s" | "i") {
                        let sh = 64 - a.size * 8;
                        word = ((word << sh) as i64 >> sh) as u64;
                    }
                }
                parts.push(if i == 0 { value } else { word });
            }
            if x.len() + words <= 8 {
                x.extend(parts);
            } else {
                stack.extend(parts);
            }
        }
    }
    let send = emu.p.stubs.find("_objc_msgSend").first().copied().ok_or(Stop::Unsupported("no objc_msgSend stub".into()))?;
    let indirect = (ret.size > 16 && ret.hfa.is_none()).then_some(rbuf);
    emu.call_abi(send, &x, &v, &stack, indirect)?;
    // Store the return value.
    let (rx, rv) = (emu.last_x01, emu.last_v03);
    if let Some((esize, count)) = ret.hfa {
        for e in 0..count {
            if esize == 4 {
                emu.p.memory.write_u32(rbuf + e * 4, rv[e as usize] as u32)?;
            } else {
                emu.write_u64(rbuf + e * 8, rv[e as usize] as u64)?;
            }
        }
    } else if indirect.is_none() && ret.size > 0 {
        let bytes: Vec<u8> = rx.iter().flat_map(|w| w.to_le_bytes()).take(ret.size as usize).collect();
        emu.p.memory.write(rbuf, &bytes)?;
    }
    Ok(())
}

/// Full forwarding (`_objc_msgForward`): `recv` doesn't implement `sel`, but
/// its class implements `-forwardInvocation:`. Ask it for the method's
/// signature, capture the message's arguments from the registers (x0… and
/// v0…, then the stack) into an NSInvocation, forward it, and put its return
/// value back in x0/x1 or v0–v3. False (nothing done) when the class doesn't
/// forward or has no signature for `sel`.
pub(crate) fn forward(emu: &mut Emu, recv: u64, sel: u64) -> Result<bool> {
    let cls = objc::class_of(emu, recv)?;
    let fwd = objc::intern(emu, "forwardInvocation:")?;
    if objc::lookup(emu, cls, fwd)?.is_none() {
        return Ok(false);
    }
    let regs = (emu.cpu.x, emu.cpu.q);
    let sp = emu.cpu.sp;
    let sig = objc::send(emu, recv, "methodSignatureForSelector:", &[sel])?;
    if sig == 0 {
        return Ok(false);
    }
    let ns_inv = host_class(emu, "NSInvocation")?;
    let inv = objc::send(emu, ns_inv, "invocationWithMethodSignature:", &[sig])?;
    let (_, buf, rbuf) = inv_parts(emu, inv)?;
    let (ret, args) = sig_parse(emu, sig)?;
    let (x, q) = regs;
    let (mut xi, mut vi, mut stack) = (0usize, 0usize, sp);
    for (i, a) in args.iter().enumerate() {
        let at = buf + i as u64 * SLOT;
        if let Some((esize, count)) = a.hfa {
            if vi + count as usize <= 8 {
                for e in 0..count {
                    let bits = q[vi] as u64;
                    if esize == 4 { emu.p.memory.write_u32(at + e * 4, bits as u32)? } else { emu.write_u64(at + e * 8, bits)? }
                    vi += 1;
                }
                continue;
            }
            vi = 8;
            let bytes = emu.p.memory.read(stack, a.size as usize)?.to_vec();
            emu.p.memory.write(at, &bytes[..bytes.len().min(SLOT as usize)])?;
            stack += a.size.div_ceil(8) * 8;
        } else if a.size > 16 {
            // By reference: copy the struct in.
            let ptr = if xi < 8 { xi += 1; x[xi - 1] } else { let p = emu.read_u64(stack)?; stack += 8; p };
            let bytes = emu.p.memory.read(ptr, a.size.min(SLOT) as usize)?.to_vec();
            emu.p.memory.write(at, &bytes)?;
        } else {
            let words = a.size.div_ceil(8).max(1) as usize;
            for w in 0..words {
                let word = if xi + words - w <= 8 && xi < 8 { xi += 1; x[xi - 1] } else { xi = 8; let p = emu.read_u64(stack)?; stack += 8; p };
                emu.write_u64(at + w as u64 * 8, word)?;
            }
        }
    }
    objc::send(emu, recv, "forwardInvocation:", &[inv])?;
    // The return value.
    emu.cpu.x = x;
    emu.cpu.q = q;
    if let Some((esize, count)) = ret.hfa {
        for e in 0..count as usize {
            emu.cpu.q[e] = if esize == 4 { emu.p.memory.read_u32(rbuf + e as u64 * 4)? as u128 } else { emu.read_u64(rbuf + e as u64 * 8)? as u128 };
        }
    } else if ret.size > 0 && ret.size <= 16 {
        emu.cpu.x[0] = emu.read_u64(rbuf)?;
        if ret.size > 8 {
            emu.cpu.x[1] = emu.read_u64(rbuf + 8)?;
        }
    } else if ret.size > 16 {
        // Indirect: the caller's buffer is in x8.
        let bytes = emu.p.memory.read(rbuf, ret.size as usize)?.to_vec();
        emu.p.memory.write(x[8], &bytes)?;
    }
    Ok(true)
}

fn invoke(emu: &mut Emu) -> Result<()> {
    let inv = emu.arg(0);
    let (_, buf, _) = inv_parts(emu, inv)?;
    let target = emu.read_u64(buf)?;
    invoke_with(emu, inv, target)
}

fn invoke_with_target(emu: &mut Emu) -> Result<()> {
    let (inv, target) = (emu.arg(0), emu.arg(2));
    let (_, buf, _) = inv_parts(emu, inv)?;
    emu.write_u64(buf, target)?;
    invoke_with(emu, inv, target)
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSObject", false, "methodSignatureForSelector:", method_signature_for_selector),
    ("NSObject", true, "methodSignatureForSelector:", method_signature_for_selector),
    ("NSObject", true, "instanceMethodSignatureForSelector:", instance_method_signature_for_selector),
    ("NSMethodSignature", true, "signatureWithObjCTypes:", signature_with_types),
    ("NSMethodSignature", false, "numberOfArguments", number_of_arguments),
    ("NSMethodSignature", false, "getArgumentTypeAtIndex:", argument_type_at),
    ("NSMethodSignature", false, "methodReturnType", method_return_type),
    ("NSMethodSignature", false, "methodReturnLength", method_return_length),
    ("NSMethodSignature", false, "frameLength", frame_length),
    ("NSMethodSignature", false, "isOneway", |e| {
        e.ret(0);
        Ok(())
    }),
    ("NSInvocation", true, "invocationWithMethodSignature:", invocation_with_signature),
    ("NSInvocation", false, "setArgument:atIndex:", set_argument),
    ("NSInvocation", false, "getArgument:atIndex:", get_argument),
    ("NSInvocation", false, "setTarget:", set_target),
    ("NSInvocation", false, "target", get_target),
    ("NSInvocation", false, "setSelector:", set_selector),
    ("NSInvocation", false, "selector", get_selector),
    ("NSInvocation", false, "methodSignature", get_signature),
    ("NSInvocation", false, "getReturnValue:", get_return_value),
    ("NSInvocation", false, "setReturnValue:", set_return_value),
    ("NSInvocation", false, "invoke", invoke),
    ("NSInvocation", false, "invokeWithTarget:", invoke_with_target),
    ("NSInvocation", false, "retainArguments", noop),
    ("NSInvocation", false, "argumentsRetained", |e| {
        e.ret(1);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_encodings() {
        let (ret, args) = parse_signature("v24@0:8@16");
        assert_eq!(ret.size, 0);
        assert_eq!(args.len(), 3);
        let (ret, args) = parse_signature("{CGRect={CGPoint=dd}{CGSize=dd}}16@0:8");
        assert_eq!(ret.size, 32);
        assert_eq!(ret.hfa, Some((8, 4)));
        assert_eq!(args.len(), 2);
        let (_, args) = parse_signature(r#"v40@0:8@"NSString"16q24@?32"#);
        assert_eq!(args.iter().map(|a| a.size).collect::<Vec<_>>(), vec![8, 8, 8, 8, 8]);
        let (_, args) = parse_signature("v20@0:8f16");
        assert_eq!(args[2].hfa, Some((4, 1)));
        let (_, args) = parse_signature("v32@0:8{_NSRange=QQ}16");
        assert_eq!(args[2].size, 16);
        assert_eq!(args[2].hfa, None);
    }
}
