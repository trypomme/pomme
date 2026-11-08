use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pomme_macho::{DylibKind, Image, LibOrdinal};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "pomme", about = "An emulator for running iOS apps elsewhere")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inspect a .app bundle (or bare Mach-O) and report what it needs from iOS.
    Inspect {
        path: PathBuf,
        /// List every imported symbol, not just per-library counts.
        #[arg(long)]
        symbols: bool,
    },
    /// Load and link a .app into guest memory, and report the result.
    Load {
        path: PathBuf,
        /// List every HLE stub that was bound.
        #[arg(long)]
        stubs: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Inspect { path, symbols } => inspect(&path, symbols),
        Cmd::Load { path, stubs } => load(&path, stubs),
    }
}

fn bundle_images(path: &Path) -> Result<(Option<String>, Vec<PathBuf>)> {
    let (id, exe, app_dir) = pomme_loader::locate_executable(path)?;
    let mut images = vec![exe];
    let mut frameworks = vec![];
    if let Ok(rd) = std::fs::read_dir(app_dir.join("Frameworks")) {
        for p in rd.flatten().map(|e| e.path()) {
            match p.extension().and_then(|x| x.to_str()) {
                Some("framework") => frameworks.push(p.join(p.file_stem().unwrap())),
                Some("dylib") => frameworks.push(p),
                _ => {}
            }
        }
    }
    frameworks.sort();
    images.extend(frameworks);
    Ok((id, images))
}

fn inspect(path: &Path, show_symbols: bool) -> Result<()> {
    let (id, images) = bundle_images(path)?;
    if let Some(id) = id {
        println!("bundle: {id}");
    }

    // Symbols each system library must provide, across every image in the app.
    let mut surface: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for path in &images {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let img = Image::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
        print_image(path, &img, show_symbols);
        for d in img.dylibs.iter().filter(|d| d.kind == DylibKind::System) {
            surface.entry(d.path.clone()).or_default();
        }
        for imp in &img.imports {
            if let LibOrdinal::Dylib(d) = imp.ordinal
                && img.dylibs[d].kind == DylibKind::System
            {
                surface.entry(img.dylibs[d].path.clone()).or_default().insert(imp.name.clone());
            }
        }
    }

    let total: usize = surface.values().map(|s| s.len()).sum();
    println!("\n== HLE surface: system libraries pomme must provide ==");
    println!("{} libraries, {} distinct symbols", surface.len(), total);
    let mut libs: Vec<_> = surface.iter().collect();
    libs.sort_by_key(|(_, syms)| std::cmp::Reverse(syms.len()));
    for (lib, syms) in libs {
        println!("  {:>5}  {lib}", syms.len());
    }
    Ok(())
}

fn print_image(path: &Path, img: &Image<'_>, show_symbols: bool) {
    println!("\n== {} ==", path.file_name().unwrap().to_string_lossy());
    print!("  {} {:?}", img.cpu, img.file_type);
    if let Some(b) = &img.build {
        print!(", {:?} min {} sdk {}", b.platform, b.min_os, b.sdk);
    }
    println!(
        ", {} fixups ({}), {} exports",
        img.fixups.len(),
        if img.uses_chained_fixups { "chained" } else { "dyld_info" },
        img.exports.len()
    );
    for s in &img.segments {
        println!("  seg {:<16} {:#012x} +{:#x} {}", s.name, s.vmaddr, s.vmsize, s.prot);
    }
    let by_lib = img.imports_by_library();
    for d in &img.dylibs {
        let n = by_lib.get(d.path.as_str()).map_or(0, |v| v.len());
        let tag = match d.kind {
            DylibKind::System => "sys",
            DylibKind::Bundled => "app",
        };
        let flags = match (d.weak, d.reexport) {
            (true, _) => " (weak)",
            (_, true) => " (re-export)",
            _ => "",
        };
        println!("  [{tag}] {n:>5} imports  {}{flags}", d.path);
    }
    if show_symbols {
        for (lib, imps) in &by_lib {
            println!("  -- {lib}");
            for i in imps {
                println!("       {}{}", i.name, if i.weak { " (weak)" } else { "" });
            }
        }
    }
}

fn load(path: &Path, show_stubs: bool) -> Result<()> {
    let start = std::time::Instant::now();
    let p = pomme_loader::load(path)?;
    let elapsed = start.elapsed();

    if let Some(id) = &p.bundle_id {
        println!("bundle: {id}");
    }
    println!("loaded {} images in {:.1?}:", p.images.len(), elapsed);
    for img in &p.images {
        println!("  {:#012x}-{:#012x}  {}", img.start, img.end, img.name);
    }
    if let Some(entry) = p.entry() {
        let insn = p.memory.read_u32(entry)?;
        println!("entry (main): {entry:#x}  first insn {insn:#010x}");
    }
    println!("mapped {:.1} MiB in {} regions", p.memory.mapped_bytes() as f64 / (1 << 20) as f64, p.memory.regions().len());

    let s = &p.stats;
    println!(
        "fixups: {} rebases ({} outside mapped memory), {} binds to guest code, {} binds to {} HLE stubs, {} missing weak",
        s.rebases, s.rebases_unmapped, s.binds_to_guest, s.binds_to_stub, p.stubs.len(), s.weak_missing
    );

    if !p.unresolved.is_empty() {
        println!("\n{} unresolved imports from bundled libraries:", p.unresolved.len());
        for u in p.unresolved.iter().take(20) {
            println!("  {} -> {} ({})", u.image, u.symbol, u.library);
        }
    }
    if show_stubs {
        println!("\nstubs:");
        for (addr, stub) in p.stubs.iter() {
            println!("  {addr:#x}  {}  {}", stub.symbol, stub.library);
        }
    }
    Ok(())
}
