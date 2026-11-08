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
pub mod objc;

mod foundation;
mod libc;
mod swift;
mod uikit;

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
    /// Print every system call and message send.
    pub trace: bool,
    /// Count of host calls by name, for reports.
    pub calls: HashMap<String, u64>,
    /// main's argc/argv, for `CommandLine` and `_NSGetArgv`.
    pub argc: u64,
    pub argv: u64,
    /// Guest-call nesting depth (host → guest → host → ...).
    pub depth: usize,
    /// Host-side state for UIKit.
    pub ui: uikit::State,
}

impl Emu {
    /// Set up a loaded process: stack, TLS, heap and the Objective-C runtime.
    pub fn new(mut p: Process) -> Result<Self> {
        p.memory.map(STACK_TOP - STACK_SIZE, STACK_SIZE, Prot::RW, "[stack]")?;
        p.memory.map(TLS_BASE, TLS_SIZE, Prot::RW, "[tls]")?;

        let mut registry: HashMap<&'static str, HostFn> = HashMap::new();
        for table in [objc::FUNCS, foundation::FUNCS, libc::FUNCS, swift::FUNCS, uikit::FUNCS] {
            registry.extend(table.iter().copied());
        }
        objc::register_reg_variants(&mut registry);
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
            trace: false,
            calls: HashMap::new(),
            argc: 0,
            argv: 0,
            depth: 0,
            ui: Default::default(),
        };
        objc::init(&mut emu)?;
        foundation::init(&mut emu)?;
        Ok(emu)
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
    /// Callee-saved registers survive (the callee preserves them); pc and lr
    /// are restored so a host function can call out and then return normally.
    pub fn call(&mut self, addr: u64, args: &[u64]) -> Result<u64> {
        assert!(args.len() <= 8, "stack arguments not supported yet");
        let (pc, lr) = (self.cpu.pc, self.cpu.x[30]);
        for (i, a) in args.iter().enumerate() {
            self.cpu.x[i] = *a;
        }
        self.cpu.x[30] = HOST_RETURN;
        self.cpu.pc = addr;
        self.depth += 1;
        let r = self.run_until_return();
        self.depth -= 1;
        r?;
        self.cpu.pc = pc;
        self.cpu.x[30] = lr;
        Ok(self.cpu.x[0])
    }

    /// Execute guest code until it returns to the host, servicing stub calls.
    fn run_until_return(&mut self) -> Result<()> {
        loop {
            let budget = self.max_steps.saturating_sub(self.cpu.steps);
            match self.cpu.run(&mut self.p.memory, budget) {
                Exit::Returned => return Ok(()),
                Exit::Brk { .. } if self.p.stubs.at(self.cpu.pc).is_some() => self.dispatch()?,
                exit => return Err(Stop::Cpu { exit, pc: self.cpu.pc }),
            }
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
        Ok(self.heap.alloc(&mut self.p.memory, size)?)
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
