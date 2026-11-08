//! The Swift runtime and standard library entry points the app calls.
//!
//! Swift `String` is two words (x0 = count and flags, x1 = object). Much of
//! the String API is inlined into the app, so we must produce real layouts:
//!
//! - small (≤ 15 UTF-8 bytes): bytes inline; x1's top byte is 0xE0 | count
//!   (ASCII) or 0xA0 | count.
//! - large, immortal: x0 = flags | count, x1 = 0x8000… | (address of the
//!   bytes − 32). This is how string literals are emitted; we use it for every
//!   host-made string (the bytes are never freed).
//! - bridged: x1 = 0x4000… | NSString pointer.

use crate::{Emu, HostFn, Result, Stop, foundation};

const COUNT_MASK: u64 = 0x0000_ffff_ffff_ffff;
const OBJECT_MASK: u64 = 0x0fff_ffff_ffff_ffff;

/// Build a Swift String value.
pub fn make_string(emu: &mut Emu, s: &str) -> Result<(u64, u64)> {
    let b = s.as_bytes();
    let ascii = s.is_ascii();
    if b.len() <= 15 {
        let mut raw = [0u8; 16];
        raw[..b.len()].copy_from_slice(b);
        raw[15] = (if ascii { 0xe0 } else { 0xa0 }) | b.len() as u8;
        let lo = u64::from_le_bytes(raw[..8].try_into().unwrap());
        let hi = u64::from_le_bytes(raw[8..].try_into().unwrap());
        return Ok((lo, hi));
    }
    let chars = emu.alloc_cstr(s)?;
    let flags = if ascii { 0xd000_0000_0000_0000 } else { 0x1000_0000_0000_0000 };
    Ok((flags | b.len() as u64, 0x8000_0000_0000_0000 | (chars - 32)))
}

/// Read a Swift String value.
pub fn read_string(emu: &mut Emu, lo: u64, hi: u64) -> Result<String> {
    if hi & (1 << 61) != 0 {
        let count = ((hi >> 56) & 0xf) as usize;
        let mut raw = [0u8; 16];
        raw[..8].copy_from_slice(&lo.to_le_bytes());
        raw[8..].copy_from_slice(&hi.to_le_bytes());
        return Ok(String::from_utf8_lossy(&raw[..count]).into_owned());
    }
    if hi & (1 << 62) != 0 {
        return foundation::nsstring(emu, hi & OBJECT_MASK);
    }
    if hi & (1 << 60) != 0 {
        return Err(Stop::Unsupported("foreign (non-bridged) Swift string".into()));
    }
    let count = (lo & COUNT_MASK) as usize;
    let chars = (hi & OBJECT_MASK) + 32;
    Ok(String::from_utf8_lossy(emu.p.memory.read(chars, count)?).into_owned())
}

/// Read an `Optional<String>`; `.none` is both words zero.
pub fn read_optional_string(emu: &mut Emu, lo: u64, hi: u64) -> Result<Option<String>> {
    if lo == 0 && hi == 0 { Ok(None) } else { read_string(emu, lo, hi).map(Some) }
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn command_line_argc(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.argc);
    Ok(())
}

fn command_line_argv(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.argv);
    Ok(())
}

/// `static String._unconditionallyBridgeFromObjectiveC(NSString?) -> String`
fn bridge_from_nsstring(emu: &mut Emu) -> Result<()> {
    let s = foundation::nsstring(emu, emu.arg(0))?;
    let (lo, hi) = make_string(emu, &s)?;
    emu.cpu.x[0] = lo;
    emu.cpu.x[1] = hi;
    Ok(())
}

/// `String._bridgeToObjectiveC() -> NSString` (self in x0, x1)
fn bridge_to_nsstring(emu: &mut Emu) -> Result<()> {
    let s = read_string(emu, emu.cpu.x[0], emu.cpu.x[1])?;
    let r = foundation::new_nsstring(emu, &s)?;
    emu.ret(r);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_$ss11CommandLineO4argcs5Int32VvgZ", command_line_argc),
    ("_$ss11CommandLineO10unsafeArgvSpySpys4Int8VGSgGvgZ", command_line_argv),
    ("_swift_getObjCClassFromMetadata", ret_self),
    ("_swift_getObjCClassFromObject", crate::objc::object_get_class),
    ("_$sSS10FoundationE36_unconditionallyBridgeFromObjectiveCySSSo8NSStringCSgFZ", bridge_from_nsstring),
    ("_$sSS10FoundationE19_bridgeToObjectiveCSo8NSStringCyF", bridge_to_nsstring),
    ("_swift_retain", ret_self),
    ("_swift_release", noop),
    ("_swift_bridgeObjectRetain", ret_self),
    ("_swift_bridgeObjectRelease", noop),
    ("_swift_unknownObjectRetain", ret_self),
    ("_swift_unknownObjectRelease", noop),
];
