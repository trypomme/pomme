//! High-level emulation: pomme's own implementations of the iOS system
//! libraries the guest links against.
//!
//! [`Emu`] owns the CPU and the loaded process. Every system import was bound
//! by the loader to a trapping stub; when the guest calls one, [`Emu`] looks up
//! the host function registered for that symbol and runs it. Host functions
//! read arguments from and write results to guest registers (AAPCS64), and may
//! call back into guest code with [`Emu::call`] — e.g. `UIApplicationMain`
//! calling the app delegate.

pub mod heap;
mod bundle;
mod calayer;
mod cg;
mod constants;
mod attributed;
mod invocation;
pub mod fs;
pub mod objc;

mod foundation;
mod libc;
pub mod libsystem;
pub mod threads;
mod posix;
mod printf;
pub mod render;
mod swift;
mod textkit;
pub mod touch;
mod uikit;
mod uiview;

use heap::Heap;
use objc::Objc;
use pomme_cpu::{Cpu, Exit, HOST_RETURN};
use pomme_loader::{MemError, Process, Prot};
use std::collections::HashMap;
use std::fmt;

pub const STACK_TOP: u64 = 0x3_0000_0000;
pub const STACK_SIZE: u64 = 8 << 20;
pub const TLS_BASE: u64 = 0x3_1000_0000;
pub const TLS_SIZE: u64 = 64 << 10;
/// Guest-callable host functions created at run time (each slot traps with BRK #0x5f).
pub const THUNK_BASE: u64 = 0x6_8000_0000;
const THUNK_SLOTS: u64 = 4096;
const THUNK_INSN: u32 = 0xd420_0000 | (0x5f << 5);
/// Planted over the first instruction of a hooked guest function.
const HOOK_IMM: u16 = 0x60;
const HOOK_INSN: u32 = 0xd420_0000 | ((HOOK_IMM as u32) << 5);
/// Generated guest code (trampolines).
pub const CODE_BASE: u64 = 0x6_9000_0000;
const CODE_SIZE: u64 = 1 << 20;

/// Hook on `__cxa_throw(void *obj, std::type_info *type, void (*dtor)(void *))`.
fn report_cxx_throw(emu: &mut Emu) -> Result<()> {
    let (obj, tinfo) = (emu.cpu.x[0], emu.cpu.x[1]);
    // type_info: {vtable, const char *name}; the name may carry a flag in its top bit.
    let name_ptr = emu.read_u64(tinfo + 8).unwrap_or(0) & 0x7fff_ffff_ffff_ffff;
    let name = emu.cstr(name_ptr).unwrap_or_default();
    // std::exception's vtable: [complete dtor, deleting dtor, what()].
    let what = emu
        .read_u64(obj)
        .and_then(|vt| emu.read_u64(vt + 16))
        .ok()
        .filter(|&f| f != 0 && name.contains("St") || name.contains("error") || name.contains("Error") || name.contains("exception"))
        .and_then(|f| emu.call(f, &[obj]).ok())
        .and_then(|p| emu.cstr(p).ok());
    let thread = emu.th.threads[emu.th.current].name.clone();
    println!("[C++ exception on thread {thread}] {name}: {}", what.as_deref().unwrap_or("(no what())"));
    emu.last_exception = Some(format!("{name}: {}", what.unwrap_or_default()));
    emu.continue_original();
    Ok(())
}

/// A host implementation of a system function.
pub type HostFn = fn(&mut Emu) -> Result<()>;

/// Why emulation stopped.
#[derive(Debug, Clone)]
pub enum Stop {
    /// The guest called a system function pomme doesn't implement yet.
    MissingFunction { symbol: String, library: String, caller: u64 },
    /// A message nobody implements.
    MissingMethod { class: String, selector: String, class_method: bool, caller: u64 },
    /// The CPU stopped: fault, unimplemented instruction, step limit, ...
    Cpu { exit: Exit, pc: u64 },
    /// The guest called `exit()`.
    Exit(i32),
    /// Something host-side isn't supported yet.
    Unsupported(String),
    /// Internal: a guest thread blocked and unwound to the scheduler.
    Blocked,
    /// Every thread is waiting and nothing is scheduled to wake any of them.
    Idle(String),
    /// Virtual time reached [`Emu::time_limit_ns`] with every thread waiting.
    TimeLimit(u64),
}

impl From<MemError> for Stop {
    fn from(e: MemError) -> Self {
        let addr = match e {
            MemError::Unmapped(a) => a,
            MemError::Overlap { base, .. } => base,
        };
        Stop::Cpu { exit: Exit::Fault { addr }, pc: 0 }
    }
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stop::MissingFunction { symbol, library, .. } => write!(f, "needs HLE: {symbol} ({library})"),
            Stop::MissingMethod { class, selector, class_method, .. } => {
                write!(f, "needs HLE: {}[{class} {selector}]", if *class_method { '+' } else { '-' })
            }
            Stop::Cpu { exit, .. } => write!(f, "cpu: {exit:?}"),
            Stop::Exit(code) => write!(f, "exit({code})"),
            Stop::Unsupported(what) => write!(f, "unsupported: {what}"),
            Stop::Blocked => write!(f, "thread blocked"),
            Stop::Idle(what) => write!(f, "idle: {what}"),
            Stop::TimeLimit(ns) => write!(f, "reached the time limit ({:.1} s of virtual time), idle", *ns as f64 / 1e9),
        }
    }
}

pub type Result<T> = std::result::Result<T, Stop>;

pub struct Emu {
    pub cpu: Cpu,
    pub p: Process,
    pub heap: Heap,
    pub objc: Objc,
    /// Host function for each stub, indexed like `p.stubs`.
    handlers: Vec<Option<HostFn>>,
    /// Instruction budget for the whole run.
    pub max_steps: u64,
    /// Stop once virtual time passes this while every thread waits (ns since boot).
    pub time_limit_ns: Option<u64>,
    /// Print every system call and message send.
    pub trace: bool,
    /// Count of host calls by name, for reports.
    pub calls: HashMap<String, u64>,
    /// main's argc/argv, for `CommandLine` and `_NSGetArgv`.
    pub argc: u64,
    pub argv: u64,
    /// Guest-call nesting depth (host → guest → host → ...).
    pub depth: usize,
    /// q0 as the last [`call`](Self::call) returned it (float results).
    pub last_q0: u128,
    /// x0–x1 and v0–v3 as the last [`call_abi`](Self::call_abi) returned them.
    pub last_x01: [u64; 2],
    pub last_v03: [u128; 4],
    /// The most recent C++ exception thrown (type: what).
    pub last_exception: Option<String>,
    /// Debugging aid: heap addresses to report allocations of (POMME_WATCH=0x...,0x...).
    pub watch: Vec<u64>,
    /// Host-side state for UIKit.
    pub ui: uikit::State,
    /// Host-side state for libSystem (errno, TLS, dispatch queue, ...).
    pub sys: libsystem::State,
    thunks: Vec<HostFn>,
    code_next: u64,
    /// Hooked guest functions: entry address → (host function, trampoline that runs the original).
    hooks: HashMap<u64, (HostFn, u64)>,
    /// While a hook runs: where `continue_original` jumps, and the hooked function.
    hook_original: u64,
    hook_target: u64,
    /// Host-side state for Foundation (CF bridge).
    pub fnd: foundation::State,
    /// Guest threads and Mach ports.
    pub th: threads::State,
    /// Open files.
    pub fs: fs::State,
    /// Text layout (font data and advance cache).
    text: textkit::State,
    /// Drawing contexts, paths and what views drew.
    render: render::State,
    /// Touches, gesture recognizers and the input script.
    pub touch: touch::State,
    bundle: bundle::State,
}

impl Emu {
    /// Set up a loaded process: stack, TLS, heap and the Objective-C runtime.
    pub fn new(mut p: Process) -> Result<Self> {
        p.memory.map(STACK_TOP - STACK_SIZE, STACK_SIZE, Prot::RW, "[stack]")?;
        p.memory.map(TLS_BASE, TLS_SIZE, Prot::RW, "[tls]")?;

        let mut registry: HashMap<&'static str, HostFn> = HashMap::new();
        for table in [objc::FUNCS, objc::REFLECT_FUNCS, foundation::FUNCS, foundation::PATH_FUNCS, libc::FUNCS, libsystem::FUNCS, libsystem::MORE_FUNCS, libsystem::WIDE_FUNCS, libsystem::DYLD_FUNCS, posix::FUNCS, fs::FUNCS, printf::FUNCS, constants::FUNCS, swift::FUNCS, uikit::FUNCS, cg::FUNCS, render::FUNCS] {
            registry.extend(table.iter().copied());
        }
        objc::register_reg_variants(&mut registry);
        // Thread-aware versions take precedence over libSystem's simpler ones.
        registry.extend(threads::FUNCS.iter().copied());
        let handlers = p.stubs.iter().map(|(_, s)| registry.get(s.symbol.as_str()).copied()).collect();

        let mut cpu = Cpu::new();
        cpu.sp = STACK_TOP;
        cpu.tpidrro_el0 = TLS_BASE;

        let mut emu = Emu {
            cpu,
            p,
            heap: Heap::new(),
            objc: Objc::default(),
            handlers,
            max_steps: u64::MAX,
            time_limit_ns: None,
            trace: false,
            calls: HashMap::new(),
            argc: 0,
            argv: 0,
            depth: 0,
            last_q0: 0,
            last_x01: [0; 2],
            last_v03: [0; 4],
            last_exception: None,
            watch: std::env::var("POMME_WATCH")
                .map(|v| v.split(',').filter_map(|a| u64::from_str_radix(a.trim().trim_start_matches("0x"), 16).ok()).collect())
                .unwrap_or_default(),
            ui: Default::default(),
            sys: Default::default(),
            thunks: Vec::new(),
            code_next: CODE_BASE,
            hooks: HashMap::new(),
            hook_original: 0,
            hook_target: 0,
            fnd: Default::default(),
            th: Default::default(),
            fs: Default::default(),
            text: Default::default(),
            render: Default::default(),
            touch: Default::default(),
            bundle: Default::default(),
        };
        let thunks = emu.p.memory.map(THUNK_BASE, THUNK_SLOTS * 4, Prot::RX, "[host thunks]")?;
        for slot in thunks.data.chunks_exact_mut(4) {
            slot.copy_from_slice(&THUNK_INSN.to_le_bytes());
        }
        emu.p.memory.map(CODE_BASE, CODE_SIZE, Prot::RX, "[trampolines]")?;
        threads::init(&mut emu);
        objc::init(&mut emu)?;
        // Report every C++ throw (type and what()) before unwinding is attempted.
        if let Some(throw) = emu.p.images.iter().find_map(|i| i.symbol("___cxa_throw")) {
            emu.hook(throw, report_cxx_throw)?;
        }
        foundation::init(&mut emu)?;
        libsystem::init(&mut emu)?;
        Ok(emu)
    }

    /// PNG screenshot of the windows at `scale` pixels per point.
    pub fn screenshot_png(&mut self, scale: f32) -> Result<Vec<u8>> {
        render::screenshot_png(self, scale)
    }

    /// The UIKit window hierarchy as indented text (for reports and tests).
    pub fn describe_views(&mut self) -> String {
        uiview::describe_tree(self).unwrap_or_else(|e| format!("(couldn't describe the views: {e})"))
    }

    /// Images in initialization order: every library before the images that link it.
    fn init_order(&self) -> Vec<usize> {
        fn visit(p: &Process, i: usize, seen: &mut Vec<bool>, out: &mut Vec<usize>) {
            if std::mem::replace(&mut seen[i], true) {
                return;
            }
            for d in p.images[i].deps.iter().flatten() {
                visit(p, *d, seen, out);
            }
            out.push(i);
        }
        let mut seen = vec![false; self.p.images.len()];
        let mut out = Vec::new();
        visit(&self.p, 0, &mut seen, &mut out);
        out
    }

    /// What dyld does before `main`: for each image, dependencies first, run
    /// Objective-C `+load` methods, then C/C++ static initializers.
    pub fn run_initializers(&mut self) -> Result<()> {
        let (argc, argv) = (self.argc, self.argv);
        for i in self.init_order() {
            if let Some(init) = self.p.images[i].symbol("___CFInitialize") {
                self.call(init, &[])?;
                foundation::bridge(self, i)?;
            }
            for list in ["__objc_nlclslist", "__objc_nlcatlist"] {
                let Some((addr, size)) = self.p.images[i].section("__DATA", list) else { continue };
                for at in (addr..addr + size).step_by(8) {
                    let entry = self.read_u64(at)?;
                    // A category's class is at +8; a class is itself.
                    let cls = if list == "__objc_nlcatlist" { self.read_u64(entry + 8)? } else { entry };
                    let meta = self.read_u64(cls)?;
                    objc::call_own_method(self, cls, meta, entry, list == "__objc_nlcatlist", "load")?;
                }
            }
            let img = &self.p.images[i];
            let mut inits = Vec::new();
            if let Some((addr, size)) = img.section("__DATA", "__mod_init_func") {
                for at in (addr..addr + size).step_by(8) {
                    inits.push(self.read_u64(at)?);
                }
            }
            if let Some((addr, size)) = img.section("__TEXT", "__init_offsets") {
                let header = img.header;
                for at in (addr..addr + size).step_by(4) {
                    inits.push(header + self.p.memory.read_u32(at)? as u64);
                }
            }
            for f in inits {
                if self.trace {
                    println!("{:>10} initializer {}", self.cpu.steps, self.symbolize(f));
                }
                self.call(f, &[argc, argv, argv + 16, argv + 24])?;
            }
        }
        Ok(())
    }

    /// Run `main(argc, argv, envp, apple)` to completion.
    pub fn run_main(&mut self) -> Result<i32> {
        let entry = self.p.entry().ok_or(Stop::Unsupported("no LC_MAIN entry point".into()))?;
        let exe = self.p.main().name.clone();
        let mut sp = STACK_TOP;
        let arg0 = self.push_cstr(&mut sp, &exe)?;
        let apple0 = self.push_cstr(&mut sp, &format!("executable_path={exe}"))?;
        let vectors = [arg0, 0, /* envp */ 0, /* apple */ apple0, 0];
        sp = (sp - vectors.len() as u64 * 8) & !0xf;
        for (i, v) in vectors.iter().enumerate() {
            self.p.memory.write_u64(sp + i as u64 * 8, *v)?;
        }
        self.cpu.sp = sp;
        self.argc = 1;
        self.argv = sp;
        match self.run_initializers() {
            Ok(()) => {}
            Err(Stop::Exit(code)) => return Ok(code),
            Err(e) => return Err(e),
        }
        self.cpu.sp = sp;
        match self.call(entry, &[1, sp, sp + 16, sp + 24]) {
            Ok(code) => Ok(code as i32),
            Err(Stop::Exit(code)) => Ok(code),
            Err(e) => Err(e),
        }
    }

    fn push_cstr(&mut self, sp: &mut u64, s: &str) -> Result<u64> {
        *sp -= s.len() as u64 + 1;
        self.p.memory.write(*sp, s.as_bytes())?;
        self.p.memory.write_u8(*sp + s.len() as u64, 0)?;
        Ok(*sp)
    }

    /// Call guest code at `addr` with integer arguments, returning x0.
    ///
    /// The calling host function's registers are preserved: pc, lr, and the
    /// argument and temporary registers (x0–x17, q0–q7), so a host function
    /// can read its own arguments after calling out. The callee's float result
    /// is in [`last_q0`](Self::last_q0).
    pub fn call(&mut self, addr: u64, args: &[u64]) -> Result<u64> {
        assert!(args.len() <= 8, "stack arguments not supported yet");
        let (pc, lr) = (self.cpu.pc, self.cpu.x[30]);
        let mut saved_x = [0u64; 18];
        saved_x.copy_from_slice(&self.cpu.x[..18]);
        let mut saved_q = [0u128; 8];
        saved_q.copy_from_slice(&self.cpu.q[..8]);
        for (i, a) in args.iter().enumerate() {
            self.cpu.x[i] = *a;
        }
        self.cpu.x[30] = HOST_RETURN;
        self.cpu.pc = addr;
        self.depth += 1;
        let r = self.run_until_return();
        self.depth -= 1;
        r?;
        let result = self.cpu.x[0];
        self.last_q0 = self.cpu.q[0];
        self.cpu.x[..18].copy_from_slice(&saved_x);
        self.cpu.q[..8].copy_from_slice(&saved_q);
        self.cpu.pc = pc;
        self.cpu.x[30] = lr;
        Ok(result)
    }

    /// Call guest code with a full AAPCS64 argument layout: integer registers,
    /// vector registers, stack words, and x8 (indirect result). Like
    /// [`call`](Self::call) the caller's registers are preserved; the results
    /// are in [`last_x01`](Self::last_x01) and [`last_v03`](Self::last_v03).
    pub fn call_abi(&mut self, addr: u64, x: &[u64], v: &[u128], stack: &[u64], x8: Option<u64>) -> Result<()> {
        let (pc, lr, sp) = (self.cpu.pc, self.cpu.x[30], self.cpu.sp);
        let mut saved_x = [0u64; 18];
        saved_x.copy_from_slice(&self.cpu.x[..18]);
        let mut saved_q = [0u128; 8];
        saved_q.copy_from_slice(&self.cpu.q[..8]);
        for (i, a) in x.iter().take(8).enumerate() {
            self.cpu.x[i] = *a;
        }
        for (i, a) in v.iter().take(8).enumerate() {
            self.cpu.q[i] = *a;
        }
        if let Some(r) = x8 {
            self.cpu.x[8] = r;
        }
        // Stack arguments at the new sp, 16-byte aligned.
        let new_sp = (sp - (stack.len() as u64 * 8).div_ceil(16) * 16 - 16) & !0xf;
        for (i, w) in stack.iter().enumerate() {
            self.write_u64(new_sp + i as u64 * 8, *w)?;
        }
        self.cpu.sp = new_sp;
        self.cpu.x[30] = HOST_RETURN;
        self.cpu.pc = addr;
        self.depth += 1;
        let r = self.run_until_return();
        self.depth -= 1;
        r?;
        self.last_x01 = [self.cpu.x[0], self.cpu.x[1]];
        self.last_v03 = [self.cpu.q[0], self.cpu.q[1], self.cpu.q[2], self.cpu.q[3]];
        self.cpu.x[..18].copy_from_slice(&saved_x);
        self.cpu.q[..8].copy_from_slice(&saved_q);
        self.cpu.sp = sp;
        self.cpu.pc = pc;
        self.cpu.x[30] = lr;
        Ok(())
    }

    /// Execute guest code until it returns to the host, servicing stub calls.
    fn run_until_return(&mut self) -> Result<()> {
        loop {
            let budget = self.max_steps.saturating_sub(self.cpu.steps);
            match self.cpu.run(&mut self.p.memory, budget) {
                Exit::Returned => return Ok(()),
                exit => self.service_trap(exit)?,
            }
        }
    }

    /// Handle a CPU exit that needs the host: a stub call, a hooked function,
    /// or a host thunk. Anything else is a stop.
    pub(crate) fn service_trap(&mut self, exit: Exit) -> Result<()> {
        match exit {
            Exit::Brk { .. } if self.p.stubs.at(self.cpu.pc).is_some() => self.dispatch(),
            Exit::Brk { imm: HOOK_IMM } if self.hooks.contains_key(&self.cpu.pc) => {
                let (f, original) = self.hooks[&self.cpu.pc];
                self.hook_original = original;
                self.hook_target = self.cpu.pc;
                self.cpu.pc = self.cpu.x[30];
                f(self)
            }
            Exit::Brk { imm: 0x5f } if (THUNK_BASE..THUNK_BASE + THUNK_SLOTS * 4).contains(&self.cpu.pc) => {
                let f = self.thunks[((self.cpu.pc - THUNK_BASE) / 4) as usize];
                self.cpu.pc = self.cpu.x[30];
                f(self)
            }
            exit => Err(Stop::Cpu { exit, pc: self.cpu.pc }),
        }
    }

    /// Run the host function for the stub at pc, then return to the caller (lr)
    /// unless the handler redirected pc (a tail call, as `objc_msgSend` does).
    fn dispatch(&mut self) -> Result<()> {
        let idx = self.p.stubs.index_of(self.cpu.pc).unwrap();
        let caller = self.cpu.x[30];
        let Some(f) = self.handlers[idx] else {
            let stub = self.p.stubs.at(self.cpu.pc).unwrap();
            return Err(Stop::MissingFunction { symbol: stub.symbol.clone(), library: stub.library.clone(), caller });
        };
        if self.trace {
            let name = &self.p.stubs.at(self.cpu.pc).unwrap().symbol;
            if !name.starts_with("_objc_msgSend") {
                println!("{:>10} {:indent$}{name}", self.cpu.steps, "", indent = self.depth * 2);
            }
        }
        *self.calls.entry(self.p.stubs.at(self.cpu.pc).unwrap().symbol.clone()).or_default() += 1;
        self.cpu.pc = caller;
        f(self)
    }

    /// A guest function pointer that runs `f` (same calling convention as a stub).
    pub fn host_thunk(&mut self, f: HostFn) -> Result<u64> {
        if let Some(i) = self.thunks.iter().position(|&g| std::ptr::fn_addr_eq(g, f)) {
            return Ok(THUNK_BASE + i as u64 * 4);
        }
        if self.thunks.len() as u64 >= THUNK_SLOTS {
            return Err(Stop::Unsupported("out of host thunk slots".into()));
        }
        self.thunks.push(f);
        Ok(THUNK_BASE + (self.thunks.len() as u64 - 1) * 4)
    }

    /// Hook the guest function at `addr`: calls to it run `f` instead, which
    /// returns to the caller like a stub, or calls [`continue_original`](Self::continue_original)
    /// to run the real function with the arguments untouched.
    pub fn hook(&mut self, addr: u64, f: HostFn) -> Result<()> {
        if self.hooks.contains_key(&addr) {
            return Ok(());
        }
        let insn = self.p.memory.read_u32(addr)?;
        let resume = addr + 4;
        let sext = |v: u32, bits: u32| ((v << (32 - bits)) as i32 >> (32 - bits)) as i64;
        // Conditional branches are relocated: the copy branches to a long jump to the original target.
        let cond = if insn & 0x7e00_0000 == 0x3400_0000 || insn & 0xff00_0010 == 0x5400_0000 {
            // CBZ/CBNZ, B.cond: imm19 at [23:5]
            Some((addr.wrapping_add((sext((insn >> 5) & 0x7ffff, 19) * 4) as u64), (insn & !(0x7ffff << 5)) | (4 << 5)))
        } else if insn & 0x7e00_0000 == 0x3600_0000 {
            // TBZ/TBNZ: imm14 at [18:5]
            Some((addr.wrapping_add((sext((insn >> 5) & 0x3fff, 14) * 4) as u64), (insn & !(0x3fff << 5)) | (4 << 5)))
        } else {
            None
        };
        let other_pc_relative = insn & 0x1f00_0000 == 0x1000_0000 // ADR/ADRP
            || insn & 0x7c00_0000 == 0x1400_0000 // B/BL
            || insn & 0x3b00_0000 == 0x1800_0000; // LDR literal
        if other_pc_relative {
            return Err(Stop::Unsupported(format!("can't hook {}: first instruction {insn:#010x} is PC-relative", self.symbolize(addr))));
        }
        const LDR_X16: u32 = 0x5800_0010; // ldr x16, #imm19*4
        const BR_X16: u32 = 0xd61f_0200;
        const NOP: u32 = 0xd503_201f;
        let original = match cond {
            None => self.emit_code(&[
                insn,                  // the displaced first instruction
                LDR_X16 | (3 << 5),    // ldr x16, resume
                BR_X16,                // (x16 is scratch at function entry)
                NOP,                   // keeps the literal 8-byte aligned
                resume as u32,
                (resume >> 32) as u32,
            ])?,
            Some((target, branch)) => self.emit_code(&[
                branch,             // 0: the condition, retargeted to word 4
                LDR_X16 | (7 << 5), // 1: not taken: ldr x16, resume (word 8)
                BR_X16,             // 2
                NOP,                // 3
                LDR_X16 | (6 << 5), // 4: taken: ldr x16, target (word 10)
                BR_X16,             // 5
                NOP,                // 6
                NOP,                // 7
                resume as u32,      // 8
                (resume >> 32) as u32,
                target as u32,      // 10
                (target >> 32) as u32,
            ])?,
        };
        self.p.memory.write_u32(addr, HOOK_INSN)?;
        self.hooks.insert(addr, (f, original));
        Ok(())
    }

    /// From inside a hook: the hooked function's address.
    pub fn hook_target(&self) -> u64 {
        self.hook_target
    }

    /// From inside a hook: run the hooked function's original code.
    pub fn continue_original(&mut self) {
        self.cpu.pc = self.hook_original;
    }

    /// The host function behind a thunk address, if it is one.
    pub fn thunk_at(&self, addr: u64) -> Option<HostFn> {
        let off = addr.checked_sub(THUNK_BASE)?;
        self.thunks.get((off / 4) as usize).copied().filter(|_| off % 4 == 0)
    }

    /// Place generated arm64 code (plus inline data) in guest memory; returns its address.
    pub fn emit_code(&mut self, words: &[u32]) -> Result<u64> {
        let at = self.code_next;
        let len = words.len() as u64 * 4;
        if at + len > CODE_BASE + CODE_SIZE {
            return Err(Stop::Unsupported("trampoline space exhausted".into()));
        }
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.p.memory.write(at, &bytes)?;
        self.code_next = (at + len + 15) & !15;
        Ok(at)
    }

    /// Whether stub `i` has a host implementation.
    pub fn has_handler(&self, i: usize) -> bool {
        self.handlers.get(i).is_some_and(|h| h.is_some())
    }

    /// How many stubs have a host implementation.
    pub fn bound_functions(&self) -> usize {
        self.handlers.iter().filter(|h| h.is_some()).count()
    }

    /// Return a value from a host function.
    pub fn ret(&mut self, v: u64) {
        self.cpu.x[0] = v;
    }

    pub fn arg(&self, i: usize) -> u64 {
        self.cpu.x[i]
    }

    pub fn cstr(&self, addr: u64) -> Result<String> {
        Ok(self.p.memory.read_cstr(addr, 1 << 16)?)
    }

    pub fn alloc(&mut self, size: u64) -> Result<u64> {
        let a = self.heap.alloc(&mut self.p.memory, size)?;
        if self.watch.contains(&a) {
            let thread = self.th.threads.get(self.th.current).map_or(String::new(), |t| t.name.clone());
            println!("[watch] {a:#x} allocated ({size} bytes) on thread {thread}, called from {}", self.symbolize(self.cpu.x[30].wrapping_sub(4)));
        }
        Ok(a)
    }

    pub fn alloc_cstr(&mut self, s: &str) -> Result<u64> {
        Ok(self.heap.cstr(&mut self.p.memory, s)?)
    }

    pub fn read_u64(&self, addr: u64) -> Result<u64> {
        Ok(self.p.memory.read_u64(addr)?)
    }

    pub fn write_u64(&mut self, addr: u64, v: u64) -> Result<()> {
        Ok(self.p.memory.write_u64(addr, v)?)
    }

    /// Describe a guest address for humans.
    pub fn symbolize(&self, addr: u64) -> String {
        self.p.symbolize(addr)
    }
}
