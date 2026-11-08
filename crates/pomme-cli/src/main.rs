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
    /// Load a .app and run it: main() on the arm64 interpreter, system libraries in HLE.
    Run {
        path: PathBuf,
        /// Stop after this many instructions.
        #[arg(long, default_value_t = 50_000_000)]
        steps: u64,
        /// Stop once the app is idle past this many seconds of virtual time.
        #[arg(long, default_value_t = 10.0)]
        seconds: f64,
        /// Print the view hierarchy when the run stops.
        #[arg(long)]
        views: bool,
        /// Write a PNG of the screen (3 px per point) when the run stops.
        #[arg(long)]
        screenshot: Option<PathBuf>,
        /// Print every system call and Objective-C message as it happens.
        #[arg(long)]
        trace: bool,
        /// Directory of pomme-built system libraries (see scripts/build-sysroot.sh).
        #[arg(long, default_value = "vendor/sysroot")]
        sysroot: PathBuf,
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
        Cmd::Run { path, steps, seconds, views, screenshot, trace, sysroot } => run(&path, steps, seconds, views, screenshot.as_deref(), trace, &sysroot),
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
    let sysroot = Path::new("vendor/sysroot").canonicalize().ok();
    let p = pomme_loader::load_with(path, &pomme_loader::LoadOptions { sysroot })?;
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

/// A hook for POMME_BREAK: print the call, then run the function normally.
fn print_call(emu: &mut pomme_hle::Emu) -> pomme_hle::Result<()> {
    let c = &emu.cpu;
    println!(
        "[break] {} from {}  x0={:#x} x1={:#x} x2={:#x} x3={:#x} d0={}",
        emu.symbolize(emu.hook_target()),
        emu.symbolize(c.x[30].wrapping_sub(4)),
        c.x[0],
        c.x[1],
        c.x[2],
        c.x[3],
        f64::from_bits(c.q[0] as u64)
    );
    emu.continue_original();
    Ok(())
}

fn run(path: &Path, max_steps: u64, seconds: f64, views: bool, screenshot: Option<&Path>, trace: bool, sysroot: &Path) -> Result<()> {
    use pomme_hle::{Emu, Stop};

    let sysroot = &sysroot.canonicalize().unwrap_or_else(|_| sysroot.to_path_buf());
    let opts = pomme_loader::LoadOptions { sysroot: sysroot.is_dir().then(|| sysroot.to_path_buf()) };
    let p = pomme_loader::load_with(path, &opts)?;
    let system: Vec<_> = p.images.iter().filter(|i| i.path.starts_with(sysroot)).map(|i| i.name.as_str()).collect();
    if !system.is_empty() {
        println!("system libraries from {}: {}", sysroot.display(), system.join(", "));
    }
    let mut emu = Emu::new(p).map_err(|e| anyhow::anyhow!("setting up the runtime: {e}"))?;
    emu.max_steps = max_steps;
    emu.time_limit_ns = Some((seconds * 1e9) as u64);
    emu.trace = trace;
    // Debugging aid: POMME_BREAK=_sym1,_sym2 prints the arguments of every call to those guest functions.
    if let Ok(list) = std::env::var("POMME_BREAK") {
        for sym in list.split(',') {
            match emu.p.images.iter().find_map(|i| i.symbol(sym)) {
                Some(addr) => emu.hook(addr, print_call).map_err(|e| anyhow::anyhow!("{e}"))?,
                None => println!("POMME_BREAK: no guest symbol {sym}"),
            }
        }
    }
    println!(
        "running {} ({} classes, {} host functions bound)",
        emu.p.bundle_id.as_deref().unwrap_or(&emu.p.main().name),
        emu.objc.classes.len() / 2,
        emu.bound_functions()
    );

    let start = std::time::Instant::now();
    let result = emu.run_main();
    let elapsed = start.elapsed();
    let steps = emu.cpu.steps;
    let mips = steps as f64 / elapsed.as_secs_f64() / 1e6;
    match result {
        Ok(code) => println!("\nexit({code}) after {steps} instructions in {elapsed:.2?}"),
        Err(stop) => {
            println!("\nstopped after {steps} instructions in {elapsed:.2?} ({mips:.1} MIPS)");
            println!("reason: {stop}");
            match &stop {
                Stop::MissingFunction { caller, .. } | Stop::MissingMethod { caller, .. } => {
                    println!("  called from {}", emu.symbolize(caller.wrapping_sub(4)))
                }
                Stop::Cpu { pc, .. } => println!("  at {}", emu.symbolize(*pc)),
                _ => {}
            }
            let c = &emu.cpu;
            println!("  pc {:#x}  lr {:#x} ({})  sp {:#x}", c.pc, c.x[30], emu.symbolize(c.x[30]), c.sp);
            // Backtrace: arm64 Darwin code keeps a frame-pointer chain of (fp, lr) pairs.
            println!("  backtrace:");
            let mut fp = c.x[29];
            for _ in 0..16 {
                let (Ok(next), Ok(ret)) = (emu.p.memory.read_u64(fp), emu.p.memory.read_u64(fp + 8)) else { break };
                if ret == 0 || ret == pomme_cpu::HOST_RETURN {
                    break;
                }
                println!("    {}", emu.symbolize(ret.wrapping_sub(4)));
                if next <= fp {
                    break;
                }
                fp = next;
            }
            for r in (0..8).step_by(4) {
                println!(
                    "  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}",
                    r, c.x[r], r + 1, c.x[r + 1], r + 2, c.x[r + 2], r + 3, c.x[r + 3]
                );
            }
        }
    }
    let mut calls: Vec<_> = emu.calls.iter().collect();
    calls.sort_by_key(|(name, n)| (std::cmp::Reverse(**n), (*name).clone()));
    println!("\n{} distinct system functions called (top 15):", calls.len());
    for (name, n) in calls.iter().take(15) {
        println!("  {n:>7}  {name}");
    }
    // Lift the limits: describing and drawing the views runs guest code.
    emu.max_steps = u64::MAX;
    emu.time_limit_ns = None;
    if views {
        println!("\nviews:\n{}", emu.describe_views());
    }
    if let Some(out) = screenshot {
        match emu.screenshot_png(3.0) {
            Ok(png) => {
                std::fs::write(out, png)?;
                println!("\nscreenshot: {}", out.display());
            }
            Err(e) => println!("\nscreenshot failed: {e}"),
        }
    }
    Ok(())
}
