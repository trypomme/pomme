//! Guest threads, deterministically scheduled on one host thread.
//!
//! Each guest thread has its own registers, stack, TLS area and pthread
//! thread-specific data. Only one runs at a time. Threads switch
//! cooperatively — when the running thread blocks (a run-loop wait in
//! `mach_msg`, a semaphore, a condition variable, a join, a sleep) — and a
//! thread that runs a long time without blocking is preempted after a fixed
//! number of instructions. Both are functions of the program alone, so a run
//! is reproducible instruction for instruction.
//!
//! Blocking, from inside a host function:
//! - A thread that blocked straight out of guest code (the usual case: guest
//!   code called a trapping stub) *unwinds*: the host function returns
//!   [`Stop::Blocked`], the scheduler keeps the thread's registers, and later
//!   completes the call (sets x0; pc is already the return address).
//! - A thread that blocked deeper in host code (a host function that called
//!   back into the guest, which then blocked) can't unwind without losing the
//!   host frames, so it *waits in place*: it runs the other threads from where
//!   it is until its condition holds. This is also how the main thread waits,
//!   since `UIApplicationMain` is a host function.
//!
//! When every thread is waiting, virtual time skips ahead to the earliest
//! deadline (timers, timeouts). If there is none, the program is idle.

use crate::{Emu, HostFn, Result, Stop};
use pomme_cpu::{Cpu, Exit, HOST_RETURN};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Instructions a thread may run before another runnable thread gets a turn.
const QUANTUM: u64 = 2_000_000;
const STACK: u64 = 1 << 20;
const TLS: u64 = 4096;
/// pthread_t for the main thread (any stable non-null value).
pub const MAIN_THREAD: u64 = 0x3_2000_0000;
/// Mach ticks are 1/24 µs.
const NS_PER_TICK_NUM: u64 = 125;
const NS_PER_TICK_DEN: u64 = 3;

const MACH_SEND_MSG: u64 = 1;
const MACH_RCV_MSG: u64 = 2;
const MACH_RCV_TIMEOUT: u64 = 0x100;
const MACH_RCV_TIMED_OUT: u64 = 0x1000_4003;
const ETIMEDOUT: u64 = 60;

#[derive(Debug, Clone)]
pub enum Wait {
    Semaphore { sem: u64, deadline: Option<u64> },
    Cond { cond: u64, generation: u64, deadline: Option<u64> },
    MachReceive { name: u32, buf: u64, size: u64, deadline: Option<u64> },
    Join { tid: usize, out: u64 },
    Sleep { deadline: u64 },
    /// Until a 32-bit flag in guest memory becomes non-zero.
    Flag { addr: u64 },
}

impl Wait {
    fn deadline(&self) -> Option<u64> {
        match *self {
            Wait::Semaphore { deadline, .. } | Wait::Cond { deadline, .. } | Wait::MachReceive { deadline, .. } => deadline,
            Wait::Sleep { deadline } => Some(deadline),
            Wait::Join { .. } | Wait::Flag { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
enum Run {
    Runnable,
    /// Unwound; the scheduler completes the call when the wait is over.
    Blocked(Wait),
    /// Waiting in place further up the host stack; only it can resume itself.
    Pinned,
    Done(u64),
}

pub struct Thread {
    /// Saved registers while another thread runs (the running thread's are in `emu.cpu`).
    cpu: Cpu,
    run: Run,
    pub handle: u64,
    pub name: String,
    tsd: HashMap<u64, u64>,
    /// `emu.depth` when its current slice started: blocking at that depth can unwind.
    base_depth: usize,
    /// Top (highest address) and size of its stack.
    stack_top: u64,
    stack_size: u64,
}

#[derive(Default)]
struct Port {
    queue: VecDeque<Vec<u8>>,
    /// The port sets this port belongs to (a port can be in several, as on XNU:
    /// CoreFoundation puts each run loop's wakeup port in every mode's set).
    sets: Vec<u32>,
    is_set: bool,
    /// For an mk_timer port: when it fires (ns).
    timer: Option<u64>,
}

#[derive(Default)]
pub struct State {
    pub threads: Vec<Thread>,
    pub current: usize,
    ports: BTreeMap<u32, Port>,
    next_port: u32,
    conds: HashMap<u64, u64>,
    next_key: u64,
    /// Port the main dispatch queue signals (`_dispatch_get_main_queue_port_4CF`).
    pub main_queue_port: u32,
    handles: HashMap<u64, usize>,
    /// NSThread object for each thread that has one.
    nsthreads: HashMap<usize, u64>,
}

pub(crate) fn init(emu: &mut Emu) {
    emu.th.threads.push(Thread {
        cpu: Cpu::new(),
        run: Run::Runnable,
        handle: MAIN_THREAD,
        name: "main".into(),
        tsd: HashMap::new(),
        base_depth: 0,
        stack_top: crate::STACK_TOP,
        stack_size: crate::STACK_SIZE,
    });
    emu.th.handles.insert(MAIN_THREAD, 0);
    emu.th.next_port = 0x1003;
}

// ---- time ----

pub fn now_ticks(emu: &Emu) -> u64 {
    emu.cpu.ticks()
}

pub fn now_ns(emu: &Emu) -> u64 {
    now_ticks(emu) * NS_PER_TICK_NUM / NS_PER_TICK_DEN
}

fn ns_to_ticks(ns: u64) -> u64 {
    ns.saturating_mul(NS_PER_TICK_DEN).div_ceil(NS_PER_TICK_NUM)
}

// ---- switching ----

/// The registers of thread `tid`, wherever they are.
fn cpu_of(emu: &mut Emu, tid: usize) -> &mut Cpu {
    if tid == emu.th.current { &mut emu.cpu } else { &mut emu.th.threads[tid].cpu }
}

/// Make `tid` the running thread. The clock carries over.
fn switch_to(emu: &mut Emu, tid: usize) {
    let cur = emu.th.current;
    if cur == tid {
        return;
    }
    let (steps, offset) = (emu.cpu.steps, emu.cpu.clock_offset);
    let next = std::mem::take(&mut emu.th.threads[tid].cpu);
    emu.th.threads[cur].cpu = std::mem::replace(&mut emu.cpu, next);
    emu.cpu.steps = steps;
    emu.cpu.clock_offset = offset;
    emu.th.current = tid;
}

/// Create a thread that starts at `pc` with `x0 = arg` and returns to the host.
pub fn spawn(emu: &mut Emu, pc: u64, arg: u64, name: &str) -> Result<usize> {
    let stack = emu.alloc(STACK)?;
    let tls = emu.alloc(TLS)?;
    let handle = emu.alloc(64)?;
    let mut cpu = Cpu::new();
    cpu.pc = pc;
    cpu.x[0] = arg;
    cpu.x[30] = HOST_RETURN;
    cpu.sp = (stack + STACK) & !0xf;
    cpu.tpidrro_el0 = tls;
    let tid = emu.th.threads.len();
    let stack_top = (stack + STACK) & !0xf;
    emu.th.threads.push(Thread {
        cpu,
        run: Run::Runnable,
        handle,
        name: name.into(),
        tsd: HashMap::new(),
        base_depth: 0,
        stack_top,
        stack_size: stack_top - stack,
    });
    emu.th.handles.insert(handle, tid);
    if emu.trace {
        println!("{:>10} thread {tid} created at {}", emu.cpu.steps, emu.symbolize(pc));
    }
    Ok(tid)
}

/// Run thread `tid` until it blocks, finishes, or uses up its quantum.
fn run_slice(emu: &mut Emu, tid: usize) -> Result<()> {
    let me = emu.th.current;
    switch_to(emu, tid);
    emu.th.threads[tid].base_depth = emu.depth;
    let limit = emu.cpu.steps + QUANTUM;
    let result = loop {
        let budget = limit.saturating_sub(emu.cpu.steps).min(emu.max_steps.saturating_sub(emu.cpu.steps));
        match emu.cpu.run(&mut emu.p.memory, budget) {
            Exit::Returned => {
                emu.th.threads[tid].run = Run::Done(emu.cpu.x[0]);
                if emu.trace {
                    println!("{:>10} thread {tid} finished", emu.cpu.steps);
                }
                break Ok(());
            }
            Exit::StepLimit if emu.cpu.steps >= emu.max_steps => break Err(Stop::Cpu { exit: Exit::StepLimit, pc: emu.cpu.pc }),
            Exit::StepLimit => break Ok(()), // preempted; still runnable
            exit => match emu.service_trap(exit) {
                Ok(()) => {}
                Err(Stop::Blocked) => break Ok(()),
                Err(e) => break Err(e),
            },
        }
    };
    // A fatal stop leaves the failing thread current, so the report shows its
    // registers and backtrace (the run is over anyway).
    if result.is_ok() {
        switch_to(emu, me);
    }
    result
}

/// The running thread's index and name (for reports).
pub fn current_thread(emu: &Emu) -> (usize, String) {
    let t = emu.th.current;
    (t, emu.th.threads[t].name.clone())
}

/// Can thread `tid` stop waiting? If so, perform the wait's effects on its
/// registers/memory and return the call's result.
fn try_complete(emu: &mut Emu, wait: &Wait) -> Result<Option<u64>> {
    let now = now_ns(emu);
    let timed_out = |d: Option<u64>| d.is_some_and(|d| now >= d);
    Ok(match *wait {
        Wait::Semaphore { sem, deadline } => {
            let v = emu.read_u64(sem + 8)?;
            if v > 0 {
                emu.write_u64(sem + 8, v - 1)?;
                Some(0)
            } else if timed_out(deadline) {
                Some(1) // non-zero: timed out
            } else {
                None
            }
        }
        Wait::Cond { cond, generation, deadline } => {
            if emu.th.conds.get(&cond).copied().unwrap_or(0) != generation {
                Some(0)
            } else if timed_out(deadline) {
                Some(ETIMEDOUT)
            } else {
                None
            }
        }
        Wait::MachReceive { name, buf, size, deadline } => match take_message(emu, name) {
            Some((port, msg)) => {
                let n = msg.len().min(size as usize);
                emu.p.memory.write(buf, &msg[..n])?;
                emu.p.memory.write_u32(buf + 4, n as u32)?; // msgh_size
                emu.p.memory.write_u32(buf + 12, port)?; // msgh_local_port: who received it
                emu.p.memory.write_u32(buf + 8, 0)?; // no reply port
                Some(0)
            }
            None if timed_out(deadline) => Some(MACH_RCV_TIMED_OUT),
            None => None,
        },
        Wait::Join { tid, out } => match emu.th.threads.get(tid).map(|t| t.run.clone()) {
            Some(Run::Done(v)) => {
                if out != 0 {
                    emu.write_u64(out, v)?;
                }
                Some(0)
            }
            _ => None,
        },
        Wait::Sleep { deadline } => (now >= deadline).then_some(0),
        Wait::Flag { addr } => (emu.p.memory.read_u32(addr)? != 0).then_some(0),
    })
}

/// Give every other thread that can run one slice. Returns whether any ran.
fn schedule_round(emu: &mut Emu) -> Result<bool> {
    fire_timers(emu);
    let me = emu.th.current;
    let mut progressed = false;
    for tid in 0..emu.th.threads.len() {
        if tid == me {
            continue;
        }
        match emu.th.threads[tid].run.clone() {
            Run::Runnable => {}
            Run::Blocked(w) => {
                // Complete the call against that thread's registers.
                switch_to(emu, tid);
                let done = try_complete(emu, &w);
                switch_to(emu, me);
                match done? {
                    Some(r) => {
                        cpu_of(emu, tid).x[0] = r;
                        emu.th.threads[tid].run = Run::Runnable;
                    }
                    None => continue,
                }
            }
            Run::Pinned | Run::Done(_) => continue,
        }
        run_slice(emu, tid)?;
        progressed = true;
    }
    Ok(progressed)
}

/// Fire due mk_timers: each sends a message to its port.
fn fire_timers(emu: &mut Emu) {
    let now = now_ns(emu);
    for (&name, port) in emu.th.ports.iter_mut() {
        if port.timer.is_some_and(|t| now >= t) {
            port.timer = None;
            port.queue.push_back(message(name, 0));
        }
    }
}

/// When nothing can run: skip virtual time to the earliest deadline. False if
/// there is none; [`Stop::TimeLimit`] if it's past the run's time limit.
fn advance_time(emu: &mut Emu, own: Option<u64>) -> Result<bool> {
    let next = next_deadline(emu, own);
    let Some(target) = next else { return Ok(false) };
    if let Some(limit) = emu.time_limit_ns
        && target > limit
    {
        return Err(Stop::TimeLimit(limit));
    }
    advance_to(emu, target);
    Ok(true)
}

/// Move virtual time forward to `target_ns` (never back) and fire due timers.
pub fn advance_to(emu: &mut Emu, target_ns: u64) {
    let now = now_ns(emu);
    if target_ns > now {
        emu.cpu.clock_offset += ns_to_ticks(target_ns - now);
    }
    fire_timers(emu);
}

/// The earliest deadline of any waiting thread or armed timer.
fn next_deadline(emu: &Emu, own: Option<u64>) -> Option<u64> {
    let mut next = own;
    let mut consider = |d: Option<u64>| {
        if let Some(d) = d {
            next = Some(next.map_or(d, |n: u64| n.min(d)));
        }
    };
    for t in &emu.th.threads {
        if let Run::Blocked(w) = &t.run {
            consider(w.deadline());
        }
    }
    for p in emu.th.ports.values() {
        consider(p.timer);
    }
    next
}

/// Block the current thread until `wait` completes; returns the call's result
/// (for in-place waits) or [`Stop::Blocked`] (for threads that unwind).
pub fn block(emu: &mut Emu, wait: Wait) -> Result<u64> {
    if let Some(r) = try_complete(emu, &wait)? {
        return Ok(r);
    }
    let me = emu.th.current;
    if me != 0 && emu.depth == emu.th.threads[me].base_depth {
        emu.th.threads[me].run = Run::Blocked(wait);
        return Err(Stop::Blocked);
    }
    // Wait in place.
    emu.th.threads[me].run = Run::Pinned;
    let result = loop {
        if let Some(r) = try_complete(emu, &wait)? {
            break Ok(r);
        }
        if schedule_round(emu)? {
            continue;
        }
        // A live host paces time on the wall clock and may deliver input.
        if emu.live.host.is_some() && crate::host::idle(emu, next_deadline(emu, wait.deadline()))? {
            continue;
        }
        if !advance_time(emu, wait.deadline())? {
            break Err(Stop::Idle(describe(emu, me, &wait)));
        }
    };
    emu.th.threads[me].run = Run::Runnable;
    result
}

fn describe(emu: &Emu, me: usize, wait: &Wait) -> String {
    let mut s = format!("every thread is waiting with nothing scheduled — thread {me} ({}) on {wait:?}", emu.th.threads[me].name);
    for (i, t) in emu.th.threads.iter().enumerate() {
        if i != me {
            s += &format!("; thread {i} ({}) {:?}", t.name, t.run);
        }
    }
    s
}

// ---- Mach ports ----

fn message(local: u32, id: u32) -> Vec<u8> {
    let mut m = vec![0u8; 24];
    m[4..8].copy_from_slice(&24u32.to_le_bytes());
    m[12..16].copy_from_slice(&local.to_le_bytes());
    m[20..24].copy_from_slice(&id.to_le_bytes());
    m
}

pub fn new_port(emu: &mut Emu, is_set: bool) -> u32 {
    let name = emu.th.next_port;
    emu.th.next_port += 4;
    emu.th.ports.insert(name, Port { is_set, ..Default::default() });
    name
}

/// Queue a message on a port (e.g. a wakeup).
pub fn post(emu: &mut Emu, port: u32, msg: Vec<u8>) {
    if let Some(p) = emu.th.ports.get_mut(&port) {
        p.queue.push_back(msg);
    }
}

/// A message for `name` (a port, or any member of a port set), lowest port first.
fn take_message(emu: &mut Emu, name: u32) -> Option<(u32, Vec<u8>)> {
    let is_set = emu.th.ports.get(&name)?.is_set;
    let port = if is_set {
        *emu.th.ports.iter().find(|(_, p)| p.sets.contains(&name) && !p.queue.is_empty())?.0
    } else {
        name
    };
    let msg = emu.th.ports.get_mut(&port)?.queue.pop_front()?;
    Some((port, msg))
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// `mach_port_allocate(task, right, mach_port_name_t *name)`: 1 = receive, 3 = port set.
fn mach_port_allocate(emu: &mut Emu) -> Result<()> {
    let (right, out) = (emu.arg(1), emu.arg(2));
    let p = new_port(emu, right == 3);
    emu.p.memory.write_u32(out, p)?;
    if emu.trace {
        println!("{:>10}   [thread {}] mach port {p:#x} allocated ({}) from {}", emu.cpu.steps, emu.th.current, if right == 3 { "set" } else { "receive" }, emu.symbolize(emu.cpu.x[30].wrapping_sub(4)));
    }
    ret(emu, 0)
}

/// `mach_port_construct(task, mach_port_options_t *, context, mach_port_name_t *name)`
fn mach_port_construct(emu: &mut Emu) -> Result<()> {
    let out = emu.arg(3);
    let p = new_port(emu, false);
    emu.p.memory.write_u32(out, p)?;
    ret(emu, 0)
}

/// `mach_port_insert_member(task, member, set)`
fn mach_port_insert_member(emu: &mut Emu) -> Result<()> {
    let (member, set) = (emu.arg(1) as u32, emu.arg(2) as u32);
    if emu.trace {
        println!("{:>10}   [thread {}] mach port {member:#x} joins set {set:#x}", emu.cpu.steps, emu.th.current);
    }
    if let Some(p) = emu.th.ports.get_mut(&member)
        && !p.sets.contains(&set)
    {
        p.sets.push(set);
    }
    ret(emu, 0)
}

fn mach_port_extract_member(emu: &mut Emu) -> Result<()> {
    let (member, set) = (emu.arg(1) as u32, emu.arg(2) as u32);
    if let Some(p) = emu.th.ports.get_mut(&member) {
        p.sets.retain(|&s| s != set);
    }
    ret(emu, 0)
}

fn mk_timer_create(emu: &mut Emu) -> Result<()> {
    let p = new_port(emu, false);
    ret(emu, p as u64)
}

/// `mk_timer_arm(port, uint64_t fire_time)` — fire time in mach ticks.
fn mk_timer_arm(emu: &mut Emu) -> Result<()> {
    let (port, at) = (emu.arg(0) as u32, emu.arg(1));
    let ns = at * NS_PER_TICK_NUM / NS_PER_TICK_DEN;
    if let Some(p) = emu.th.ports.get_mut(&port) {
        p.timer = Some(ns);
    }
    ret(emu, 0)
}

fn mk_timer_cancel(emu: &mut Emu) -> Result<()> {
    let port = emu.arg(0) as u32;
    if let Some(p) = emu.th.ports.get_mut(&port) {
        p.timer = None;
    }
    if emu.arg(1) != 0 {
        emu.write_u64(emu.arg(1), 0)?;
    }
    ret(emu, 0)
}

/// `mach_msg(msg, option, send_size, rcv_size, rcv_name, timeout_ms, notify)`
fn mach_msg(emu: &mut Emu) -> Result<()> {
    let (msg, option, send_size, rcv_size, rcv_name, timeout) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4) as u32, emu.arg(5));
    if option & MACH_SEND_MSG != 0 {
        let dest = emu.p.memory.read_u32(msg + 8)?;
        let bytes = emu.p.memory.read(msg, send_size.max(24) as usize)?.to_vec();
        if emu.trace {
            let known = emu.th.ports.contains_key(&dest);
            println!("{:>10}   [thread {}] mach send to port {dest:#x}{}", emu.cpu.steps, emu.th.current, if known { "" } else { " (unknown port: dropped)" });
        }
        post(emu, dest, bytes);
    }
    if option & MACH_RCV_MSG != 0 {
        if emu.trace {
            let members: Vec<u32> = emu.th.ports.iter().filter(|(_, p)| p.sets.contains(&rcv_name)).map(|(n, _)| *n).collect();
            println!("{:>10}   [thread {}] mach receive on {rcv_name:#x} (members {members:x?}, timeout {})", emu.cpu.steps, emu.th.current, if option & MACH_RCV_TIMEOUT != 0 { format!("{timeout}ms") } else { "none".into() });
        }
        let deadline = (option & MACH_RCV_TIMEOUT != 0).then(|| now_ns(emu) + timeout * 1_000_000);
        let r = block(emu, Wait::MachReceive { name: rcv_name, buf: msg, size: rcv_size, deadline })?;
        return ret(emu, r);
    }
    ret(emu, 0)
}

/// `_dispatch_get_main_queue_port_4CF()`: the main run loop listens on this
/// port; `dispatch_async` to the main queue posts to it.
fn main_queue_port(emu: &mut Emu) -> Result<()> {
    if emu.th.main_queue_port == 0 {
        emu.th.main_queue_port = new_port(emu, false);
    }
    ret(emu, emu.th.main_queue_port as u64)
}

/// Called by `dispatch_async` for main-queue work: wake the main run loop.
pub fn wake_main_queue(emu: &mut Emu) {
    let port = emu.th.main_queue_port;
    if port != 0 && emu.th.ports.get(&port).is_some_and(|p| p.queue.is_empty()) {
        post(emu, port, message(port, 0));
    }
}

fn main_queue_callback(emu: &mut Emu) -> Result<()> {
    crate::libsystem::drain_queue(emu)?;
    Ok(())
}

// ---- pthreads ----

fn current_tsd(emu: &mut Emu) -> &mut HashMap<u64, u64> {
    let me = emu.th.current;
    &mut emu.th.threads[me].tsd
}

/// `pthread_create(pthread_t *out, attr, start, arg)`
fn pthread_create(emu: &mut Emu) -> Result<()> {
    let (out, start, arg) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let tid = spawn(emu, start, arg, "pthread")?;
    let handle = emu.th.threads[tid].handle;
    emu.write_u64(out, handle)?;
    ret(emu, 0)
}

fn tid_of(emu: &Emu, handle: u64) -> usize {
    if handle == 0 { emu.th.current } else { emu.th.handles.get(&handle).copied().unwrap_or(usize::MAX) }
}

/// `pthread_join(pthread_t, void **value)`
fn pthread_join(emu: &mut Emu) -> Result<()> {
    let (handle, out) = (emu.arg(0), emu.arg(1));
    let tid = tid_of(emu, handle);
    let r = block(emu, Wait::Join { tid, out })?;
    ret(emu, r)
}

fn pthread_self(emu: &mut Emu) -> Result<()> {
    let h = emu.th.threads[emu.th.current].handle;
    ret(emu, h)
}

fn pthread_main_np(emu: &mut Emu) -> Result<()> {
    ret(emu, (emu.th.current == 0) as u64)
}

/// `pthread_threadid_np(pthread_t, uint64_t *id)`
fn pthread_threadid_np(emu: &mut Emu) -> Result<()> {
    let tid = tid_of(emu, emu.arg(0));
    if emu.arg(1) != 0 {
        emu.write_u64(emu.arg(1), tid as u64 + 1)?;
    }
    ret(emu, 0)
}

fn pthread_mach_thread_np(emu: &mut Emu) -> Result<()> {
    let tid = tid_of(emu, emu.arg(0));
    ret(emu, 0x2000 + tid as u64)
}

/// `pthread_get_stackaddr_np(pthread_t)`: the stack's top (it grows down).
fn pthread_get_stackaddr_np(emu: &mut Emu) -> Result<()> {
    let tid = tid_of(emu, emu.arg(0));
    let top = emu.th.threads.get(tid).map_or(0, |t| t.stack_top);
    ret(emu, top)
}

fn pthread_get_stacksize_np(emu: &mut Emu) -> Result<()> {
    let tid = tid_of(emu, emu.arg(0));
    let size = emu.th.threads.get(tid).map_or(0, |t| t.stack_size);
    ret(emu, size)
}

fn pthread_key_create(emu: &mut Emu) -> Result<()> {
    emu.th.next_key += 1;
    let k = emu.th.next_key + 0x100; // above the reserved Darwin TSD slots
    emu.write_u64(emu.arg(0), k)?;
    ret(emu, 0)
}

fn pthread_getspecific(emu: &mut Emu) -> Result<()> {
    let k = emu.arg(0);
    let v = current_tsd(emu).get(&k).copied().unwrap_or(0);
    ret(emu, v)
}

fn pthread_setspecific(emu: &mut Emu) -> Result<()> {
    let (k, v) = (emu.arg(0), emu.arg(1));
    current_tsd(emu).insert(k, v);
    ret(emu, 0)
}

fn pthread_setname_np(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    let me = emu.th.current;
    emu.th.threads[me].name = name;
    ret(emu, 0)
}

/// `pthread_getname_np(pthread_t, char *buf, size_t len)`
fn pthread_getname_np(emu: &mut Emu) -> Result<()> {
    let tid = tid_of(emu, emu.arg(0));
    let name = emu.th.threads.get(tid).map_or(String::new(), |t| t.name.clone());
    let (buf, len) = (emu.arg(1), emu.arg(2) as usize);
    if len > 0 {
        let n = name.len().min(len - 1);
        emu.p.memory.write(buf, &name.as_bytes()[..n])?;
        emu.p.memory.write_u8(buf + n as u64, 0)?;
    }
    ret(emu, 0)
}

// ---- condition variables and semaphores ----

/// `pthread_cond_wait(cond, mutex)` — mutexes are no-ops (only blocking points switch threads).
fn pthread_cond_wait(emu: &mut Emu) -> Result<()> {
    let cond = emu.arg(0);
    let generation = emu.th.conds.get(&cond).copied().unwrap_or(0);
    let r = block(emu, Wait::Cond { cond, generation, deadline: None })?;
    ret(emu, r)
}

/// `pthread_cond_timedwait(cond, mutex, const struct timespec *abstime)` (wall clock)
fn pthread_cond_timedwait(emu: &mut Emu) -> Result<()> {
    let (cond, ts) = (emu.arg(0), emu.arg(2));
    let secs = emu.read_u64(ts)?;
    let nsec = emu.read_u64(ts + 8)?;
    let wall_ns = secs * 1_000_000_000 + nsec;
    let deadline = wall_ns.saturating_sub(crate::libsystem::EPOCH_BASE_SECS * 1_000_000_000);
    let generation = emu.th.conds.get(&cond).copied().unwrap_or(0);
    let r = block(emu, Wait::Cond { cond, generation, deadline: Some(deadline) })?;
    ret(emu, r)
}

/// Spurious wakeups are allowed, so signal wakes every waiter, like broadcast.
fn pthread_cond_signal(emu: &mut Emu) -> Result<()> {
    *emu.th.conds.entry(emu.arg(0)).or_default() += 1;
    ret(emu, 0)
}

/// `dispatch_semaphore_wait(sem, dispatch_time_t timeout)`
fn dispatch_semaphore_wait(emu: &mut Emu) -> Result<()> {
    let (sem, timeout) = (emu.arg(0), emu.arg(1));
    let deadline = (timeout != u64::MAX).then_some(timeout);
    let r = block(emu, Wait::Semaphore { sem, deadline })?;
    ret(emu, r)
}

/// `dispatch_time(when, delta_ns)`: our dispatch_time_t is nanoseconds of the
/// guest clock; DISPATCH_TIME_NOW is 0, DISPATCH_TIME_FOREVER is ~0.
fn dispatch_time(emu: &mut Emu) -> Result<()> {
    let (when, delta) = (emu.arg(0), emu.arg(1) as i64);
    let base = match when {
        0 => now_ns(emu),
        u64::MAX => return ret(emu, u64::MAX),
        w => w,
    };
    ret(emu, base.saturating_add_signed(delta))
}

fn sleep_ns(emu: &mut Emu, ns: u64) -> Result<()> {
    let deadline = now_ns(emu) + ns;
    block(emu, Wait::Sleep { deadline })?;
    ret(emu, 0)
}

fn usleep(emu: &mut Emu) -> Result<()> {
    sleep_ns(emu, emu.arg(0) * 1000)
}

fn sleep(emu: &mut Emu) -> Result<()> {
    sleep_ns(emu, emu.arg(0) * 1_000_000_000)
}

/// `nanosleep(const struct timespec *req, rem)`
fn nanosleep(emu: &mut Emu) -> Result<()> {
    let ts = emu.arg(0);
    let ns = emu.read_u64(ts)? * 1_000_000_000 + emu.read_u64(ts + 8)?;
    sleep_ns(emu, ns)
}

fn sched_yield(emu: &mut Emu) -> Result<()> {
    if emu.th.current == 0 || emu.depth != emu.th.threads[emu.th.current].base_depth {
        schedule_round(emu)?;
    }
    ret(emu, 0)
}

/// `thread_info(thread, flavor, info, count*)`: THREAD_BASIC_INFO (3), with
/// the CPU time being virtual time (deterministic; a busy thread, roughly).
fn thread_info(emu: &mut Emu) -> Result<()> {
    let (flavor, info, count) = (emu.arg(1), emu.arg(2), emu.arg(3));
    const THREAD_BASIC_INFO: u64 = 3;
    const WORDS: u64 = 10; // sizeof(thread_basic_info) / sizeof(natural_t)
    if flavor != THREAD_BASIC_INFO || info == 0 {
        return ret(emu, 4); // KERN_INVALID_ARGUMENT
    }
    let ns = now_ns(emu);
    let words = [(ns / 1_000_000_000) as u32, ((ns / 1000) % 1_000_000) as u32, 0, 0, 0, 0, 1, 0, 0, 0];
    for (i, w) in words.iter().enumerate() {
        emu.p.memory.write_u32(info + i as u64 * 4, *w)?;
    }
    if count != 0 {
        emu.p.memory.write_u32(count, WORDS as u32)?;
    }
    ret(emu, 0)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_thread_info", thread_info),
    ("_mach_port_allocate", mach_port_allocate),
    ("_mach_port_construct", mach_port_construct),
    ("_mach_port_insert_member", mach_port_insert_member),
    ("_mach_port_extract_member", mach_port_extract_member),
    ("_mk_timer_create", mk_timer_create),
    ("_mk_timer_arm", mk_timer_arm),
    ("_mk_timer_cancel", mk_timer_cancel),
    ("_mach_msg", mach_msg),
    ("__dispatch_get_main_queue_port_4CF", main_queue_port),
    ("__dispatch_main_queue_callback_4CF", main_queue_callback),
    ("_pthread_create", pthread_create),
    ("_pthread_join", pthread_join),
    ("_pthread_self", pthread_self),
    ("_pthread_main_np", pthread_main_np),
    ("_pthread_threadid_np", pthread_threadid_np),
    ("_pthread_mach_thread_np", pthread_mach_thread_np),
    ("_pthread_key_create", pthread_key_create),
    ("_pthread_get_stackaddr_np", pthread_get_stackaddr_np),
    ("_pthread_get_stacksize_np", pthread_get_stacksize_np),
    ("_pthread_getspecific", pthread_getspecific),
    ("_pthread_setspecific", pthread_setspecific),
    ("_pthread_setname_np", pthread_setname_np),
    ("_pthread_getname_np", pthread_getname_np),
    ("_pthread_cond_wait", pthread_cond_wait),
    ("_pthread_cond_timedwait", pthread_cond_timedwait),
    ("_pthread_cond_signal", pthread_cond_signal),
    ("_pthread_cond_broadcast", pthread_cond_signal),
    ("_dispatch_semaphore_wait", dispatch_semaphore_wait),
    ("_dispatch_time", dispatch_time),
    ("_dispatch_walltime", dispatch_time),
    ("_usleep", usleep),
    ("_sleep", sleep),
    ("_nanosleep", nanosleep),
    ("_sched_yield", sched_yield),
];

/// The thread-specific value of a TSD key for the running thread (for other modules).
pub fn tsd_get(emu: &mut Emu, key: u64) -> u64 {
    current_tsd(emu).get(&key).copied().unwrap_or(0)
}

// ---- NSThread: {isa, target, selector, object, block, tid+1, name, threadDictionary} ----

const LDR_LIT: u32 = 0x5800_0000; // ldr xN, #imm19*4
const BR_X16: u32 = 0xd61f_0200;
const NOP: u32 = 0xd503_201f;

fn ldr_lit(reg: u32, words_ahead: u32) -> u32 {
    LDR_LIT | (words_ahead << 5) | reg
}

/// Guest code that calls `objc_msgSend(x0, x1, x2)` with fixed arguments.
fn msgsend_trampoline(emu: &mut Emu, recv: u64, sel: u64, arg: u64) -> Result<u64> {
    let send = emu.p.stubs.find("_objc_msgSend").first().copied().ok_or(Stop::Unsupported("no objc_msgSend stub".into()))?;
    let lit = |v: u64| [v as u32, (v >> 32) as u32];
    let mut words = vec![ldr_lit(0, 6), ldr_lit(1, 7), ldr_lit(2, 8), ldr_lit(16, 9), BR_X16, NOP];
    for v in [recv, sel, arg, send] {
        words.extend(lit(v));
    }
    emu.emit_code(&words)
}

/// Guest code that calls a block with no arguments.
fn block_trampoline(emu: &mut Emu, block: u64) -> Result<u64> {
    emu.emit_code(&[
        ldr_lit(0, 4), // the block
        0xf940_0810,   // ldr x16, [x0, #16]: its invoke function
        BR_X16,
        NOP,
        block as u32,
        (block >> 32) as u32,
    ])
}

fn ns_thread_object(emu: &mut Emu, tid: usize) -> Result<u64> {
    if let Some(&o) = emu.th.nsthreads.get(&tid) {
        return Ok(o);
    }
    let cls = crate::objc::host_class(emu, "NSThread")?;
    let o = crate::objc::create_instance(emu, cls, 0)?;
    emu.write_u64(o + 40, tid as u64 + 1)?;
    emu.th.nsthreads.insert(tid, o);
    Ok(o)
}

fn nsthread_init_target(emu: &mut Emu) -> Result<()> {
    let (t, target, sel, obj) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    emu.write_u64(t + 8, target)?;
    emu.write_u64(t + 16, sel)?;
    emu.write_u64(t + 24, obj)?;
    ret(emu, t)
}

fn nsthread_init_block(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    emu.cpu.x[0] = emu.arg(2);
    crate::libc::block_copy(emu)?;
    let block = emu.cpu.x[0];
    emu.write_u64(t + 32, block)?;
    ret(emu, t)
}

/// `-start`: a new guest thread runs the subclass's `-main`, or the target/selector or block.
fn nsthread_start(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    if emu.read_u64(t + 40)? != 0 {
        return ret(emu, 0);
    }
    let cls = emu.read_u64(t)?;
    let main_sel = crate::objc::intern(emu, "main")?;
    let overrides_main = matches!(crate::objc::lookup(emu, cls, main_sel)?, Some(crate::objc::Imp::Guest(_)));
    let block = emu.read_u64(t + 32)?;
    let pc = if overrides_main {
        msgsend_trampoline(emu, t, main_sel, 0)?
    } else if block != 0 {
        block_trampoline(emu, block)?
    } else {
        let (target, sel, obj) = (emu.read_u64(t + 8)?, emu.read_u64(t + 16)?, emu.read_u64(t + 24)?);
        msgsend_trampoline(emu, target, sel, obj)?
    };
    let name_obj = emu.read_u64(t + 48)?;
    let name = if name_obj == 0 { "NSThread".to_string() } else { crate::foundation::nsstring(emu, name_obj)? };
    let tid = spawn(emu, pc, 0, &name)?;
    emu.write_u64(t + 40, tid as u64 + 1)?;
    emu.th.nsthreads.insert(tid, t);
    ret(emu, 0)
}

fn tid_of_nsthread(emu: &Emu, t: u64) -> Result<Option<usize>> {
    let v = emu.read_u64(t + 40)?;
    Ok((v != 0).then(|| v as usize - 1))
}

fn nsthread_set_name(emu: &mut Emu) -> Result<()> {
    let (t, name) = (emu.arg(0), emu.arg(2));
    emu.write_u64(t + 48, name)?;
    if let Some(tid) = tid_of_nsthread(emu, t)? {
        emu.th.threads[tid].name = crate::foundation::nsstring(emu, name)?;
    }
    ret(emu, 0)
}

fn nsthread_name(emu: &mut Emu) -> Result<()> {
    let n = emu.read_u64(emu.arg(0) + 48)?;
    ret(emu, n)
}

fn nsthread_is_main(emu: &mut Emu) -> Result<()> {
    let tid = tid_of_nsthread(emu, emu.arg(0))?;
    ret(emu, (tid == Some(0)) as u64)
}

fn nsthread_is_finished(emu: &mut Emu) -> Result<()> {
    let tid = tid_of_nsthread(emu, emu.arg(0))?;
    let done = tid.is_some_and(|t| matches!(emu.th.threads[t].run, Run::Done(_)));
    ret(emu, done as u64)
}

fn nsthread_is_executing(emu: &mut Emu) -> Result<()> {
    let tid = tid_of_nsthread(emu, emu.arg(0))?;
    let running = tid.is_some_and(|t| !matches!(emu.th.threads[t].run, Run::Done(_)));
    ret(emu, running as u64)
}

fn nsthread_thread_dictionary(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    let mut d = emu.read_u64(t + 56)?;
    if d == 0 {
        d = crate::foundation::new_dict_of(emu, &[])?;
        emu.write_u64(t + 56, d)?;
    }
    ret(emu, d)
}

fn nsthread_current(emu: &mut Emu) -> Result<()> {
    let o = ns_thread_object(emu, emu.th.current)?;
    ret(emu, o)
}

fn nsthread_main(emu: &mut Emu) -> Result<()> {
    let o = ns_thread_object(emu, 0)?;
    ret(emu, o)
}

fn nsthread_class_is_main(emu: &mut Emu) -> Result<()> {
    ret(emu, (emu.th.current == 0) as u64)
}

fn nsthread_detach_block(emu: &mut Emu) -> Result<()> {
    let cls = crate::objc::host_class(emu, "NSThread")?;
    let t = crate::objc::create_instance(emu, cls, 0)?;
    emu.cpu.x[0] = t;
    nsthread_init_block(emu)?;
    emu.cpu.x[0] = t;
    nsthread_start(emu)
}

/// `+detachNewThreadSelector:toTarget:withObject:`
fn nsthread_detach_selector(emu: &mut Emu) -> Result<()> {
    let (sel, target, obj) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let cls = crate::objc::host_class(emu, "NSThread")?;
    let t = crate::objc::create_instance(emu, cls, 0)?;
    emu.write_u64(t + 8, target)?;
    emu.write_u64(t + 16, sel)?;
    emu.write_u64(t + 24, obj)?;
    emu.cpu.x[0] = t;
    nsthread_start(emu)
}

/// `+sleepForTimeInterval:(NSTimeInterval)seconds` (d0)
fn nsthread_sleep(emu: &mut Emu) -> Result<()> {
    let secs = f64::from_bits(emu.cpu.q[0] as u64).max(0.0);
    sleep_ns(emu, (secs * 1e9) as u64)
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_yes(emu: &mut Emu) -> Result<()> {
    ret(emu, 1)
}

/// A global block whose invoke does `[target sel:obj]` and then sets `*flag = 1`.
fn perform_block(emu: &mut Emu, target: u64, sel: u64, obj: u64, flag: u64) -> Result<u64> {
    let send = emu.p.stubs.find("_objc_msgSend").first().copied().ok_or(Stop::Unsupported("no objc_msgSend stub".into()))?;
    let mut words = vec![
        0xa9bf_7bfd, // stp x29, x30, [sp, #-16]!
        0x9100_03fd, // mov x29, sp
        ldr_lit(0, 10),
        ldr_lit(1, 11),
        ldr_lit(2, 12),
        ldr_lit(16, 13),
        0xd63f_0200, // blr x16
        ldr_lit(9, 13),
        0x5280_002a, // mov w10, #1
        0xb900_012a, // str w10, [x9]
        0xa8c1_7bfd, // ldp x29, x30, [sp], #16
        0xd65f_03c0, // ret
    ];
    for v in [target, sel, obj, send, flag] {
        words.extend([v as u32, (v >> 32) as u32]);
    }
    let invoke = emu.emit_code(&words)?;
    // struct Block_layout { isa; int flags; int reserved; invoke; descriptor } + descriptor {reserved, size}
    let block = emu.alloc(48)?;
    let isa = crate::objc::host_class(emu, "__NSGlobalBlock__")?;
    emu.write_u64(block, isa)?;
    emu.p.memory.write_u32(block + 8, 1 << 28)?; // BLOCK_IS_GLOBAL
    emu.write_u64(block + 16, invoke)?;
    emu.write_u64(block + 24, block + 32)?;
    emu.write_u64(block + 40, 32)?;
    Ok(block)
}

/// Run `[target sel:obj]` on thread `tid` through its run loop; optionally wait for it.
fn perform_on_thread(emu: &mut Emu, tid: usize, target: u64, sel: u64, obj: u64, wait: bool) -> Result<()> {
    if tid == emu.th.current {
        if wait {
            let name = crate::objc::sel_name(emu, sel);
            crate::objc::send(emu, target, &name, &[obj])?;
        } else {
            // Not waiting: still asynchronous, on our own run loop.
            let flag = emu.alloc(8)?;
            let block = perform_block(emu, target, sel, obj, flag)?;
            enqueue_on_run_loop(emu, tid, block)?;
        }
        return ret(emu, 0);
    }
    let flag = emu.alloc(8)?;
    let work = perform_block(emu, target, sel, obj, flag)?;
    enqueue_on_run_loop(emu, tid, work)?;
    if wait {
        block(emu, Wait::Flag { addr: flag })?;
    }
    ret(emu, 0)
}

fn enqueue_on_run_loop(emu: &mut Emu, tid: usize, block: u64) -> Result<()> {
    let handle = emu.th.threads[tid].handle;
    let rl = crate::foundation::cf_call(emu, "__CFRunLoopGet0", &[handle])?;
    let modes = crate::foundation::cf_data_pub(emu, "_kCFRunLoopCommonModes")?;
    crate::foundation::cf_call(emu, "_CFRunLoopPerformBlock", &[rl, modes, block])?;
    crate::foundation::cf_call(emu, "_CFRunLoopWakeUp", &[rl])?;
    Ok(())
}

/// `-performSelector:onThread:withObject:waitUntilDone:` (and the `modes:` variant)
fn perform_on_thread_sel(emu: &mut Emu) -> Result<()> {
    let (target, sel, thread, obj, wait) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5) & 0xff != 0);
    let tid = tid_of_nsthread(emu, thread)?.ok_or(Stop::Unsupported("performSelector:onThread: thread not started".into()))?;
    perform_on_thread(emu, tid, target, sel, obj, wait)
}

/// `-performSelectorOnMainThread:withObject:waitUntilDone:` (and the `modes:` variant)
fn perform_on_main_sel(emu: &mut Emu) -> Result<()> {
    let (target, sel, obj, wait) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4) & 0xff != 0);
    perform_on_thread(emu, 0, target, sel, obj, wait)
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSObject", false, "performSelector:onThread:withObject:waitUntilDone:", perform_on_thread_sel),
    ("NSObject", false, "performSelector:onThread:withObject:waitUntilDone:modes:", perform_on_thread_sel),
    ("NSObject", false, "performSelectorOnMainThread:withObject:waitUntilDone:", perform_on_main_sel),
    ("NSObject", false, "performSelectorOnMainThread:withObject:waitUntilDone:modes:", perform_on_main_sel),
    ("NSThread", false, "initWithTarget:selector:object:", nsthread_init_target),
    ("NSThread", false, "initWithBlock:", nsthread_init_block),
    ("NSThread", false, "start", nsthread_start),
    ("NSThread", false, "setName:", nsthread_set_name),
    ("NSThread", false, "name", nsthread_name),
    ("NSThread", false, "isMainThread", nsthread_is_main),
    ("NSThread", false, "isFinished", nsthread_is_finished),
    ("NSThread", false, "isExecuting", nsthread_is_executing),
    ("NSThread", false, "isCancelled", |e| ret(e, 0)),
    ("NSThread", false, "cancel", noop),
    ("NSThread", false, "threadDictionary", nsthread_thread_dictionary),
    ("NSThread", false, "setQualityOfService:", noop),
    ("NSThread", false, "qualityOfService", |e| ret(e, 0x21)),
    ("NSThread", false, "setStackSize:", noop),
    ("NSThread", false, "stackSize", |e| ret(e, STACK)),
    ("NSThread", false, "setThreadPriority:", noop),
    ("NSThread", true, "currentThread", nsthread_current),
    ("NSThread", true, "mainThread", nsthread_main),
    ("NSThread", true, "isMainThread", nsthread_class_is_main),
    ("NSThread", true, "isMultiThreaded", ret_yes),
    ("NSThread", true, "detachNewThreadWithBlock:", nsthread_detach_block),
    ("NSThread", true, "detachNewThreadSelector:toTarget:withObject:", nsthread_detach_selector),
    ("NSThread", true, "sleepForTimeInterval:", nsthread_sleep),
];
