//! Mach-O inspection for pomme.
//!
//! Today this only *reads* a binary and reports what it needs from the
//! outside world (dylibs + imported symbols). The same parsing will become
//! the front half of the loader that maps guest code into emulated memory.

use goblin::mach::{Mach, MachO, constants::cputype, load_command::CommandVariant};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a Mach-O file: {0}")]
    Parse(#[from] goblin::error::Error),
    #[error("fat binary has no arm64 slice")]
    NoArm64Slice,
    #[error("malformed chained fixups: {0}")]
    ChainedFixups(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Ios,
    IosSimulator,
    Other(u32),
}

impl Platform {
    fn from_raw(p: u32) -> Self {
        match p {
            1 => Self::MacOs,
            2 => Self::Ios,
            7 => Self::IosSimulator,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Version(pub u32);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v = self.0;
        write!(f, "{}.{}.{}", v >> 16, (v >> 8) & 0xff, v & 0xff)
    }
}

#[derive(Debug, Clone)]
pub struct BuildVersion {
    pub platform: Platform,
    pub min_os: Version,
    pub sdk: Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DylibKind {
    /// Lives in the OS (`/System/...`, `/usr/lib/...`) — pomme must provide it (HLE).
    System,
    /// Shipped inside the app bundle (`@rpath`, `@executable_path`, ...) — guest code we execute.
    Bundled,
}

#[derive(Debug, Clone)]
pub struct Dylib {
    pub path: String,
    pub kind: DylibKind,
    pub weak: bool,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub name: String,
    /// Install name of the library, or a special lookup like `<flat>`.
    pub library: String,
    pub weak: bool,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub cpu: &'static str,
    pub file_type: &'static str,
    pub build: Option<BuildVersion>,
    pub entry: u64,
    pub segments: Vec<(String, u64, u64)>,
    pub dylibs: Vec<Dylib>,
    pub imports: Vec<Import>,
    pub uses_chained_fixups: bool,
}

impl Summary {
    /// Imports grouped by library, sorted.
    pub fn imports_by_library(&self) -> BTreeMap<&str, Vec<&Import>> {
        let mut map: BTreeMap<&str, Vec<&Import>> = BTreeMap::new();
        for imp in &self.imports {
            map.entry(imp.library.as_str()).or_default().push(imp);
        }
        for v in map.values_mut() {
            v.sort_by(|a, b| a.name.cmp(&b.name));
        }
        map
    }
}

/// Parse a Mach-O (thin or fat) and summarise its arm64 slice.
pub fn inspect(bytes: &[u8]) -> Result<Summary> {
    match Mach::parse(bytes)? {
        Mach::Binary(macho) => summarise(&macho, bytes),
        Mach::Fat(fat) => {
            for (i, arch) in fat.iter_arches().enumerate() {
                let arch = arch?;
                if arch.cputype == cputype::CPU_TYPE_ARM64 {
                    let slice = arch.slice(bytes);
                    if let goblin::mach::SingleArch::MachO(m) = fat.get(i)? {
                        return summarise(&m, slice);
                    }
                }
            }
            Err(Error::NoArm64Slice)
        }
    }
}

fn summarise(m: &MachO<'_>, bytes: &[u8]) -> Result<Summary> {
    let cpu = match m.header.cputype {
        cputype::CPU_TYPE_ARM64 => "arm64",
        cputype::CPU_TYPE_ARM => "armv7",
        cputype::CPU_TYPE_X86_64 => "x86_64",
        _ => "unknown",
    };
    let file_type = match m.header.filetype {
        goblin::mach::header::MH_EXECUTE => "executable",
        goblin::mach::header::MH_DYLIB => "dylib",
        goblin::mach::header::MH_BUNDLE => "bundle",
        _ => "other",
    };

    let mut build = None;
    let mut chained = None;
    let mut dylibs = Vec::new();
    for lc in &m.load_commands {
        match &lc.command {
            CommandVariant::BuildVersion(bv) => {
                build = Some(BuildVersion {
                    platform: Platform::from_raw(bv.platform),
                    min_os: Version(bv.minos),
                    sdk: Version(bv.sdk),
                });
            }
            CommandVariant::DyldChainedFixups(cmd) => {
                chained = Some((cmd.dataoff as usize, cmd.datasize as usize));
            }
            CommandVariant::LoadDylib(d)
            | CommandVariant::LoadWeakDylib(d)
            | CommandVariant::ReexportDylib(d)
            | CommandVariant::LazyLoadDylib(d)
            | CommandVariant::LoadUpwardDylib(d) => {
                let off = lc.offset + d.dylib.name as usize;
                let path = cstr_at(bytes, off).unwrap_or("<bad name>").to_string();
                let kind = if path.starts_with('@') {
                    DylibKind::Bundled
                } else {
                    DylibKind::System
                };
                let weak = matches!(lc.command, CommandVariant::LoadWeakDylib(_));
                dylibs.push(Dylib { path, kind, weak });
            }
            _ => {}
        }
    }

    let imports = match chained {
        Some((off, size)) => {
            let data = bytes
                .get(off..off + size)
                .ok_or(Error::ChainedFixups("out of bounds"))?;
            parse_chained_imports(data, &dylibs)?
        }
        None => m
            .imports()?
            .into_iter()
            .map(|i| Import {
                name: i.name.to_string(),
                library: i.dylib.to_string(),
                weak: i.is_weak,
            })
            .collect(),
    };

    let segments = m
        .segments
        .iter()
        .map(|s| {
            (
                s.name().unwrap_or("?").to_string(),
                s.vmaddr,
                s.vmsize,
            )
        })
        .collect();

    Ok(Summary {
        cpu,
        file_type,
        build,
        entry: m.entry,
        segments,
        dylibs,
        imports,
        uses_chained_fixups: chained.is_some(),
    })
}

fn cstr_at(bytes: &[u8], off: usize) -> Option<&str> {
    let rest = bytes.get(off..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&rest[..end]).ok()
}

fn u32_at(d: &[u8], off: usize) -> Result<u32> {
    d.get(off..off + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(Error::ChainedFixups("truncated"))
}

fn u64_at(d: &[u8], off: usize) -> Result<u64> {
    d.get(off..off + 8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .ok_or(Error::ChainedFixups("truncated"))
}

/// Decode the imports table of `LC_DYLD_CHAINED_FIXUPS`
/// (see `<mach-o/fixup-chains.h>`).
fn parse_chained_imports(d: &[u8], dylibs: &[Dylib]) -> Result<Vec<Import>> {
    let imports_offset = u32_at(d, 8)? as usize;
    let symbols_offset = u32_at(d, 12)? as usize;
    let imports_count = u32_at(d, 16)? as usize;
    let imports_format = u32_at(d, 20)?;
    let symbols_format = u32_at(d, 24)?;
    if symbols_format != 0 {
        return Err(Error::ChainedFixups("compressed symbol table unsupported"));
    }

    let mut out = Vec::with_capacity(imports_count);
    for i in 0..imports_count {
        // Normalise all three formats to (ordinal as signed, weak, name_offset).
        let (ordinal, weak, name_off): (i64, bool, usize) = match imports_format {
            // DYLD_CHAINED_IMPORT: lib_ordinal:8 weak:1 name_offset:23
            1 | 2 => {
                let stride = if imports_format == 1 { 4 } else { 8 };
                let raw = u32_at(d, imports_offset + i * stride)?;
                let ord = (raw & 0xff) as u8 as i8 as i64;
                // Ordinals 0xF0.. are the special negative values.
                let ord = if (raw & 0xff) > 0xf0 { ord } else { (raw & 0xff) as i64 };
                (ord, raw & 0x100 != 0, (raw >> 9) as usize)
            }
            // DYLD_CHAINED_IMPORT_ADDEND64: lib_ordinal:16 weak:1 reserved:15 name_offset:32
            3 => {
                let raw = u64_at(d, imports_offset + i * 16)?;
                let ord16 = (raw & 0xffff) as u16;
                let ord = if ord16 > 0xfff0 { ord16 as i16 as i64 } else { ord16 as i64 };
                (ord, raw & 0x1_0000 != 0, (raw >> 32) as usize)
            }
            _ => return Err(Error::ChainedFixups("unknown imports_format")),
        };
        let name = cstr_at(d, symbols_offset + name_off)
            .ok_or(Error::ChainedFixups("bad symbol name"))?
            .to_string();
        let library = match ordinal {
            0 => "<self>".to_string(),
            -1 => "<main executable>".to_string(),
            -2 => "<flat lookup>".to_string(),
            -3 => "<weak lookup>".to_string(),
            n if n > 0 => dylibs
                .get(n as usize - 1)
                .map(|d| d.path.clone())
                .unwrap_or_else(|| format!("<ordinal {n}>")),
            n => format!("<ordinal {n}>"),
        };
        out.push(Import { name, library, weak });
    }
    Ok(out)
}
