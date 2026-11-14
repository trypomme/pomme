//! CommonCrypto's message digests: MD5, SHA-1 and the SHA-2 family, one-shot
//! (`CC_MD5(data, len, md)`) and incremental (`CC_SHA256_Init/Update/Final`).
//! The incremental state lives host-side, keyed by the guest's context
//! struct (whose bytes apps don't look inside).

use crate::{Emu, HostFn, Result};
use sha2::Digest;
use std::collections::HashMap;

#[derive(Clone, Copy)]
enum Algorithm {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
}

#[derive(Clone)]
enum Hasher {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha224(sha2::Sha224),
    Sha256(sha2::Sha256),
    Sha384(sha2::Sha384),
    Sha512(sha2::Sha512),
}

impl Hasher {
    fn new(a: Algorithm) -> Hasher {
        match a {
            Algorithm::Md5 => Hasher::Md5(md5::Md5::new()),
            Algorithm::Sha1 => Hasher::Sha1(sha1::Sha1::new()),
            Algorithm::Sha224 => Hasher::Sha224(sha2::Sha224::new()),
            Algorithm::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
            Algorithm::Sha384 => Hasher::Sha384(sha2::Sha384::new()),
            Algorithm::Sha512 => Hasher::Sha512(sha2::Sha512::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Md5(h) => h.update(bytes),
            Hasher::Sha1(h) => h.update(bytes),
            Hasher::Sha224(h) => h.update(bytes),
            Hasher::Sha256(h) => h.update(bytes),
            Hasher::Sha384(h) => h.update(bytes),
            Hasher::Sha512(h) => h.update(bytes),
        }
    }

    fn finish(self) -> Vec<u8> {
        match self {
            Hasher::Md5(h) => h.finalize().to_vec(),
            Hasher::Sha1(h) => h.finalize().to_vec(),
            Hasher::Sha224(h) => h.finalize().to_vec(),
            Hasher::Sha256(h) => h.finalize().to_vec(),
            Hasher::Sha384(h) => h.finalize().to_vec(),
            Hasher::Sha512(h) => h.finalize().to_vec(),
        }
    }
}

#[derive(Clone, Default)]
pub struct State {
    contexts: HashMap<u64, Hasher>,
}

/// `CC_X(data, len, md)`: the digest of `len` bytes into `md`; returns `md`.
fn one_shot(emu: &mut Emu, a: Algorithm) -> Result<()> {
    let (data, len, md) = (emu.arg(0), emu.arg(1) as u32, emu.arg(2));
    let bytes = if len == 0 { Vec::new() } else { emu.p.memory.read(data, len as usize)?.to_vec() };
    let mut h = Hasher::new(a);
    h.update(&bytes);
    emu.p.memory.write(md, &h.finish())?;
    emu.ret(md);
    Ok(())
}

/// `CC_X_Init(ctx)` → 1.
fn init(emu: &mut Emu, a: Algorithm) -> Result<()> {
    let ctx = emu.arg(0);
    emu.crypto.contexts.insert(ctx, Hasher::new(a));
    emu.ret(1);
    Ok(())
}

/// `CC_X_Update(ctx, data, len)` → 1.
fn update(emu: &mut Emu, a: Algorithm) -> Result<()> {
    let (ctx, data, len) = (emu.arg(0), emu.arg(1), emu.arg(2) as u32);
    let bytes = if len == 0 { Vec::new() } else { emu.p.memory.read(data, len as usize)?.to_vec() };
    emu.crypto.contexts.entry(ctx).or_insert_with(|| Hasher::new(a)).update(&bytes);
    emu.ret(1);
    Ok(())
}

/// `CC_X_Final(md, ctx)` → 1.
fn finish(emu: &mut Emu, a: Algorithm) -> Result<()> {
    let (md, ctx) = (emu.arg(0), emu.arg(1));
    let h = emu.crypto.contexts.remove(&ctx).unwrap_or_else(|| Hasher::new(a));
    emu.p.memory.write(md, &h.finish())?;
    emu.ret(1);
    Ok(())
}

macro_rules! digest {
    ($name:literal, $a:expr) => {
        [
            (concat!("_CC_", $name), |e| one_shot(e, $a)),
            (concat!("_CC_", $name, "_Init"), |e| init(e, $a)),
            (concat!("_CC_", $name, "_Update"), |e| update(e, $a)),
            (concat!("_CC_", $name, "_Final"), |e| finish(e, $a)),
        ]
    };
}

pub(crate) const DIGESTS: [[(&str, HostFn); 4]; 6] = [
    digest!("MD5", Algorithm::Md5),
    digest!("SHA1", Algorithm::Sha1),
    digest!("SHA224", Algorithm::Sha224),
    digest!("SHA256", Algorithm::Sha256),
    digest!("SHA384", Algorithm::Sha384),
    digest!("SHA512", Algorithm::Sha512),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(a: Algorithm, s: &str) -> String {
        let mut h = Hasher::new(a);
        h.update(s.as_bytes());
        h.finish().iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn known_digests() {
        assert_eq!(hex(Algorithm::Md5, "abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(Algorithm::Sha1, "abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(Algorithm::Sha256, "abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
