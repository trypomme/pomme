//! `LC_DYLD_CHAINED_FIXUPS` (see `<mach-o/fixup-chains.h>`).
//!
//! Each fixup location in `__DATA*` holds a packed 64-bit value that is either
//! a rebase or a bind, plus the distance to the next fixup on the same page.
//! We walk every chain and flatten it into [`Fixup`]s.

use crate::read::{Cursor, cstr};
use crate::{Error, Fixup, Image, Import, LibOrdinal, Result, Segment};

const DYLD_CHAINED_PTR_64: u16 = 2;
const DYLD_CHAINED_PTR_64_OFFSET: u16 = 6;
const DYLD_CHAINED_PTR_START_NONE: u16 = 0xffff;

pub fn parse(data: &[u8], segments: &[Segment<'_>], header_vmaddr: u64) -> Result<(Vec<Import>, Vec<Fixup>)> {
    let mut h = Cursor::new(data, "chained fixups header");
    let _version = h.u32()?;
    let starts_offset = h.u32()? as usize;
    let imports_offset = h.u32()? as usize;
    let symbols_offset = h.u32()? as usize;
    let imports_count = h.u32()? as usize;
    let imports_format = h.u32()?;
    let symbols_format = h.u32()?;
    if symbols_format != 0 {
        return Err(Error::Unsupported("compressed chained-fixup symbol table".into()));
    }

    let imports = parse_imports(data, imports_offset, symbols_offset, imports_count, imports_format)?;
    let fixups = parse_starts(data, starts_offset, segments, header_vmaddr, imports.len())?;
    Ok((imports, fixups))
}

fn parse_imports(
    data: &[u8],
    imports_offset: usize,
    symbols_offset: usize,
    count: usize,
    format: u32,
) -> Result<Vec<Import>> {
    let mut c = Cursor::at(data, imports_offset, "chained imports");
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        // Normalise all three formats to (signed ordinal, weak, name offset, addend).
        let (ordinal, weak, name_off, addend) = match format {
            // DYLD_CHAINED_IMPORT(_ADDEND): lib_ordinal:8 weak:1 name_offset:23 [+ i32 addend]
            1 | 2 => {
                let raw = c.u32()?;
                let addend = if format == 2 { c.u32()? as i32 as i64 } else { 0 };
                let ord = (raw & 0xff) as u8;
                // 0xfd..=0xff are the special negative ordinals.
                let ord = if ord > 0xf0 { ord as i8 as i64 } else { ord as i64 };
                (ord, raw & 0x100 != 0, (raw >> 9) as usize, addend)
            }
            // DYLD_CHAINED_IMPORT_ADDEND64: lib_ordinal:16 weak:1 reserved:15 name_offset:32, i64 addend
            3 => {
                let raw = c.u64()?;
                let addend = c.u64()? as i64;
                let ord = (raw & 0xffff) as u16;
                let ord = if ord > 0xfff0 { ord as i16 as i64 } else { ord as i64 };
                (ord, raw & 0x1_0000 != 0, (raw >> 32) as usize, addend)
            }
            _ => return Err(Error::Unsupported(format!("chained imports_format {format}"))),
        };
        let name = cstr(data, symbols_offset + name_off)
            .ok_or(Error::Malformed("chained import name"))?
            .to_string();
        out.push(Import { name, ordinal: LibOrdinal::from_signed(ordinal)?, weak, addend });
    }
    Ok(out)
}

fn parse_starts(
    data: &[u8],
    starts_offset: usize,
    segments: &[Segment<'_>],
    header_vmaddr: u64,
    import_count: usize,
) -> Result<Vec<Fixup>> {
    let mut c = Cursor::at(data, starts_offset, "chained starts_in_image");
    let seg_count = c.u32()? as usize;
    let mut fixups = Vec::new();

    for i in 0..seg_count {
        let seg_info_offset = c.u32()? as usize;
        if seg_info_offset == 0 {
            continue;
        }
        let mut s = Cursor::at(data, starts_offset + seg_info_offset, "chained starts_in_segment");
        let _size = s.u32()?;
        let page_size = s.u16()? as u64;
        let pointer_format = s.u16()?;
        let segment_offset = s.u64()?;
        let _max_valid_pointer = s.u32()?;
        let page_count = s.u16()?;

        if !matches!(pointer_format, DYLD_CHAINED_PTR_64 | DYLD_CHAINED_PTR_64_OFFSET) {
            return Err(Error::Unsupported(format!(
                "chained pointer format {pointer_format} in segment {i} (arm64e?)"
            )));
        }

        for page in 0..page_count as u64 {
            let page_start = s.u16()?;
            if page_start == DYLD_CHAINED_PTR_START_NONE {
                continue;
            }
            let mut at = header_vmaddr + segment_offset + page * page_size + page_start as u64;
            loop {
                let raw = Image::file_u64_at(segments, at)?;
                let next = (raw >> 51) & 0xfff;
                if raw >> 63 == 1 {
                    let import = (raw & 0xff_ffff) as usize;
                    if import >= import_count {
                        return Err(Error::Malformed("chained bind ordinal"));
                    }
                    let addend = ((raw >> 24) & 0xff) as i64;
                    fixups.push(Fixup::Bind { at, import, addend });
                } else {
                    let target36 = raw & 0xf_ffff_ffff;
                    let high8 = (raw >> 36) & 0xff;
                    let target = if pointer_format == DYLD_CHAINED_PTR_64_OFFSET {
                        header_vmaddr + target36
                    } else {
                        target36
                    };
                    fixups.push(Fixup::Rebase { at, target: target | (high8 << 56) });
                }
                if next == 0 {
                    break;
                }
                at += next * 4;
            }
        }
    }
    Ok(fixups)
}
