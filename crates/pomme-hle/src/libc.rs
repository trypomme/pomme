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
    if emu.watch.contains(&p) {
        let mut bt = String::new();
        let mut fp = emu.cpu.x[29];
        for _ in 0..8 {
            let (Ok(next), Ok(ret)) = (emu.p.memory.read_u64(fp), emu.p.memory.read_u64(fp + 8)) else { break };
            if ret == 0 {
                break;
            }
            bt += &format!("\n    {}", emu.symbolize(ret.wrapping_sub(4)));
            if next <= fp {
                break;
            }
            fp = next;
        }
        println!("[watch] {p:#x} freed, called from {}{bt}", emu.symbolize(emu.cpu.x[30].wrapping_sub(4)));
    }
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

fn abort(emu: &mut Emu) -> Result<()> {
    if emu.sys.unwound_into_host {
        return Err(Stop::Unsupported(
            "abort() called: a C++ exception reached one of pomme's own frames (system code that called back into the app), which it can't unwind through; its handler, if any, is beyond that frame".into(),
        ));
    }
    Err(Stop::Unsupported("abort() called".into()))
}

// ---- blocks ----
//
// struct Block_layout { void *isa; int32 flags; int32 reserved; void *invoke; Block_descriptor *descriptor; ... }
// struct Block_descriptor { uint64 reserved; uint64 size; [copy helper; dispose helper;] ... }

const BLOCK_NEEDS_FREE: u32 = 1 << 24;
const BLOCK_HAS_COPY_DISPOSE: u32 = 1 << 25;
const BLOCK_IS_GLOBAL: u32 = 1 << 28;

/// `_Block_copy` from host code.
pub(crate) fn copy_block(emu: &mut Emu, b: u64) -> Result<u64> {
    let saved = emu.cpu.x[0];
    emu.cpu.x[0] = b;
    block_copy(emu)?;
    let c = emu.cpu.x[0];
    emu.cpu.x[0] = saved;
    Ok(c)
}

/// `_Block_copy` / `objc_retainBlock`: move a stack block to the heap.
pub(crate) fn block_copy(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(0);
    if block == 0 {
        return Ok(());
    }
    let flags = emu.p.memory.read_u32(block + 8)?;
    if flags & BLOCK_IS_GLOBAL != 0 {
        return Ok(());
    }
    if flags & BLOCK_NEEDS_FREE != 0 {
        // Already on the heap: copying is a retain (refcount in bits 1–15, steps of 2).
        emu.p.memory.write_u32(block + 8, flags + 2)?;
        return Ok(());
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

/// `_Block_release(block)`: drop a reference to a heap block; at zero, run its
/// dispose helper (which releases what it captured) and free it.
pub(crate) fn block_release_obj(emu: &mut Emu, block: u64) -> Result<()> {
    if block == 0 {
        return Ok(());
    }
    let flags = emu.p.memory.read_u32(block + 8)?;
    if flags & BLOCK_NEEDS_FREE == 0 || flags & BLOCK_IS_GLOBAL != 0 {
        return Ok(()); // stack and global blocks aren't reference counted
    }
    let count = flags & 0xfffe;
    if count > 2 {
        emu.p.memory.write_u32(block + 8, flags - 2)?;
        return Ok(());
    }
    emu.p.memory.write_u32(block + 8, flags & !0xfffe)?;
    if flags & BLOCK_HAS_COPY_DISPOSE != 0 {
        let desc = emu.read_u64(block + 24)?;
        let dispose = emu.read_u64(desc + 24)?;
        if dispose != 0 {
            emu.call(dispose, &[block])?;
        }
    }
    emu.heap.free(&emu.p.memory, block)?;
    Ok(())
}

fn block_release(emu: &mut Emu) -> Result<()> {
    block_release_obj(emu, emu.arg(0))
}

/// Is this a heap block (for Objective-C retain/release of blocks)?
pub(crate) fn is_heap_block(emu: &Emu, obj: u64) -> bool {
    emu.p.memory.read_u32(obj + 8).is_ok_and(|f| f & BLOCK_NEEDS_FREE != 0 && f & BLOCK_IS_GLOBAL == 0)
        && emu.read_u64(obj).is_ok_and(|isa| emu.objc.class(isa).is_some_and(|c| c.name == "__NSMallocBlock__"))
}

/// `_Block_object_assign(void *dst, const void *obj, int flags)`: store a captured variable.
fn block_object_assign(emu: &mut Emu) -> Result<()> {
    let (dst, obj, flags) = (emu.arg(0), emu.arg(1), emu.arg(2) as u32);
    const FIELD_IS_OBJECT: u32 = 3;
    const FIELD_IS_BLOCK: u32 = 7;
    const FIELD_IS_BYREF: u32 = 8;
    let value = match flags & 0xff {
        f if f & FIELD_IS_BYREF != 0 => byref_copy(emu, obj)?,
        FIELD_IS_BLOCK => {
            emu.cpu.x[0] = obj;
            block_copy(emu)?;
            emu.cpu.x[0]
        }
        FIELD_IS_OBJECT => {
            crate::objc::objc_retain_obj(emu, obj)?;
            obj
        }
        _ => obj,
    };
    emu.write_u64(dst, value)
}

/// Move a `__block` variable to the heap (libclosure's `_Block_byref_copy`).
///
/// struct Block_byref { void *isa; Block_byref *forwarding; int32 flags; uint32 size;
///                      [keep, destroy helpers if BLOCK_BYREF_HAS_COPY_DISPOSE]; variable... }
fn byref_copy(emu: &mut Emu, src: u64) -> Result<u64> {
    const BYREF_NEEDS_FREE: u32 = 1 << 24;
    const BYREF_HAS_COPY_DISPOSE: u32 = 1 << 25;
    const BYREF_LAYOUT_EXTENDED: u32 = 1 << 28;
    let forwarding = emu.read_u64(src + 8)?;
    if forwarding != src && forwarding != 0 {
        return Ok(forwarding); // already on the heap
    }
    let flags = emu.p.memory.read_u32(src + 16)?;
    if flags & BYREF_NEEDS_FREE != 0 {
        return Ok(src); // a heap byref: sharing it is the retain
    }
    let size = emu.p.memory.read_u32(src + 20)? as u64;
    let copy = emu.alloc(size.max(24))?;
    emu.write_u64(copy, 0)?;
    emu.write_u64(copy + 8, copy)?;
    emu.p.memory.write_u32(copy + 16, flags | BYREF_NEEDS_FREE | 4)?; // refcount 2: the frame's and the block's
    emu.p.memory.write_u32(copy + 20, size as u32)?;
    emu.write_u64(src + 8, copy)?; // the stack copy now forwards to the heap one
    if flags & BYREF_HAS_COPY_DISPOSE != 0 {
        let keep = emu.read_u64(src + 24)?;
        let destroy = emu.read_u64(src + 32)?;
        emu.write_u64(copy + 24, keep)?;
        emu.write_u64(copy + 32, destroy)?;
        if flags & BYREF_LAYOUT_EXTENDED != 0 {
            let layout = emu.read_u64(src + 40)?;
            emu.write_u64(copy + 40, layout)?;
        }
        // The helper copies the variable (e.g. a C++ copy constructor).
        emu.call(keep, &[copy, src])?;
    } else if size > 24 {
        let bytes = emu.p.memory.read(src + 24, (size - 24) as usize)?.to_vec();
        emu.p.memory.write(copy + 24, &bytes)?;
    }
    Ok(copy)
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
    ("__Block_release", block_release),
    ("__Block_object_assign", block_object_assign),
    ("__Block_object_dispose", noop),
];
