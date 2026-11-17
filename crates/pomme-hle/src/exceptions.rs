//! Objective-C exceptions, as objc4 does them: a throw is a C++ exception
//! (libc++abi's `__cxa_throw`, real code in the sysroot) whose type info is
//! Objective-C's own, and libunwind finds the `@catch` that takes it. Each
//! `@catch (SomeClass *e)` names a type info `{vtable, name, class}`
//! (`OBJC_EHTYPE_$_SomeClass`; a catch of `id` has no class); libc++abi asks
//! it `can_catch` through its vtable, and the answer is whether the thrown
//! object is of that class.
//!
//! An exception nobody catches ends in std::terminate, where (as objc4's
//! terminate handler does) the app's uncaught-exception handler hears it
//! before the run stops.

use crate::objc::{self, send};
use crate::{Emu, Result, Stop};

#[derive(Clone, Default)]
pub(crate) struct State {
    /// `objc_ehtype_vtable` (0: libc++abi isn't loaded, so throws stop the run).
    vtable: u64,
    /// The terminate handler before ours (0: ours isn't installed yet).
    old_terminate: u64,
}

fn abi(emu: &Emu, name: &str) -> Option<u64> {
    emu.p.images.iter().find_map(|i| i.symbol(name))
}

fn abi_or_stop(emu: &Emu, name: &str) -> Result<u64> {
    abi(emu, name).ok_or_else(|| Stop::Unsupported(format!("Objective-C exceptions need libc++abi's {name} (run scripts/build-sysroot.sh)")))
}

/// Give `objc_ehtype_vtable`, `OBJC_EHTYPE_id` and the system classes'
/// `OBJC_EHTYPE_$_X` real memory.
pub(crate) fn bind(emu: &mut Emu) -> Result<()> {
    if abi(emu, "___cxa_throw").is_none() {
        return Ok(());
    }
    // [offset to top, type info, then libc++abi's __shim_type_info virtuals:
    // two destructors, noop1, noop2, can_catch, search_above_dst,
    // search_below_dst, has_unambiguous_public_base].
    let vtable = emu.alloc(12 * 8)?;
    let noop = emu.host_thunk(|_| Ok(()))?;
    let no = emu.host_thunk(|e| {
        e.ret(0);
        Ok(())
    })?;
    let can_catch = emu.host_thunk(can_catch)?;
    for (i, f) in [noop, noop, noop, noop, can_catch, no, no, no].into_iter().enumerate() {
        emu.write_u64(vtable + 16 + i as u64 * 8, f)?;
    }
    emu.exceptions.vtable = vtable;
    let id = type_info(emu, "id", 0)?;
    emu.write_u64(vtable + 8, id)?;
    for stub in emu.p.stubs.find("_objc_ehtype_vtable") {
        emu.p.rebind(stub, vtable).map_err(|e| Stop::Unsupported(e.to_string()))?;
    }
    for stub in emu.p.stubs.find("_OBJC_EHTYPE_id") {
        emu.p.rebind(stub, id).map_err(|e| Stop::Unsupported(e.to_string()))?;
    }
    let names: Vec<String> = emu.p.stubs.iter().filter_map(|(_, s)| s.symbol.strip_prefix("_OBJC_EHTYPE_$_").map(str::to_string)).collect();
    for name in names {
        let cls = emu.objc.by_name.get(&name).copied().unwrap_or(0);
        if cls == 0 {
            continue; // a class pomme doesn't have: nothing of it is ever thrown
        }
        let t = type_info(emu, &name, cls)?;
        for stub in emu.p.stubs.find(&format!("_OBJC_EHTYPE_$_{name}")) {
            emu.p.rebind(stub, t).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
    }
    Ok(())
}

/// Whether a thrown type info is Objective-C's.
pub(crate) fn is_objc(emu: &Emu, tinfo: u64) -> bool {
    emu.exceptions.vtable != 0 && emu.p.memory.read_u64(tinfo).ok() == Some(emu.exceptions.vtable + 16)
}

/// objc_typeinfo {vtable (past its two header words), name, class}.
fn type_info(emu: &mut Emu, name: &str, cls: u64) -> Result<u64> {
    let t = emu.alloc(24)?;
    let n = emu.alloc_cstr(name)?;
    emu.write_u64(t, emu.exceptions.vtable + 16)?;
    emu.write_u64(t + 8, n)?;
    emu.write_u64(t + 16, cls)?;
    Ok(t)
}

/// `can_catch(this = the catch's type info, thrown type, void *&adjusted)`:
/// an Objective-C exception, of the catch's class (any, for `id`). The
/// thrown object is objc4's `{id obj; objc_typeinfo tinfo}`.
fn can_catch(emu: &mut Emu) -> Result<()> {
    let (catch, thrown, adjusted) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let ok = (|| -> Result<bool> {
        if !is_objc(emu, thrown) {
            return Ok(false);
        }
        let cls = emu.read_u64(catch + 16)?;
        if cls == 0 {
            return Ok(true);
        }
        let obj = emu.read_u64(emu.read_u64(adjusted)?)?;
        objc::is_kind_of(emu, obj, cls)
    })()?;
    emu.ret(ok as u64);
    Ok(())
}

/// `objc_exception_throw(obj)`, tail-calling `__cxa_throw` so the unwinder
/// starts from the thrower's frame.
pub(crate) fn throw(emu: &mut Emu) -> Result<()> {
    let obj = emu.arg(0);
    if emu.exceptions.vtable == 0 {
        return Err(uncaught(emu, obj));
    }
    if emu.exceptions.old_terminate == 0 {
        let set = abi_or_stop(emu, "__ZSt13set_terminatePFvvE")?;
        let ours = emu.host_thunk(terminate)?;
        emu.exceptions.old_terminate = emu.call(set, &[ours])?;
    }
    let alloc = abi_or_stop(emu, "___cxa_allocate_exception")?;
    let exc = emu.call(alloc, &[32])?;
    objc::objc_retain_obj(emu, obj)?;
    let cls = objc::class_of(emu, obj)?;
    let name = emu.objc.class_name(cls);
    let name = emu.alloc_cstr(&name)?;
    // {obj, its type info {vtable, name, class}}, as objc4's.
    emu.write_u64(exc, obj)?;
    emu.write_u64(exc + 8, emu.exceptions.vtable + 16)?;
    emu.write_u64(exc + 16, name)?;
    emu.write_u64(exc + 24, cls)?;
    emu.cpu.x[0] = exc;
    emu.cpu.x[1] = exc + 8;
    emu.cpu.x[2] = 0;
    emu.cpu.pc = abi_or_stop(emu, "___cxa_throw")?;
    Ok(())
}

/// `__objc_personality_v0`, the unwinder's question for frames with
/// `@catch`: libc++abi's, as objc4's is.
pub(crate) fn personality(emu: &mut Emu) -> Result<()> {
    emu.cpu.pc = abi_or_stop(emu, "___gxx_personality_v0")?;
    Ok(())
}

/// `objc_begin_catch(buf)`: the caught object.
pub(crate) fn begin_catch(emu: &mut Emu) -> Result<()> {
    let f = abi_or_stop(emu, "___cxa_begin_catch")?;
    let p = emu.call(f, &[emu.arg(0)])?;
    let obj = emu.read_u64(p)?;
    emu.ret(obj);
    Ok(())
}

pub(crate) fn end_catch(emu: &mut Emu) -> Result<()> {
    emu.cpu.pc = abi_or_stop(emu, "___cxa_end_catch")?;
    Ok(())
}

pub(crate) fn rethrow(emu: &mut Emu) -> Result<()> {
    emu.cpu.pc = abi_or_stop(emu, "___cxa_rethrow")?;
    Ok(())
}

/// std::terminate's handler: for an Objective-C exception, the app's
/// uncaught-exception handler, then the run stops naming it; anything else
/// goes to the previous handler.
fn terminate(emu: &mut Emu) -> Result<()> {
    let current = abi_or_stop(emu, "___cxa_current_exception_type")?;
    let t = emu.call(current, &[])?;
    if t != 0 && is_objc(emu, t) {
        let obj = emu.read_u64(t - 8)?;
        return Err(uncaught(emu, obj));
    }
    emu.cpu.pc = emu.exceptions.old_terminate;
    Ok(())
}

/// The app's uncaught-exception handler hears it (crash reporters record it
/// there), and the run stops.
fn uncaught(emu: &mut Emu, exc: u64) -> Stop {
    let handler = std::mem::take(&mut emu.uncaught_exception_handler);
    if handler != 0
        && let Err(e) = emu.call(handler, &[exc])
    {
        return e;
    }
    let mut text = |sel: &str| send(emu, exc, sel, &[]).ok().filter(|&s| s != 0).and_then(|s| crate::foundation::nsstring(emu, s).ok());
    let (name, reason) = (text("name"), text("reason"));
    Stop::Unsupported(format!("uncaught Objective-C exception {}: {}", name.unwrap_or_default(), reason.unwrap_or_default()))
}

/// `objc_terminate()`: std::terminate.
pub(crate) fn objc_terminate(emu: &mut Emu) -> Result<()> {
    emu.cpu.pc = abi_or_stop(emu, "__ZSt9terminatev")?;
    Ok(())
}
