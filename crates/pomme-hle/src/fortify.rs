//! The fortified (`_FORTIFY_SOURCE`) memory and string functions compilers
//! emit when they know a destination's size: `__memcpy_chk(dst, src, n,
//! dstlen)` and friends. Each does what its plain function does, after
//! checking that the write fits in `dstlen` bytes; one that wouldn't aborts,
//! as Darwin's `__chk_fail` does.

use crate::{Emu, HostFn, Result, Stop};

fn chk_fail(name: &str) -> Result<()> {
    Err(Stop::Unsupported(format!("buffer overflow detected in {name} (__chk_fail): the write is longer than its destination")))
}

/// Run the plain function `name` with the same arguments.
fn plain(emu: &mut Emu, name: &'static str) -> Result<()> {
    let f = *emu.functions.get(name).ok_or_else(|| Stop::Unsupported(format!("{name} isn't implemented")))?;
    f(emu)
}

fn len(emu: &Emu, s: u64) -> Result<u64> {
    Ok(emu.p.memory.cstr_len(s)? as u64)
}

/// `__memcpy_chk`, `__memmove_chk`, `__memset_chk`: (dst, src or byte, n, dstlen).
fn mem_chk(emu: &mut Emu, name: &'static str, plain_name: &'static str) -> Result<()> {
    if emu.arg(2) > emu.arg(3) {
        return chk_fail(name);
    }
    plain(emu, plain_name)?;
    emu.ret(emu.arg(0));
    Ok(())
}

/// `__strcpy_chk(dst, src, dstlen)` and `__stpcpy_chk`: the string and its NUL must fit.
fn strcpy_chk(emu: &mut Emu) -> Result<()> {
    if len(emu, emu.arg(1))? + 1 > emu.arg(2) {
        return chk_fail("__strcpy_chk");
    }
    plain(emu, "_strcpy")?;
    emu.ret(emu.arg(0));
    Ok(())
}

fn stpcpy_chk(emu: &mut Emu) -> Result<()> {
    let n = len(emu, emu.arg(1))?;
    if n + 1 > emu.arg(2) {
        return chk_fail("__stpcpy_chk");
    }
    plain(emu, "_strcpy")?;
    emu.ret(emu.arg(0) + n);
    Ok(())
}

/// `__strncpy_chk(dst, src, n, dstlen)`: strncpy writes exactly n bytes.
fn strncpy_chk(emu: &mut Emu) -> Result<()> {
    if emu.arg(2) > emu.arg(3) {
        return chk_fail("__strncpy_chk");
    }
    plain(emu, "_strncpy")?;
    emu.ret(emu.arg(0));
    Ok(())
}

/// `__strcat_chk(dst, src, dstlen)`: both strings and the NUL must fit.
fn strcat_chk(emu: &mut Emu) -> Result<()> {
    if len(emu, emu.arg(0))? + len(emu, emu.arg(1))? + 1 > emu.arg(2) {
        return chk_fail("__strcat_chk");
    }
    plain(emu, "_strcat")?;
    emu.ret(emu.arg(0));
    Ok(())
}

/// `__strncat_chk(dst, src, n, dstlen)`: at most n of src, then the NUL.
fn strncat_chk(emu: &mut Emu) -> Result<()> {
    let added = len(emu, emu.arg(1))?.min(emu.arg(2));
    if len(emu, emu.arg(0))? + added + 1 > emu.arg(3) {
        return chk_fail("__strncat_chk");
    }
    plain(emu, "_strncat")?;
    emu.ret(emu.arg(0));
    Ok(())
}

/// `stpcpy(dst, src)`: strcpy, returning the end of the copy.
fn stpcpy(emu: &mut Emu) -> Result<()> {
    let n = len(emu, emu.arg(1))?;
    plain(emu, "_strcpy")?;
    emu.ret(emu.arg(0) + n);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("___memcpy_chk", |e| mem_chk(e, "__memcpy_chk", "_memmove")),
    ("___memmove_chk", |e| mem_chk(e, "__memmove_chk", "_memmove")),
    ("___memset_chk", |e| mem_chk(e, "__memset_chk", "_memset")),
    ("___strcpy_chk", strcpy_chk),
    ("___stpcpy_chk", stpcpy_chk),
    ("___strncpy_chk", strncpy_chk),
    ("___strcat_chk", strcat_chk),
    ("___strncat_chk", strncat_chk),
    ("_stpcpy", stpcpy),
];
