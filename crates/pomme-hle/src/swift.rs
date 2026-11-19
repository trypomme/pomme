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

/// `Darwin.errno` (libswift_errno): the C errno, read and written.
fn errno_get(emu: &mut Emu) -> Result<()> {
    let v = crate::libsystem::errno(emu)?;
    emu.ret(v as u64);
    Ok(())
}

fn errno_set(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0) & 0xffff_ffff;
    crate::libsystem::set_errno(emu, v)
}

/// `NSNumber(integerLiteral:)` and friends: Foundation's literal initializers.
fn number_from_int(emu: &mut Emu) -> Result<()> {
    let n = foundation::new_number_i64(emu, emu.arg(0) as i64)?;
    emu.ret(n);
    Ok(())
}

fn number_from_float(emu: &mut Emu) -> Result<()> {
    let v = f64::from_bits(emu.cpu.q[0] as u64);
    let n = foundation::new_number_f64(emu, v)?;
    emu.ret(n);
    Ok(())
}

fn number_from_bool(emu: &mut Emu) -> Result<()> {
    let b = foundation::boolean(emu, emu.arg(0) & 1 != 0);
    emu.ret(b);
    Ok(())
}

/// `NSObject.observe(_:options:changeHandler:)` (Swift's typed KVO, from
/// Apple's Foundation overlay): an observation that never fires. pomme
/// doesn't notify key-value observers of view properties; keyboard
/// controller watches a view's `center` this way for interactive dismissal,
/// which scripted input doesn't do.
fn kvo_observe(emu: &mut Emu) -> Result<()> {
    // The Foundation overlay's Swift class (its invalidate() is a vtable call).
    let o = match emu.objc.by_name.get("_TtC10Foundation21NSKeyValueObservation").copied() {
        Some(cls) => {
            let a = crate::objc::send(emu, cls, "alloc", &[])?;
            crate::objc::send(emu, a, "init", &[])?
        }
        None => {
            let cls = crate::objc::host_class(emu, "NSKeyValueObservation")?;
            crate::objc::create_instance(emu, cls, 0)?
        }
    };
    emu.ret(o);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // WidgetKit: pomme has no home screen widgets, so WidgetCenter.shared is nil and asking it
    // to reload a widget's timeline does nothing (Wonderous's home_widget plugin does at launch).
    ("_$s9WidgetKit0A6CenterCMa", |e| {
        e.ret(0);
        e.cpu.x[1] = 0;
        Ok(())
    }),
    ("_$s9WidgetKit0A6CenterC6sharedACvgZ", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_$s9WidgetKit0A6CenterC15reloadTimelines6ofKindySS_tFTj", noop),
    ("_$s9WidgetKit0A6CenterC15reloadTimelines6ofKindySS_tF", noop),
    ("_$s9WidgetKit0A6CenterC20reloadAllTimelinesyyFTj", noop),
    ("_$s9WidgetKit0A6CenterC20reloadAllTimelinesyyF", noop),
    // NSKeyValueObservation.invalidate() (dispatch thunk and method): ours never fire.
    ("_$s10Foundation21NSKeyValueObservationC10invalidateyyFTj", noop),
    ("_$s10Foundation21NSKeyValueObservationC10invalidateyyF", noop),
    (
        "_$s10Foundation27_KeyValueCodingAndObservingPAAE7observe_7options13changeHandlerAA05NSKeyC11ObservationCs0B4PathCyxqd__G_So0kcF7OptionsVyx_AA0kC14ObservedChangeVyqd__GtctlF",
        kvo_observe,
    ),
    ("_$sSo8NSNumberC10FoundationE14integerLiteralABSi_tcfC", number_from_int),
    ("_$sSo8NSNumberC10FoundationE12floatLiteralABSd_tcfC", number_from_float),
    ("_$sSo8NSNumberC10FoundationE14booleanLiteralABSb_tcfC", number_from_bool),
    ("_$s6Darwin5errnos5Int32Vvg", errno_get),
    ("_$s6Darwin5errnos5Int32Vvs", errno_set),
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
