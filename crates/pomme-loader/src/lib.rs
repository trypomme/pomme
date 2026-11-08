//! The pomme loader: does what `dyld` does at launch.
//!
//! 1. Find the app's executable and, transitively, every bundled framework it links.
//! 2. Map each image's segments into guest [`Memory`] (sliding dylibs to free space).
//! 3. Apply every fixup: rebases get the slide, binds get the target symbol's
//!    address — either an export of another guest image, or an HLE stub for
//!    symbols that live in iOS system libraries.
//!
//! Not done yet: running initializers, ObjC/Swift runtime registration, TLS.

use pomme_mem::{Memory, PAGE_SIZE, page_align_up};
use pomme_macho::{Dylib, DylibKind, ExportKind, FileType, Fixup, Image, LibOrdinal};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

pub use pomme_mem::{self as mem, MemError, Prot};

/// Dylibs are slid into the address space starting here.
const DYLIB_BASE: u64 = 0x2_0000_0000;
/// HLE stubs for system-library symbols live here, one 16-byte slot each.
pub const STUB_BASE: u64 = 0x6_0000_0000;
pub const STUB_SIZE: u64 = 16;
/// Stub library name for flat / weak-lookup imports that no guest image defines.
pub const FLAT_NAMESPACE: &str = "<flat namespace>";
/// `BRK #0x5e` — executing a stub traps into the host; the slot index is (pc - STUB_BASE) / 16.
pub const STUB_INSN: u32 = 0xd420_0000 | (0x5e << 5);

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}: {source}")]
    Macho { path: PathBuf, source: pomme_macho::Error },
    #[error("{path}: {source}")]
    Plist { path: PathBuf, source: plist::Error },
    #[error("{0}")]
    Bundle(String),
    #[error("image {image} linked {lib}, which is not in the bundle")]
    MissingDylib { image: String, lib: String },
    #[error(transparent)]
    Memory(#[from] MemError),
}

pub type Result<T> = std::result::Result<T, LoadError>;

#[derive(Debug)]
pub struct LoadedImage {
    pub name: String,
    pub path: PathBuf,
    pub install_name: Option<String>,
    pub file_type: FileType,
    /// Added to every unslid vmaddr of this image.
    pub slide: u64,
    /// Slid address of the Mach-O header.
    pub header: u64,
    pub start: u64,
    pub end: u64,
    /// Slid address of `main`, if this is the executable.
    pub entry: Option<u64>,
    pub dylibs: Vec<Dylib>,
    /// For each entry in `dylibs`, the index of the loaded image (bundled libs only).
    pub deps: Vec<Option<usize>>,
    exports: HashMap<String, ExportKind>,
}

impl LoadedImage {
    /// Slid address of an exported symbol defined directly in this image.
    pub fn symbol(&self, name: &str) -> Option<u64> {
        match self.exports.get(name)? {
            ExportKind::Regular { addr } | ExportKind::ThreadLocal { addr } => Some(addr.wrapping_add(self.slide)),
            ExportKind::Absolute { value } => Some(*value),
            ExportKind::Reexport { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Stub {
    pub library: String,
    pub symbol: String,
}

#[derive(Debug, Default)]
pub struct Stubs {
    list: Vec<Stub>,
    index: HashMap<Stub, usize>,
}

impl Stubs {
    fn get_or_insert(&mut self, library: &str, symbol: &str) -> u64 {
        let key = Stub { library: library.into(), symbol: symbol.into() };
        let n = self.list.len();
        let i = *self.index.entry(key.clone()).or_insert(n);
        if i == n {
            self.list.push(key);
        }
        STUB_BASE + i as u64 * STUB_SIZE
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &Stub)> {
        self.list.iter().enumerate().map(|(i, s)| (STUB_BASE + i as u64 * STUB_SIZE, s))
    }

    /// The stub at a guest address, if it is one.
    pub fn at(&self, addr: u64) -> Option<&Stub> {
        let off = addr.checked_sub(STUB_BASE)?;
        self.list.get((off / STUB_SIZE) as usize)
    }
}

#[derive(Debug, Default, Clone)]
pub struct LinkStats {
    pub rebases: usize,
    /// Binds resolved to another guest image's export.
    pub binds_to_guest: usize,
    /// Binds resolved to an HLE stub.
    pub binds_to_stub: usize,
    /// Weak imports that resolved to nothing (written as 0).
    pub weak_missing: usize,
    /// Rebased pointers whose target is outside every mapped segment (should be 0).
    pub rebases_unmapped: usize,
}

#[derive(Debug, Clone)]
pub struct Unresolved {
    pub image: String,
    pub library: String,
    pub symbol: String,
}

#[derive(Debug)]
pub struct Process {
    pub bundle_id: Option<String>,
    pub memory: Memory,
    pub images: Vec<LoadedImage>,
    pub stubs: Stubs,
    pub stats: LinkStats,
    /// Imports from bundled libraries that no image exports. Bound to stubs so
    /// that calling them traps with a useful name instead of jumping to 0.
    pub unresolved: Vec<Unresolved>,
}

impl Process {
    pub fn main(&self) -> &LoadedImage {
        &self.images[0]
    }

    /// The image containing a guest address.
    pub fn image_at(&self, addr: u64) -> Option<&LoadedImage> {
        self.images.iter().find(|i| addr >= i.start && addr < i.end)
    }

    /// Describe an address for humans: `stub _puts (/usr/lib/libSystem.B.dylib)`,
    /// `React _RCTFoo+0x1c`, or `hermesvm+0x1234`. Uses exported symbols only.
    pub fn symbolize(&self, addr: u64) -> String {
        if let Some(stub) = self.stubs.at(addr) {
            return format!("stub {} ({})", stub.symbol, stub.library);
        }
        let Some(img) = self.image_at(addr) else { return format!("{addr:#x}") };
        let nearest = img
            .exports
            .keys()
            .filter_map(|name| img.symbol(name).map(|a| (a, name)))
            .filter(|&(a, _)| a <= addr)
            .max_by_key(|&(a, _)| a);
        match nearest {
            Some((a, name)) => format!("{} {name}+{:#x}", img.name, addr - a),
            None => format!("{}+{:#x}", img.name, addr - img.header),
        }
    }

    pub fn entry(&self) -> Option<u64> {
        self.main().entry
    }
}

/// Load an `.app` bundle directory, or a bare executable.
pub fn load(path: &Path) -> Result<Process> {
    let (bundle_id, exe, app_dir) = locate_executable(path)?;

    // Discover images: the executable, then bundled dylibs breadth-first.
    let mut paths: Vec<PathBuf> = vec![exe];
    let mut files: Vec<Vec<u8>> = Vec::new();
    let mut deps: Vec<Vec<Option<usize>>> = Vec::new();
    let mut queue = VecDeque::from([0usize]);
    let mut by_path: HashMap<PathBuf, usize> = HashMap::from([(paths[0].clone(), 0)]);

    while let Some(i) = queue.pop_front() {
        let bytes = read(&paths[i])?;
        let image = parse(&paths[i], &bytes)?;
        let mut img_deps = Vec::new();
        for d in &image.dylibs {
            if d.kind == DylibKind::System {
                img_deps.push(None);
                continue;
            }
            let Some(file) = resolve_dylib_path(&d.path, &paths[i], &app_dir, &image.rpaths) else {
                if d.weak {
                    img_deps.push(None);
                    continue;
                }
                return Err(LoadError::MissingDylib { image: display_name(&paths[i]), lib: d.path.clone() });
            };
            let next = paths.len();
            let idx = *by_path.entry(file.clone()).or_insert_with(|| {
                paths.push(file);
                queue.push_back(next);
                next
            });
            img_deps.push(Some(idx));
        }
        // Images are discovered in index order, so these pushes stay aligned with `paths`.
        debug_assert_eq!(files.len(), i);
        files.push(bytes);
        deps.push(img_deps);
    }

    let parsed: Vec<Image<'_>> = paths.iter().zip(&files).map(|(p, b)| parse(p, b)).collect::<Result<_>>()?;

    // Map every image.
    let mut memory = Memory::default();
    let mut images = Vec::with_capacity(parsed.len());
    let mut next_base = DYLIB_BASE;
    for (i, img) in parsed.iter().enumerate() {
        let lo = img.mapped_segments().map(|s| s.vmaddr).min().unwrap_or(0);
        let hi = img.mapped_segments().map(|s| s.vmaddr + s.vmsize).max().unwrap_or(0);
        let slide = if i == 0 && img.file_type == FileType::Executable && lo != 0 {
            0 // The main executable loads at its preferred address.
        } else {
            let base = next_base;
            next_base = page_align_up(base + (hi - lo)) + PAGE_SIZE; // one guard page between images
            base.wrapping_sub(lo)
        };
        let name = display_name(&paths[i]);
        for seg in img.mapped_segments() {
            let prot = Prot { read: seg.prot.read, write: seg.prot.write, exec: seg.prot.exec };
            let region = memory.map(seg.vmaddr.wrapping_add(slide), seg.vmsize, prot, format!("{name}:{}", seg.name))?;
            region.data[..seg.data.len()].copy_from_slice(seg.data);
        }
        images.push(LoadedImage {
            name,
            path: paths[i].clone(),
            install_name: img.install_name.clone(),
            file_type: img.file_type,
            slide,
            header: img.header_vmaddr.wrapping_add(slide),
            start: lo.wrapping_add(slide),
            end: hi.wrapping_add(slide),
            entry: img.entry.map(|e| e.wrapping_add(slide)),
            dylibs: img.dylibs.clone(),
            deps: std::mem::take(&mut deps[i]),
            exports: img.exports.iter().map(|e| (e.name.clone(), e.kind.clone())).collect(),
        });
    }

    // Link.
    let mut linker = Linker { images: &images, stubs: Stubs::default(), stats: LinkStats::default(), unresolved: Vec::new() };
    for (i, img) in parsed.iter().enumerate() {
        let slide = images[i].slide;
        for fixup in &img.fixups {
            let value = match *fixup {
                Fixup::Rebase { target, .. } => {
                    linker.stats.rebases += 1;
                    let high8 = target & (0xff << 56);
                    ((target & !(0xff << 56)).wrapping_add(slide)) | high8
                }
                Fixup::Bind { import, addend, .. } => {
                    let imp = &img.imports[import];
                    let base = linker.bind(i, img, imp.ordinal, &imp.name, imp.weak);
                    if base == 0 { 0 } else { base.wrapping_add(imp.addend as u64).wrapping_add(addend as u64) }
                }
            };
            memory.write_u64(fixup.location().wrapping_add(slide), value)?;
        }
    }
    let Linker { stubs, mut stats, unresolved, .. } = linker;

    // Sanity check: every rebased pointer should land inside its image's mapped memory.
    // (A pointer to the very end of a segment, e.g. a section-end marker, also counts.)
    for (i, img) in parsed.iter().enumerate() {
        for f in &img.fixups {
            if let Fixup::Rebase { target, .. } = *f {
                let t = (target & !(0xff << 56)).wrapping_add(images[i].slide);
                if memory.region_of(t).is_none() && memory.region_of(t.wrapping_sub(1)).is_none() {
                    stats.rebases_unmapped += 1;
                }
            }
        }
    }

    // Back every stub slot with a trapping instruction.
    if !stubs.is_empty() {
        let size = page_align_up(stubs.len() as u64 * STUB_SIZE);
        let region = memory.map(STUB_BASE, size, Prot::RX, "[hle stubs]")?;
        for slot in region.data.chunks_exact_mut(STUB_SIZE as usize) {
            slot[..4].copy_from_slice(&STUB_INSN.to_le_bytes());
        }
    }

    Ok(Process { bundle_id, memory, images, stubs, stats, unresolved })
}

struct Linker<'i> {
    images: &'i [LoadedImage],
    stubs: Stubs,
    stats: LinkStats,
    unresolved: Vec<Unresolved>,
}

impl Linker<'_> {
    /// Resolve one import of image `i`. Returns the address to bind (0 for a missing weak import).
    fn bind(&mut self, i: usize, img: &Image<'_>, ordinal: LibOrdinal, name: &str, weak: bool) -> u64 {
        let found = match ordinal {
            LibOrdinal::Dylib(d) => match self.images[i].deps[d] {
                Some(target) => self.lookup(target, name, 0),
                None if img.dylibs[d].kind == DylibKind::System => {
                    Some(self.stubs.get_or_insert(&img.dylibs[d].path, name))
                }
                None => None, // weak bundled dylib that isn't present
            },
            LibOrdinal::SelfImage => self.lookup(i, name, 0),
            LibOrdinal::MainExecutable => self.lookup(0, name, 0),
            // Search every guest image; otherwise it lives in a system library (on iOS
            // typically libc++ for C++ operators/typeinfo), so the HLE layer resolves it by name.
            LibOrdinal::Flat | LibOrdinal::WeakLookup => (0..self.images.len())
                .find_map(|t| self.images[t].symbol(name))
                .or_else(|| Some(self.stubs.get_or_insert(FLAT_NAMESPACE, name))),
        };

        match found {
            Some(addr) if addr >= STUB_BASE && self.stubs.at(addr).is_some() => self.stats.binds_to_stub += 1,
            Some(_) => self.stats.binds_to_guest += 1,
            None if weak => {
                self.stats.weak_missing += 1;
                return 0;
            }
            None => {
                let library = img.library_name(ordinal).to_string();
                self.unresolved.push(Unresolved { image: self.images[i].name.clone(), library: library.clone(), symbol: name.into() });
                self.stats.binds_to_stub += 1;
                return self.stubs.get_or_insert(&library, name);
            }
        }
        found.unwrap()
    }

    /// Find `name` exported by image `t`, following re-exports.
    fn lookup(&mut self, t: usize, name: &str, depth: u32) -> Option<u64> {
        let image = &self.images[t];
        let Some(export) = image.exports.get(name) else {
            // Not exported directly: search libraries this image re-exports.
            if depth >= 8 {
                return None;
            }
            return (0..image.dylibs.len())
                .filter(|&d| image.dylibs[d].reexport)
                .find_map(|d| self.lookup(image.deps[d]?, name, depth + 1));
        };
        match export {
            ExportKind::Reexport { ordinal: LibOrdinal::Dylib(d), name: real } if depth < 8 => {
                let real = real.clone();
                match image.deps[*d] {
                    Some(next) => self.lookup(next, &real, depth + 1),
                    None => Some(self.stubs.get_or_insert(&image.dylibs[*d].path, &real)),
                }
            }
            ExportKind::Reexport { .. } => None,
            _ => image.symbol(name),
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|source| LoadError::Io { path: path.into(), source })
}

fn parse<'a>(path: &Path, bytes: &'a [u8]) -> Result<Image<'a>> {
    Image::parse(bytes).map_err(|source| LoadError::Macho { path: path.into(), source })
}

fn display_name(p: &Path) -> String {
    p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// Returns (bundle id, executable path, app directory).
pub fn locate_executable(path: &Path) -> Result<(Option<String>, PathBuf, PathBuf)> {
    if path.is_file() {
        let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        return Ok((None, path.to_path_buf(), dir));
    }
    let plist_path = path.join("Info.plist");
    let info: plist::Dictionary =
        plist::from_file(&plist_path).map_err(|source| LoadError::Plist { path: plist_path.clone(), source })?;
    let exe = info
        .get("CFBundleExecutable")
        .and_then(|v| v.as_string())
        .ok_or_else(|| LoadError::Bundle(format!("{} has no CFBundleExecutable", plist_path.display())))?;
    let id = info.get("CFBundleIdentifier").and_then(|v| v.as_string()).map(String::from);
    Ok((id, path.join(exe), path.to_path_buf()))
}

/// Turn an install name like `@rpath/React.framework/React` into a file in the bundle.
fn resolve_dylib_path(install: &str, loader: &Path, app_dir: &Path, rpaths: &[String]) -> Option<PathBuf> {
    let loader_dir = loader.parent().unwrap_or(app_dir);
    let expand = |p: &str| -> PathBuf {
        if let Some(rest) = p.strip_prefix("@executable_path/") {
            app_dir.join(rest)
        } else if let Some(rest) = p.strip_prefix("@loader_path/") {
            loader_dir.join(rest)
        } else {
            PathBuf::from(p)
        }
    };
    let candidates: Vec<PathBuf> = if let Some(rest) = install.strip_prefix("@rpath/") {
        rpaths
            .iter()
            .map(|r| expand(r).join(rest))
            // Apps almost always use @executable_path/Frameworks; try it even if the rpath is missing.
            .chain([app_dir.join("Frameworks").join(rest)])
            .collect()
    } else {
        vec![expand(install)]
    };
    candidates.into_iter().find(|c| c.is_file()).map(|c| c.canonicalize().unwrap_or(c))
}
