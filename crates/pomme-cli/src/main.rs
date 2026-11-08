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
    /// Load a .app and execute main() on the arm64 interpreter until it needs the host.
    Run {
        path: PathBuf,
        /// Stop after this many instructions.
        #[arg(long, default_value_t = 50_000_000)]
        steps: u64,
        /// Instead of stopping at the first HLE stub, fake it (return 0) and keep going,
        /// logging every system call. Shows how far the app gets and what it needs.
        #[arg(long)]
        stub_trace: bool,
        /// With --stub-trace: print each call as it happens, not just the summary.
        #[arg(long)]
        verbose: bool,
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
        Cmd::Run { path, steps, stub_trace, verbose } => run(&path, steps, stub_trace, verbose),
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

const STACK_TOP: u64 = 0x3_0000_0000;
const STACK_SIZE: u64 = 8 << 20;
const TLS_BASE: u64 = 0x3_1000_0000;
const TLS_SIZE: u64 = 64 << 10;

/// Push a C string onto the stack image and return its guest address.
fn push_cstr(mem: &mut pomme_loader::mem::Memory, sp: &mut u64, s: &str) -> Result<u64> {
    let bytes = [s.as_bytes(), &[0]].concat();
    *sp -= bytes.len() as u64;
    mem.write(*sp, &bytes)?;
    Ok(*sp)
}

fn run(path: &Path, max_steps: u64, stub_trace: bool, verbose: bool) -> Result<()> {
    use pomme_cpu::{Cpu, Exit, HOST_RETURN};
    use pomme_loader::Prot;

    let mut p = pomme_loader::load(path)?;
    let entry = p.entry().context("no LC_MAIN entry point")?;
    p.memory.map(STACK_TOP - STACK_SIZE, STACK_SIZE, Prot::RW, "[stack]")?;
    p.memory.map(TLS_BASE, TLS_SIZE, Prot::RW, "[tls]")?;

    // main(argc, argv, envp, apple)
    let mut sp = STACK_TOP;
    let exe_name = p.main().name.clone();
    let arg0 = push_cstr(&mut p.memory, &mut sp, &exe_name)?;
    let apple0 = push_cstr(&mut p.memory, &mut sp, &format!("executable_path={exe_name}"))?;
    sp &= !0xf;
    let vectors = [arg0, 0, /* envp */ 0, /* apple */ apple0, 0];
    sp -= vectors.len() as u64 * 8;
    sp &= !0xf;
    for (i, v) in vectors.iter().enumerate() {
        p.memory.write_u64(sp + i as u64 * 8, *v)?;
    }

    let mut cpu = Cpu::new();
    cpu.x[0] = 1;
    cpu.x[1] = sp;
    cpu.x[2] = sp + 16;
    cpu.x[3] = sp + 24;
    cpu.x[30] = HOST_RETURN;
    cpu.sp = sp;
    cpu.pc = entry;
    cpu.tpidrro_el0 = TLS_BASE;

    println!("running {} from {} ({entry:#x})", p.bundle_id.as_deref().unwrap_or(&exe_name), p.symbolize(entry));
    let start = std::time::Instant::now();
    let mut calls: Vec<(String, u64)> = Vec::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();

    let exit = loop {
        let remaining = max_steps.saturating_sub(cpu.steps);
        let exit = cpu.run(&mut p.memory, remaining);
        match exit {
            Exit::Brk { .. } if stub_trace && p.stubs.at(cpu.pc).is_some() => {
                let stub = p.stubs.at(cpu.pc).unwrap();
                if stub.symbol == "_exit" {
                    break None;
                }
                let mut desc = stub.symbol.clone();
                // For message sends, the selector is a C string in x1.
                if stub.symbol.starts_with("_objc_msgSend")
                    && let Ok(sel) = p.memory.read_cstr(cpu.x[1], 128)
                {
                    desc = format!("{} [{sel}]", stub.symbol);
                }
                if verbose {
                    println!("  {:>10}  {desc}  (from {})", cpu.steps, p.symbolize(cpu.x[30].wrapping_sub(4)));
                }
                *counts.entry(desc.clone()).or_default() += 1;
                if calls.len() < 40 {
                    calls.push((desc, cpu.steps));
                }
                // Fake the call: return 0 to the caller.
                cpu.x[0] = 0;
                cpu.x[1] = 0;
                cpu.pc = cpu.x[30];
            }
            other => break Some(other),
        }
    };
    let elapsed = start.elapsed();
    let Some(exit) = exit else {
        println!("\nexit({}) after {} instructions in {:.2?}", cpu.x[0] as i32, cpu.steps, elapsed);
        print_trace(&calls, counts);
        return Ok(());
    };

    println!(
        "\nstopped after {} instructions in {:.2?} ({:.1} MIPS)",
        cpu.steps,
        elapsed,
        cpu.steps as f64 / elapsed.as_secs_f64() / 1e6
    );
    match exit {
        Exit::Brk { imm } => match p.stubs.at(cpu.pc) {
            Some(stub) => println!("reason: called {} from {} — needs HLE", stub.symbol, p.symbolize(cpu.x[30].wrapping_sub(4))),
            None => println!("reason: BRK #{imm:#x} at {}", p.symbolize(cpu.pc)),
        },
        Exit::Unimplemented { insn } => println!("reason: unimplemented instruction {insn:#010x} at {}", p.symbolize(cpu.pc)),
        Exit::Fault { addr } => println!("reason: fault accessing {addr:#x} at {}", p.symbolize(cpu.pc)),
        Exit::Svc { imm } => println!("reason: syscall SVC #{imm:#x} (x16 = {}) at {}", cpu.x[16], p.symbolize(cpu.pc.wrapping_sub(4))),
        Exit::Returned => println!("reason: main returned {}", cpu.x[0] as i32),
        Exit::StepLimit => println!("reason: step limit at {}", p.symbolize(cpu.pc)),
    }
    println!("  pc {:#x}  lr {:#x} ({})  sp {:#x}", cpu.pc, cpu.x[30], p.symbolize(cpu.x[30]), cpu.sp);
    for r in (0..8).step_by(4) {
        println!(
            "  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}",
            r, cpu.x[r], r + 1, cpu.x[r + 1], r + 2, cpu.x[r + 2], r + 3, cpu.x[r + 3]
        );
    }

    if stub_trace {
        print_trace(&calls, counts);
    }
    Ok(())
}

fn print_trace(calls: &[(String, u64)], counts: BTreeMap<String, u64>) {
    println!("\nfirst system calls:");
    for (desc, step) in calls {
        println!("  {step:>10}  {desc}");
    }
    let mut by_count: Vec<_> = counts.into_iter().collect();
    by_count.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!("\n{} distinct system calls (top 25 by count):", by_count.len());
    for (desc, n) in by_count.iter().take(25) {
        println!("  {n:>7}  {desc}");
    }
}
