//! The export trie (`LC_DYLD_EXPORTS_TRIE`, or `export_off` in dyld_info).

use crate::read::Cursor;
use crate::{Error, LibOrdinal, Result};

const EXPORT_SYMBOL_FLAGS_KIND_MASK: u64 = 0x03;
const EXPORT_SYMBOL_FLAGS_KIND_THREAD_LOCAL: u64 = 0x01;
const EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE: u64 = 0x02;
const EXPORT_SYMBOL_FLAGS_WEAK_DEFINITION: u64 = 0x04;
const EXPORT_SYMBOL_FLAGS_REEXPORT: u64 = 0x08;
const EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER: u64 = 0x10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportKind {
    /// Defined here, at this unslid vmaddr.
    Regular { addr: u64 },
    ThreadLocal { addr: u64 },
    /// A fixed value, not relocated.
    Absolute { value: u64 },
    /// Defined by another library this image links (`dylibs[ordinal]`), maybe renamed.
    Reexport { ordinal: LibOrdinal, name: String },
}

#[derive(Debug, Clone)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub weak: bool,
}

pub fn parse(trie: &[u8], header_vmaddr: u64) -> Result<Vec<Export>> {
    let mut out = Vec::new();
    let mut stack = vec![(0usize, String::new())];
    let mut visited = vec![false; trie.len()];

    while let Some((node, prefix)) = stack.pop() {
        if node >= trie.len() || std::mem::replace(&mut visited[node], true) {
            return Err(Error::Malformed("export trie (bad or cyclic node)"));
        }
        let mut c = Cursor::at(trie, node, "export trie");
        let terminal_size = c.uleb()? as usize;
        let children_at = c.pos + terminal_size;

        if terminal_size > 0 {
            let flags = c.uleb()?;
            let weak = flags & EXPORT_SYMBOL_FLAGS_WEAK_DEFINITION != 0;
            let kind = if flags & EXPORT_SYMBOL_FLAGS_REEXPORT != 0 {
                let ordinal = LibOrdinal::from_signed(c.uleb()? as i64)?;
                let name = c.cstr()?;
                let name = if name.is_empty() { prefix.clone() } else { name.to_string() };
                ExportKind::Reexport { ordinal, name }
            } else {
                // For STUB_AND_RESOLVER, this is the stub; the resolver follows. Calling
                // the stub is always correct, so we ignore the resolver.
                let addr = c.uleb()?;
                if flags & EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER != 0 {
                    c.uleb()?;
                }
                match flags & EXPORT_SYMBOL_FLAGS_KIND_MASK {
                    EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE => ExportKind::Absolute { value: addr },
                    EXPORT_SYMBOL_FLAGS_KIND_THREAD_LOCAL => {
                        ExportKind::ThreadLocal { addr: header_vmaddr + addr }
                    }
                    _ => ExportKind::Regular { addr: header_vmaddr + addr },
                }
            };
            out.push(Export { name: prefix.clone(), kind, weak });
        }

        let mut c = Cursor::at(trie, children_at, "export trie children");
        let child_count = c.u8()?;
        for _ in 0..child_count {
            let edge = c.cstr()?;
            let child = c.uleb()? as usize;
            stack.push((child, format!("{prefix}{edge}")));
        }
    }
    Ok(out)
}
