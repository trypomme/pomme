//! Mach-O parsing for pomme.
//!
//! [`Image::parse`] turns an arm64 Mach-O into everything the loader needs:
//! segments (with their file bytes), linked dylibs, imports, a flat list of
//! fixups (rebases + binds, from either chained fixups or dyld_info opcodes),
//! and the export trie.

mod chained;
mod exports;
mod opcodes;
mod read;

use goblin::mach::{Mach, MachO, constants::cputype, load_command::CommandVariant};
use std::collections::BTreeMap;

pub use exports::{Export, ExportKind};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a Mach-O file: {0}")]
    Parse(#[from] goblin::error::Error),
    #[error("fat binary has no arm64 slice")]
    NoArm64Slice,
    #[error("malformed {0}")]
    Malformed(&'static str),
    #[error("unsupported: {0}")]
    Unsupported(String),
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
    /// `LC_REEXPORT_DYLIB`: its exports are also visible through this image.
    pub reexport: bool,
}

/// Where an import is looked up, from its two-level-namespace ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibOrdinal {
    /// This image itself.
    SelfImage,
    MainExecutable,
    /// Search every loaded image (`-flat_namespace`).
    Flat,
    /// Search every image for a weak definition (C++ coalescing).
    WeakLookup,
    /// Index into [`Image::dylibs`].
    Dylib(usize),
}

impl LibOrdinal {
    pub(crate) fn from_signed(n: i64) -> Result<Self> {
        Ok(match n {
            0 => Self::SelfImage,
            -1 => Self::MainExecutable,
            -2 => Self::Flat,
            -3 => Self::WeakLookup,
            n if n > 0 => Self::Dylib(n as usize - 1),
            _ => return Err(Error::Malformed("library ordinal")),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Import {
    pub name: String,
    pub ordinal: LibOrdinal,
    pub weak: bool,
    pub addend: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixup {
    /// Write `target` (an unslid vmaddr of this image) + slide at `at`.
    Rebase { at: u64, target: u64 },
    /// Write the address of `imports[import]` + `addend` at `at`.
    Bind { at: u64, import: usize, addend: i64 },
}

impl Fixup {
    pub fn location(&self) -> u64 {
        match *self {
            Fixup::Rebase { at, .. } | Fixup::Bind { at, .. } => at,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Prot {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
}

impl Prot {
    fn from_raw(p: u32) -> Self {
        Self { read: p & 1 != 0, write: p & 2 != 0, exec: p & 4 != 0 }
    }
}

impl std::fmt::Display for Prot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = |b, ch| if b { ch } else { '-' };
        write!(f, "{}{}{}", c(self.read, 'r'), c(self.write, 'w'), c(self.exec, 'x'))
    }
}

#[derive(Debug, Clone)]
pub struct Segment<'a> {
    pub name: String,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub prot: Prot,
    /// File contents (`filesize` bytes); the rest of `vmsize` is zero-fill.
    pub data: &'a [u8],
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    /// Unslid vmaddr.
    pub addr: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Executable,
    Dylib,
    Bundle,
    Other(u32),
}

#[derive(Debug, Clone)]
pub struct Image<'a> {
    pub cpu: &'static str,
    pub file_type: FileType,
    pub build: Option<BuildVersion>,
    /// `LC_ID_DYLIB`, for dylibs.
    pub install_name: Option<String>,
    /// Unslid vmaddr of the Mach-O header (start of `__TEXT`).
    pub header_vmaddr: u64,
    /// Unslid vmaddr of `main`, for executables (`LC_MAIN`).
    pub entry: Option<u64>,
    pub segments: Vec<Segment<'a>>,
    pub dylibs: Vec<Dylib>,
    pub rpaths: Vec<String>,
    pub imports: Vec<Import>,
    pub fixups: Vec<Fixup>,
    pub exports: Vec<Export>,
    /// Every function and data symbol the symbol table defines (locals too):
    /// (unslid vmaddr, name), sorted by address. For symbolizing and breakpoints.
    pub symbols: Vec<(u64, String)>,
    pub uses_chained_fixups: bool,
}

impl<'a> Image<'a> {
    /// Parse a Mach-O (thin or fat), picking the arm64 slice.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        match Mach::parse(bytes)? {
            Mach::Binary(m) => Self::from_macho(&m, bytes),
            Mach::Fat(fat) => {
                for arch in fat.iter_arches() {
                    let arch = arch?;
                    if arch.cputype == cputype::CPU_TYPE_ARM64 {
                        let slice = arch.slice(bytes);
                        return Self::from_macho(&MachO::parse(slice, 0)?, slice);
                    }
                }
                Err(Error::NoArm64Slice)
            }
        }
    }

    /// Display name of the library an import binds to.
    pub fn library_name(&self, ord: LibOrdinal) -> &str {
        match ord {
            LibOrdinal::SelfImage => "<self>",
            LibOrdinal::MainExecutable => "<main executable>",
            LibOrdinal::Flat => "<flat lookup>",
            LibOrdinal::WeakLookup => "<weak lookup>",
            LibOrdinal::Dylib(i) => self.dylibs.get(i).map_or("<bad ordinal>", |d| &d.path),
        }
    }

    /// Imports grouped by library display name, sorted.
    pub fn imports_by_library(&self) -> BTreeMap<&str, Vec<&Import>> {
        let mut map: BTreeMap<&str, Vec<&Import>> = BTreeMap::new();
        for imp in &self.imports {
            map.entry(self.library_name(imp.ordinal)).or_default().push(imp);
        }
        for v in map.values_mut() {
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v.dedup_by(|a, b| a.name == b.name);
        }
        map
    }

    /// Find a section by segment and section name, e.g. `("__DATA", "__objc_classlist")`.
    /// Also matches `__DATA_CONST` / `__DATA_DIRTY` when asked for `__DATA`.
    pub fn section(&self, segment: &str, name: &str) -> Option<&Section> {
        self.segments
            .iter()
            .filter(|s| s.name == segment || (segment == "__DATA" && s.name.starts_with("__DATA")))
            .flat_map(|s| &s.sections)
            .find(|sec| sec.name == name)
    }

    /// Segments that are actually mapped (skips `__PAGEZERO`).
    pub fn mapped_segments(&self) -> impl Iterator<Item = &Segment<'a>> {
        self.segments.iter().filter(|s| s.vmsize > 0 && !(s.vmaddr == 0 && s.name == "__PAGEZERO"))
    }

    /// Read the raw pointer-sized value stored in the file at unslid `vmaddr`.
    pub(crate) fn file_u64_at(segments: &[Segment<'_>], vmaddr: u64) -> Result<u64> {
        let seg = segments
            .iter()
            .find(|s| vmaddr >= s.vmaddr && vmaddr + 8 <= s.vmaddr + s.vmsize)
            .ok_or(Error::Malformed("fixup outside any segment"))?;
        let off = (vmaddr - seg.vmaddr) as usize;
        // Inside vmsize but past filesize reads as zero-fill.
        Ok(seg.data.get(off..off + 8).map_or(0, read::le_u64))
    }

    fn from_macho(m: &MachO<'_>, bytes: &'a [u8]) -> Result<Self> {
        let cpu = match m.header.cputype {
            cputype::CPU_TYPE_ARM64 => "arm64",
            cputype::CPU_TYPE_ARM => "armv7",
            cputype::CPU_TYPE_X86_64 => "x86_64",
            _ => "unknown",
        };
        let file_type = match m.header.filetype {
            goblin::mach::header::MH_EXECUTE => FileType::Executable,
            goblin::mach::header::MH_DYLIB => FileType::Dylib,
            goblin::mach::header::MH_BUNDLE => FileType::Bundle,
            other => FileType::Other(other),
        };

        let mut segments = Vec::new();
        for s in &m.segments {
            let start = s.fileoff as usize;
            let data = bytes
                .get(start..start + s.filesize as usize)
                .ok_or(Error::Malformed("segment file range"))?;
            segments.push(Segment {
                name: s.name().unwrap_or("?").to_string(),
                vmaddr: s.vmaddr,
                vmsize: s.vmsize,
                prot: Prot::from_raw(s.initprot),
                data,
                sections: s
                    .sections()?
                    .iter()
                    .map(|(sec, _)| Section { name: sec.name().unwrap_or("?").to_string(), addr: sec.addr, size: sec.size })
                    .collect(),
            });
        }
        let header_vmaddr = segments
            .iter()
            .find(|s| s.name == "__TEXT")
            .map(|s| s.vmaddr)
            .ok_or(Error::Malformed("no __TEXT segment"))?;

        let linkedit = |off: u32, size: u32| -> Result<&'a [u8]> {
            bytes
                .get(off as usize..(off + size) as usize)
                .ok_or(Error::Malformed("linkedit range"))
        };

        let mut build = None;
        let mut install_name = None;
        let mut entry = None;
        let mut chained_data = None;
        let mut dyld_info = None;
        let mut export_trie = None;
        let mut dylibs = Vec::new();
        let mut rpaths = Vec::new();

        for lc in &m.load_commands {
            let name_at = |off: u32| read::cstr(bytes, lc.offset + off as usize).unwrap_or("<bad>").to_string();
            match &lc.command {
                CommandVariant::BuildVersion(bv) => {
                    build = Some(BuildVersion {
                        platform: Platform::from_raw(bv.platform),
                        min_os: Version(bv.minos),
                        sdk: Version(bv.sdk),
                    });
                }
                CommandVariant::IdDylib(d) => install_name = Some(name_at(d.dylib.name)),
                CommandVariant::Main(e) => entry = Some(header_vmaddr + e.entryoff),
                CommandVariant::Rpath(r) => rpaths.push(name_at(r.path)),
                CommandVariant::DyldChainedFixups(c) => chained_data = Some(linkedit(c.dataoff, c.datasize)?),
                CommandVariant::DyldExportsTrie(c) => export_trie = Some(linkedit(c.dataoff, c.datasize)?),
                CommandVariant::DyldInfo(i) | CommandVariant::DyldInfoOnly(i) => dyld_info = Some(*i),
                CommandVariant::LoadDylib(d)
                | CommandVariant::LoadWeakDylib(d)
                | CommandVariant::ReexportDylib(d)
                | CommandVariant::LazyLoadDylib(d)
                | CommandVariant::LoadUpwardDylib(d) => {
                    let path = name_at(d.dylib.name);
                    let kind = if path.starts_with('@') { DylibKind::Bundled } else { DylibKind::System };
                    let weak = matches!(lc.command, CommandVariant::LoadWeakDylib(_));
                    let reexport = matches!(lc.command, CommandVariant::ReexportDylib(_));
                    dylibs.push(Dylib { path, kind, weak, reexport });
                }
                _ => {}
            }
        }

        let (imports, fixups) = if let Some(data) = chained_data {
            chained::parse(data, &segments, header_vmaddr)?
        } else if let Some(info) = dyld_info {
            if export_trie.is_none() && info.export_size > 0 {
                export_trie = Some(linkedit(info.export_off, info.export_size)?);
            }
            opcodes::parse(
                &segments,
                linkedit(info.rebase_off, info.rebase_size)?,
                linkedit(info.bind_off, info.bind_size)?,
                linkedit(info.lazy_bind_off, info.lazy_bind_size)?,
            )?
        } else {
            (Vec::new(), Vec::new())
        };

        let exports = match export_trie {
            Some(t) => exports::parse(t, header_vmaddr)?,
            None => Vec::new(),
        };

        let mut symbols: Vec<(u64, String)> = m
            .symbols()
            .flatten()
            .filter(|(name, n)| !n.is_stab() && n.get_type() == goblin::mach::symbols::N_SECT && !name.is_empty() && !name.starts_with("ltmp"))
            .map(|(name, n)| (n.n_value, name.to_string()))
            .collect();
        symbols.sort();
        symbols.dedup_by_key(|s| s.0);

        Ok(Image {
            cpu,
            file_type,
            build,
            install_name,
            header_vmaddr,
            entry,
            segments,
            dylibs,
            rpaths,
            imports,
            fixups,
            exports,
            symbols,
            uses_chained_fixups: chained_data.is_some(),
        })
    }
}
