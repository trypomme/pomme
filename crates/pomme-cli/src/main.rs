use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use pomme_macho::{DylibKind, Summary};
use std::collections::BTreeMap;
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
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Inspect { path, symbols } => inspect(&path, symbols),
    }
}

struct Bundle {
    id: Option<String>,
    executable: PathBuf,
    frameworks: Vec<PathBuf>,
}

fn open_bundle(path: &Path) -> Result<Bundle> {
    if path.is_file() {
        return Ok(Bundle { id: None, executable: path.into(), frameworks: vec![] });
    }
    let info: plist::Dictionary = plist::from_file(path.join("Info.plist"))
        .with_context(|| format!("reading {}/Info.plist", path.display()))?;
    let exe = info
        .get("CFBundleExecutable")
        .and_then(|v| v.as_string())
        .context("Info.plist has no CFBundleExecutable")?;
    let id = info.get("CFBundleIdentifier").and_then(|v| v.as_string()).map(String::from);

    let mut frameworks = vec![];
    if let Ok(rd) = std::fs::read_dir(path.join("Frameworks")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "framework") {
                let name = p.file_stem().unwrap().to_owned();
                frameworks.push(p.join(name));
            } else if p.extension().is_some_and(|x| x == "dylib") {
                frameworks.push(p);
            }
        }
    }
    frameworks.sort();
    Ok(Bundle { id, executable: path.join(exe), frameworks })
}

fn inspect(path: &Path, show_symbols: bool) -> Result<()> {
    let bundle = open_bundle(path)?;
    if let Some(id) = &bundle.id {
        println!("bundle: {id}");
    }

    let mut images = vec![bundle.executable.clone()];
    images.extend(bundle.frameworks.iter().cloned());

    // Symbols each system library must provide, across every image in the app.
    let mut system_surface: BTreeMap<String, BTreeMap<String, ()>> = BTreeMap::new();

    for img in &images {
        let bytes = std::fs::read(img).with_context(|| format!("reading {}", img.display()))?;
        let s = pomme_macho::inspect(&bytes).with_context(|| format!("parsing {}", img.display()))?;
        print_image(img, &s, show_symbols);
        for d in s.dylibs.iter().filter(|d| d.kind == DylibKind::System) {
            system_surface.entry(d.path.clone()).or_default();
        }
        for imp in &s.imports {
            if imp.library.starts_with('/') {
                system_surface
                    .entry(imp.library.clone())
                    .or_default()
                    .insert(imp.name.clone(), ());
            }
        }
    }

    if images.is_empty() {
        bail!("nothing to inspect");
    }
    let total: usize = system_surface.values().map(|s| s.len()).sum();
    println!("\n== HLE surface: system libraries pomme must provide ==");
    println!("{} libraries, {} distinct symbols", system_surface.len(), total);
    let mut libs: Vec<_> = system_surface.iter().collect();
    libs.sort_by_key(|(_, syms)| std::cmp::Reverse(syms.len()));
    for (lib, syms) in libs {
        println!("  {:>5}  {lib}", syms.len());
    }
    Ok(())
}

fn print_image(path: &Path, s: &Summary, show_symbols: bool) {
    println!("\n== {} ==", path.file_name().unwrap().to_string_lossy());
    print!("  {} {}", s.cpu, s.file_type);
    if let Some(b) = &s.build {
        print!(", {:?} min {} sdk {}", b.platform, b.min_os, b.sdk);
    }
    println!(
        ", entry {:#x}, fixups: {}",
        s.entry,
        if s.uses_chained_fixups { "chained" } else { "dyld_info" }
    );
    for (name, addr, size) in &s.segments {
        println!("  seg {name:<16} {addr:#012x} +{size:#x}");
    }
    let by_lib = s.imports_by_library();
    for d in &s.dylibs {
        let n = by_lib.get(d.path.as_str()).map_or(0, |v| v.len());
        let tag = match d.kind {
            DylibKind::System => "sys",
            DylibKind::Bundled => "app",
        };
        println!("  [{tag}] {n:>5} imports  {}{}", d.path, if d.weak { " (weak)" } else { "" });
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
