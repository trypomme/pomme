//! Classic `LC_DYLD_INFO(_ONLY)` rebase and bind opcode streams.
//!
//! Weak-bind (C++ coalescing) opcodes are not interpreted yet: those slots
//! already contain a valid rebased pointer to the image's own definition.

use crate::read::Cursor;
use crate::{Error, Fixup, Image, Import, LibOrdinal, Result, Segment};
use std::collections::HashMap;

pub fn parse(
    segments: &[Segment<'_>],
    rebase: &[u8],
    bind: &[u8],
    lazy_bind: &[u8],
) -> Result<(Vec<Import>, Vec<Fixup>)> {
    let mut fixups = Vec::new();
    parse_rebases(segments, rebase, &mut fixups)?;
    let mut binds = Binder { imports: Vec::new(), dedupe: HashMap::new(), fixups: &mut fixups };
    binds.run(segments, bind, false)?;
    binds.run(segments, lazy_bind, true)?;
    let imports = binds.imports;
    Ok((imports, fixups))
}

fn seg_addr(segments: &[Segment<'_>], seg: usize, off: u64) -> Result<u64> {
    Ok(segments.get(seg).ok_or(Error::Malformed("opcode segment index"))?.vmaddr + off)
}

fn parse_rebases(segments: &[Segment<'_>], data: &[u8], out: &mut Vec<Fixup>) -> Result<()> {
    let mut c = Cursor::new(data, "rebase opcodes");
    let (mut seg, mut off) = (0usize, 0u64);
    let mut push = |seg: usize, off: u64| -> Result<()> {
        let at = seg_addr(segments, seg, off)?;
        out.push(Fixup::Rebase { at, target: Image::file_u64_at(segments, at)? });
        Ok(())
    };
    while !c.done() {
        let b = c.u8()?;
        let (op, imm) = (b & 0xf0, (b & 0x0f) as u64);
        match op {
            0x00 => break, // DONE
            0x10 => {
                // SET_TYPE_IMM: 1 = pointer; 32-bit absolute/pcrel types never appear on arm64.
                if imm != 1 {
                    return Err(Error::Unsupported(format!("rebase type {imm}")));
                }
            }
            0x20 => {
                seg = imm as usize;
                off = c.uleb()?;
            }
            0x30 => off = off.wrapping_add(c.uleb()?),
            0x40 => off += imm * 8,
            0x50 => {
                for _ in 0..imm {
                    push(seg, off)?;
                    off += 8;
                }
            }
            0x60 => {
                for _ in 0..c.uleb()? {
                    push(seg, off)?;
                    off += 8;
                }
            }
            0x70 => {
                push(seg, off)?;
                off = off.wrapping_add(c.uleb()? + 8);
            }
            0x80 => {
                let (count, skip) = (c.uleb()?, c.uleb()?);
                for _ in 0..count {
                    push(seg, off)?;
                    off += skip + 8;
                }
            }
            _ => return Err(Error::Malformed("rebase opcode")),
        }
    }
    Ok(())
}

struct Binder<'f> {
    imports: Vec<Import>,
    dedupe: HashMap<(String, LibOrdinal, bool, i64), usize>,
    fixups: &'f mut Vec<Fixup>,
}

impl Binder<'_> {
    fn run(&mut self, segments: &[Segment<'_>], data: &[u8], lazy: bool) -> Result<()> {
        let mut c = Cursor::new(data, "bind opcodes");
        let (mut seg, mut off) = (0usize, 0u64);
        let mut ordinal = LibOrdinal::SelfImage;
        let mut name = String::new();
        let mut weak = false;
        let mut addend = 0i64;

        while !c.done() {
            let b = c.u8()?;
            let (op, imm) = (b & 0xf0, b & 0x0f);
            let mut bind_here = false;
            let mut advance = 0u64;
            match op {
                // DONE ends the stream for regular binds; lazy binds use it as a separator.
                0x00 if lazy => continue,
                0x00 => break,
                0x10 => ordinal = LibOrdinal::from_signed(imm as i64)?,
                0x20 => ordinal = LibOrdinal::from_signed(c.uleb()? as i64)?,
                0x30 => {
                    let n = if imm == 0 { 0 } else { (0xf0 | imm) as i8 as i64 };
                    ordinal = LibOrdinal::from_signed(n)?;
                }
                0x40 => {
                    name = c.cstr()?.to_string();
                    weak = imm & 0x1 != 0;
                }
                0x50 => {
                    if imm != 1 {
                        return Err(Error::Unsupported(format!("bind type {imm}")));
                    }
                }
                0x60 => addend = c.sleb()?,
                0x70 => {
                    seg = imm as usize;
                    off = c.uleb()?;
                }
                0x80 => off = off.wrapping_add(c.uleb()?),
                0x90 => {
                    bind_here = true;
                    advance = 8;
                }
                0xa0 => {
                    bind_here = true;
                    advance = c.uleb()?.wrapping_add(8);
                }
                0xb0 => {
                    bind_here = true;
                    advance = imm as u64 * 8 + 8;
                }
                0xc0 => {
                    let (count, skip) = (c.uleb()?, c.uleb()?);
                    for _ in 0..count {
                        self.bind(segments, seg, off, &name, ordinal, weak, addend)?;
                        off += skip + 8;
                    }
                }
                0xd0 => return Err(Error::Unsupported("threaded binds (arm64e)".into())),
                _ => return Err(Error::Malformed("bind opcode")),
            }
            if bind_here {
                self.bind(segments, seg, off, &name, ordinal, weak, addend)?;
                off = off.wrapping_add(advance);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn bind(
        &mut self,
        segments: &[Segment<'_>],
        seg: usize,
        off: u64,
        name: &str,
        ordinal: LibOrdinal,
        weak: bool,
        addend: i64,
    ) -> Result<()> {
        let key = (name.to_string(), ordinal, weak, addend);
        let import = *self.dedupe.entry(key).or_insert_with(|| {
            self.imports.push(Import { name: name.to_string(), ordinal, weak, addend });
            self.imports.len() - 1
        });
        let at = seg_addr(segments, seg, off)?;
        // The import already carries the addend.
        self.fixups.push(Fixup::Bind { at, import, addend: 0 });
        Ok(())
    }
}
