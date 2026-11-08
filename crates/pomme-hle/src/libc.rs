//! libSystem: memory, strings, process control, and blocks (libclosure).

use crate::{Emu, HostFn, Result, Stop};

fn malloc(emu: &mut Emu) -> Result<()> {
    let p = emu.alloc(emu.arg(0))?;
    emu.ret(p);
    Ok(())
}

fn calloc(emu: &mut Emu) -> Result<()> {
    let p = emu.alloc(emu.arg(0).saturating_mul(emu.arg(1)))?;
    emu.ret(p);
    Ok(())
}

fn free(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    Ok(emu.heap.free(&emu.p.memory, p)?)
}

fn realloc(emu: &mut Emu) -> Result<()> {
    let p = emu.heap.realloc(&mut emu.p.memory, emu.cpu.x[0], emu.cpu.x[1])?;
    emu.ret(p);
    Ok(())
}

fn malloc_size(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let s = if p == 0 { 0 } else { emu.heap.size(&emu.p.memory, p)? };
    emu.ret(s);
    Ok(())
}

pub(crate) fn memmove(emu: &mut Emu) -> Result<()> {
    let (dst, src, n) = (emu.arg(0), emu.arg(1), emu.arg(2) as usize);
    if n > 0 {
        let bytes = emu.p.memory.read(src, n)?.to_vec();
        emu.p.memory.write(dst, &bytes)?;
    }
    Ok(()) // returns dst, already in x0
}

fn memset(emu: &mut Emu) -> Result<()> {
    let (dst, c, n) = (emu.arg(0), emu.arg(1) as u8, emu.arg(2) as usize);
    if n > 0 {
        emu.p.memory.write(dst, &vec![c; n])?;
    }
    Ok(())
}

fn bzero(emu: &mut Emu) -> Result<()> {
    let (dst, n) = (emu.arg(0), emu.arg(1) as usize);
    if n > 0 {
        emu.p.memory.write(dst, &vec![0; n])?;
    }
    Ok(())
}

fn strlen(emu: &mut Emu) -> Result<()> {
    let s = emu.cstr(emu.arg(0))?;
    emu.ret(s.len() as u64);
    Ok(())
}

fn strcmp(emu: &mut Emu) -> Result<()> {
    let a = emu.cstr(emu.arg(0))?;
    let b = emu.cstr(emu.arg(1))?;
    emu.ret(a.as_bytes().cmp(b.as_bytes()) as i8 as i64 as u64);
    Ok(())
}

fn exit(emu: &mut Emu) -> Result<()> {
    Err(Stop::Exit(emu.arg(0) as i32))
}

fn abort(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("abort() called".into()))
}

// ---- blocks ----
//
// struct Block_layout { void *isa; int32 flags; int32 reserved; void *invoke; Block_descriptor *descriptor; ... }
// struct Block_descriptor { uint64 reserved; uint64 size; [copy helper; dispose helper;] ... }

const BLOCK_NEEDS_FREE: u32 = 1 << 24;
const BLOCK_HAS_COPY_DISPOSE: u32 = 1 << 25;
const BLOCK_IS_GLOBAL: u32 = 1 << 28;

/// `_Block_copy` / `objc_retainBlock`: move a stack block to the heap.
pub(crate) fn block_copy(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(0);
    if block == 0 {
        return Ok(());
    }
    let flags = emu.p.memory.read_u32(block + 8)?;
    if flags & (BLOCK_IS_GLOBAL | BLOCK_NEEDS_FREE) != 0 {
        return Ok(()); // global or already on the heap: copying is a retain
    }
    let desc = emu.read_u64(block + 24)?;
    let size = emu.read_u64(desc + 8)?;
    let copy = emu.alloc(size)?;
    let bytes = emu.p.memory.read(block, size as usize)?.to_vec();
    emu.p.memory.write(copy, &bytes)?;
    let malloc_cls = crate::objc::host_class(emu, "__NSMallocBlock__")?;
    emu.write_u64(copy, malloc_cls)?;
    emu.p.memory.write_u32(copy + 8, flags | BLOCK_NEEDS_FREE | 2)?; // refcount 1
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let helper = emu.read_u64(desc + 16)?;
        emu.call(helper, &[copy, block])?;
    }
    emu.ret(copy);
    Ok(())
}

/// `_Block_object_assign(void *dst, const void *obj, int flags)`: store a captured variable.
fn block_object_assign(emu: &mut Emu) -> Result<()> {
    let (dst, obj, flags) = (emu.arg(0), emu.arg(1), emu.arg(2) as u32);
    const FIELD_IS_BLOCK: u32 = 7;
    const FIELD_IS_BYREF: u32 = 8;
    let value = match flags & 0xff {
        f if f & FIELD_IS_BYREF != 0 => {
            // __block variable: leave it shared (we never move byrefs to the heap,
            // which is wrong if the defining frame returns; good enough for now).
            let forwarding = emu.read_u64(obj + 8)?;
            if forwarding == 0 { obj } else { forwarding }
        }
        FIELD_IS_BLOCK => {
            emu.cpu.x[0] = obj;
            block_copy(emu)?;
            emu.cpu.x[0]
        }
        _ => obj,
    };
    emu.write_u64(dst, value)
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_malloc", malloc),
    ("_calloc", calloc),
    ("_free", free),
    ("_realloc", realloc),
    ("_malloc_size", malloc_size),
    ("_memcpy", memmove),
    ("_memmove", memmove),
    ("_memset", memset),
    ("_bzero", bzero),
    ("_strlen", strlen),
    ("_strcmp", strcmp),
    ("_exit", exit),
    ("_abort", abort),
    ("__Block_copy", block_copy),
    ("__Block_release", noop),
    ("__Block_object_assign", block_object_assign),
    ("__Block_object_dispose", noop),
];
