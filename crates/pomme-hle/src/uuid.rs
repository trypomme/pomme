//! NSUUID: random (version 4) UUIDs from pomme's seeded generator, so a run
//! repeats exactly; parsed from and written as "E621E1F8-C36C-495A-93FC-0C247A3E6E5F".

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};

/// The UUID's 16 bytes sit after the isa.
const BYTES: u64 = 8;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn bytes_of(emu: &Emu, u: u64) -> Result<[u8; 16]> {
    let b = emu.p.memory.read(u + BYTES, 16)?;
    let mut out = [0u8; 16];
    out.copy_from_slice(b);
    Ok(out)
}

fn set(emu: &mut Emu, u: u64, bytes: &[u8; 16]) -> Result<u64> {
    emu.p.memory.write(u + BYTES, bytes)?;
    Ok(u)
}

/// An NSUUID object with room for its bytes.
fn new_uuid(emu: &mut Emu, bytes: &[u8; 16]) -> Result<u64> {
    let cls = host_class(emu, "NSUUID")?;
    let u = objc::create_instance(emu, cls, 16)?;
    set(emu, u, bytes)
}

/// "E621E1F8-C36C-495A-93FC-0C247A3E6E5F", as NSUUID writes it.
pub fn format(b: &[u8; 16]) -> String {
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// Parse the 8-4-4-4-12 hex form (either case); None if it isn't one.
pub fn parse(s: &str) -> Option<[u8; 16]> {
    let groups: Vec<&str> = s.split('-').collect();
    if groups.iter().map(|g| g.len()).collect::<Vec<_>>() != [8, 4, 4, 4, 12] {
        return None;
    }
    let hex: String = groups.concat();
    let mut out = [0u8; 16];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Whether `obj` is an NSUUID.
fn is_uuid(emu: &mut Emu, obj: u64) -> Result<bool> {
    let cls = host_class(emu, "NSUUID")?;
    objc::is_kind_of(emu, obj, cls)
}

const U: &str = "NSUUID";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (U, true, "UUID", |e| {
        let b = crate::posix::random_uuid(e);
        let u = new_uuid(e, &b)?;
        ret(e, u)
    }),
    (U, true, "allocWithZone:", |e| {
        let u = new_uuid(e, &[0; 16])?;
        ret(e, u)
    }),
    (U, true, "alloc", |e| {
        let u = new_uuid(e, &[0; 16])?;
        ret(e, u)
    }),
    (U, false, "init", |e| {
        let b = crate::posix::random_uuid(e);
        let u = set(e, e.arg(0), &b)?;
        ret(e, u)
    }),
    (U, false, "initWithUUIDString:", |e| {
        let (u, s) = (e.arg(0), e.arg(2));
        let parsed = if s == 0 { None } else { parse(&nsstring(e, s)?) };
        match parsed {
            Some(b) => {
                let u = set(e, u, &b)?;
                ret(e, u)
            }
            None => ret(e, 0),
        }
    }),
    (U, false, "initWithUUIDBytes:", |e| {
        let (u, p) = (e.arg(0), e.arg(2));
        let mut b = [0u8; 16];
        if p != 0 {
            b.copy_from_slice(e.p.memory.read(p, 16)?);
        }
        let u = set(e, u, &b)?;
        ret(e, u)
    }),
    (U, false, "getUUIDBytes:", |e| {
        let (u, out) = (e.arg(0), e.arg(2));
        let b = bytes_of(e, u)?;
        e.p.memory.write(out, &b)?;
        Ok(())
    }),
    (U, false, "UUIDString", |e| {
        let b = bytes_of(e, e.arg(0))?;
        let s = foundation::new_nsstring(e, &format(&b))?;
        ret(e, s)
    }),
    (U, false, "description", |e| {
        let b = bytes_of(e, e.arg(0))?;
        let s = foundation::new_nsstring(e, &format(&b))?;
        ret(e, s)
    }),
    (U, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = b != 0 && is_uuid(e, b)? && bytes_of(e, a)? == bytes_of(e, b)?;
        ret(e, same as u64)
    }),
    (U, false, "hash", |e| {
        let b = bytes_of(e, e.arg(0))?;
        let h = u64::from_le_bytes(b[..8].try_into().unwrap()) ^ u64::from_le_bytes(b[8..].try_into().unwrap());
        ret(e, h)
    }),
    (U, false, "compare:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let (x, y) = (bytes_of(e, a)?, bytes_of(e, b)?);
        // NSComparisonResult: -1, 0, 1.
        ret(e, x.cmp(&y) as i64 as u64)
    }),
    (U, false, "copyWithZone:", |e| {
        let u = e.arg(0);
        ret(e, u)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let b = [0xe6, 0x21, 0xe1, 0xf8, 0xc3, 0x6c, 0x49, 0x5a, 0x93, 0xfc, 0x0c, 0x24, 0x7a, 0x3e, 0x6e, 0x5f];
        assert_eq!(format(&b), "E621E1F8-C36C-495A-93FC-0C247A3E6E5F");
        assert_eq!(parse("e621e1f8-c36c-495a-93fc-0c247a3e6e5f"), Some(b));
        assert_eq!(parse("E621E1F8C36C495A93FC0C247A3E6E5F"), None);
        assert_eq!(parse("E621E1F8-C36C-495A-93FC-0C247A3E6E5G"), None);
    }
}
