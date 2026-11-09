//! A sampling profiler for guest code: where the app's instructions go.
//!
//! Every `every` instructions the CPU stops ([`pomme_cpu::Exit::Sample`]) and
//! the profiler records the running thread's stack. It walks the guest's frame
//! pointers (arm64 Apple code keeps them), so it sees every guest function
//! from the one running to the thread's entry; where guest code called into
//! pomme (a stub or a hooked function or a host thunk) and pomme called back
//! into guest code, the host function is put in its place in the stack. Frames
//! are named from each image's symbol table.
//!
//! Time is instructions retired, the guest's clock, so the same run gives the
//! same profile every time; what pomme's own host functions cost in wall time
//! isn't counted (profile the pomme binary itself for that).
//!
//! [`Profiler::speedscope`] writes the samples in speedscope's format
//! (<https://www.speedscope.app>: flame chart, left-heavy, sandwich), one
//! profile per thread; [`Profiler::summary`] lists the functions with the
//! most instructions.

use crate::Emu;
use pomme_cpu::HOST_RETURN;
use std::collections::HashMap;

/// A host function the guest is in the middle of calling, and where it returns
/// to (the guest caller, which no frame record holds: only lr had it).
#[derive(Clone, Copy, Debug)]
pub(crate) struct HostCall {
    pub frame: HostFrame,
    pub ret: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HostFrame {
    /// The stub at this address (an imported function pomme implements).
    Stub(u64),
    /// The hooked guest function at this address.
    Hook(u64),
    /// A host thunk (run-loop callouts, blocks pomme made).
    Thunk(usize),
}

pub struct Profiler {
    /// Instructions between samples.
    pub every: u64,
    /// Frame names and the image each is in.
    frames: Vec<(String, String)>,
    /// (image index, function start) or a host frame's key → frame index.
    ids: HashMap<FrameKey, u32>,
    /// Address → frame index, so each address is looked up once.
    by_addr: HashMap<u64, u32>,
    /// Distinct stacks (root first) and their indices.
    stacks: Vec<Vec<u32>>,
    stack_ids: HashMap<Vec<u32>, u32>,
    /// Per thread: its samples in order, as (stack, weight) with repeats merged.
    threads: HashMap<usize, Vec<(u32, u64)>>,
    /// Host functions being called, innermost last.
    host: Vec<HostCall>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum FrameKey {
    Guest(usize, u64),
    Unknown(u64),
    Host(String),
}

impl Profiler {
    pub fn new(every: u64) -> Self {
        Profiler {
            every: every.max(1),
            frames: Vec::new(),
            ids: HashMap::new(),
            by_addr: HashMap::new(),
            stacks: Vec::new(),
            stack_ids: HashMap::new(),
            threads: HashMap::new(),
            host: Vec::new(),
        }
    }

    pub(crate) fn host_depth(&self) -> usize {
        self.host.len()
    }

    pub(crate) fn push_host(&mut self, c: HostCall) {
        self.host.push(c);
    }

    pub(crate) fn pop_host(&mut self) {
        self.host.pop();
    }

    fn frame(&mut self, key: FrameKey, name: impl FnOnce() -> (String, String)) -> u32 {
        if let Some(&i) = self.ids.get(&key) {
            return i;
        }
        let i = self.frames.len() as u32;
        self.frames.push(name());
        self.ids.insert(key, i);
        i
    }

    /// The samples recorded so far.
    pub fn samples(&self) -> u64 {
        self.threads.values().flatten().map(|&(_, w)| w).sum::<u64>() / self.every
    }
}

/// The function containing `addr`: its frame.
fn guest_frame(emu: &mut Emu, addr: u64) -> u32 {
    if let Some(&i) = emu.profiler.as_ref().and_then(|p| p.by_addr.get(&addr)) {
        return i;
    }
    let (key, name) = match emu.p.images.iter().position(|i| addr >= i.start && addr < i.end) {
        Some(ii) => {
            let img = &emu.p.images[ii];
            match img.symbol_at(addr) {
                Some((start, sym)) => (FrameKey::Guest(ii, start), (display_name(sym), img.name.clone())),
                // No symbol before it: name it by image and offset.
                None => (FrameKey::Guest(ii, addr), (format!("{}+{:#x}", img.name, addr - img.start), img.name.clone())),
            }
        }
        None => (FrameKey::Unknown(addr), (format!("{addr:#x}"), String::new())),
    };
    let p = emu.profiler.as_mut().unwrap();
    let i = p.frame(key, || name);
    p.by_addr.insert(addr, i);
    i
}

fn host_frame(emu: &mut Emu, f: HostFrame) -> u32 {
    let name = match f {
        HostFrame::Stub(addr) => emu.p.stubs.at(addr).map_or("?".to_string(), |s| s.symbol.strip_prefix('_').unwrap_or(&s.symbol).to_string()),
        HostFrame::Hook(addr) => {
            let g = guest_frame(emu, addr);
            format!("{} (pomme)", emu.profiler.as_ref().unwrap().frames[g as usize].0)
        }
        HostFrame::Thunk(i) => format!("host callout #{i}"),
    };
    let p = emu.profiler.as_mut().unwrap();
    p.frame(FrameKey::Host(name.clone()), || (name, "pomme".to_string()))
}

/// A symbol as people read it: without the C symbol underscore, and C++ names
/// demangled (without parameters, which make flame charts unreadable).
pub fn display_name(sym: &str) -> String {
    let sym = sym.strip_prefix('_').unwrap_or(sym);
    // A block inside a function: `__<function>_block_invoke[_N]`.
    // (An Objective-C method's has its name's length first: `___40-[A b:]_block_invoke`.)
    if let Some(i) = sym.find("_block_invoke")
        && let Some(inner) = sym[..i].strip_prefix('_')
    {
        let inner = if inner.starts_with("_Z") { demangle(inner) } else { inner.trim_start_matches(|c: char| c == '_' || c.is_ascii_digit()).to_string() };
        return format!("block in {inner}");
    }
    demangle(sym)
}

/// A C++ name (`_Z…`) demangled; anything else as it is.
fn demangle(sym: &str) -> String {
    if sym.starts_with("_Z") {
        let opts = cpp_demangle::DemangleOptions::new().no_params().no_return_type();
        if let Some(name) = cpp_demangle::Symbol::new(sym.as_bytes()).ok().and_then(|s| s.demangle_with_options(&opts).ok()) {
            return name;
        }
    }
    sym.to_string()
}

/// The function a code address is in, for telling a leaf's caller from itself.
fn function_of(emu: &Emu, addr: u64) -> Option<u64> {
    let img = emu.p.image_at(addr)?;
    img.symbol_at(addr).map(|(start, _)| start)
}

/// A host function in the stack, and the guest function that called it.
fn push_host_call(emu: &mut Emu, stack: &mut Vec<u32>, h: HostCall) {
    stack.push(host_frame(emu, h.frame));
    if h.ret != HOST_RETURN && h.ret > 4 {
        stack.push(guest_frame(emu, h.ret - 4));
    }
}

/// Take a sample of the running thread (the CPU stopped with `Exit::Sample`).
pub(crate) fn sample(emu: &mut Emu) {
    let Some(every) = emu.profiler.as_ref().map(|p| p.every) else { return };
    emu.cpu.sample_at = emu.cpu.steps + every;
    let tid = emu.th.current;
    let host_base = if tid == 0 { 0 } else { emu.th.threads.get(tid).map_or(0, |t| t.host_base) };
    let mut host: Vec<HostCall> = emu.profiler.as_ref().unwrap().host.get(host_base..).unwrap_or_default().to_vec();

    // Innermost first.
    let mut stack = vec![guest_frame(emu, emu.cpu.pc)];
    let (lr, mut fp) = (emu.cpu.x[30], emu.cpu.x[29]);
    let first_ret = emu.read_u64(fp.wrapping_add(8)).unwrap_or(0);
    // A leaf function that hasn't saved a frame: its caller is only in lr.
    if lr != HOST_RETURN && lr != first_ret && lr > 4 && function_of(emu, lr) != function_of(emu, emu.cpu.pc) {
        stack.push(guest_frame(emu, lr - 4));
    }
    for _ in 0..1024 {
        if fp == 0 || fp % 8 != 0 {
            break;
        }
        let (Ok(next), Ok(ret)) = (emu.read_u64(fp), emu.read_u64(fp + 8)) else { break };
        if ret == HOST_RETURN {
            // Guest code a host function called: that host function comes next, then its caller.
            if let Some(h) = host.pop() {
                push_host_call(emu, &mut stack, h);
            }
        } else if ret == 0 {
            break;
        } else {
            stack.push(guest_frame(emu, ret - 4));
        }
        // Callers' frames are higher on the stack.
        if next <= fp {
            break;
        }
        fp = next;
    }
    // Host functions further out (the run loop's callouts called from the host's own loop).
    while let Some(h) = host.pop() {
        push_host_call(emu, &mut stack, h);
    }
    stack.reverse();

    let p = emu.profiler.as_mut().unwrap();
    let id = match p.stack_ids.get(&stack) {
        Some(&i) => i,
        None => {
            let i = p.stacks.len() as u32;
            p.stacks.push(stack.clone());
            p.stack_ids.insert(stack, i);
            i
        }
    };
    let samples = p.threads.entry(tid).or_default();
    match samples.last_mut() {
        Some((last, w)) if *last == id => *w += every,
        _ => samples.push((id, every)),
    }
}

impl Profiler {
    /// The profile in speedscope's file format (JSON), one profile per
    /// thread, weighted in instructions. `names` gives each thread's name.
    pub fn speedscope(&self, title: &str, names: impl Fn(usize) -> String) -> String {
        use serde_json::json;
        let frames: Vec<_> = self.frames.iter().map(|(name, file)| json!({ "name": name, "file": file })).collect();
        let mut tids: Vec<_> = self.threads.keys().copied().collect();
        tids.sort();
        let profiles: Vec<_> = tids
            .iter()
            .map(|tid| {
                let samples = &self.threads[tid];
                let total: u64 = samples.iter().map(|s| s.1).sum();
                json!({
                    "type": "sampled",
                    "name": format!("thread {tid} {}", names(*tid)),
                    "unit": "none",
                    "startValue": 0,
                    "endValue": total,
                    "samples": samples.iter().map(|(s, _)| &self.stacks[*s as usize]).collect::<Vec<_>>(),
                    "weights": samples.iter().map(|s| s.1).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "$schema": "https://www.speedscope.app/file-format-schema.json",
            "name": title,
            "exporter": "pomme",
            "activeProfileIndex": 0,
            "shared": { "frames": frames },
            "profiles": profiles,
        })
        .to_string()
    }

    /// The `n` functions with the most instructions of their own (self) and
    /// counting what they call (total), as text.
    pub fn summary(&self, n: usize) -> String {
        let mut own: HashMap<u32, u64> = HashMap::new();
        let mut total: HashMap<u32, u64> = HashMap::new();
        let mut all = 0;
        for &(s, w) in self.threads.values().flatten() {
            let stack = &self.stacks[s as usize];
            all += w;
            if let Some(&leaf) = stack.last() {
                *own.entry(leaf).or_default() += w;
            }
            let mut seen = Vec::new();
            for &f in stack {
                if !seen.contains(&f) {
                    seen.push(f);
                    *total.entry(f).or_default() += w;
                }
            }
        }
        let all = all.max(1) as f64;
        let table = |m: HashMap<u32, u64>| {
            let mut rows: Vec<_> = m.into_iter().collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            rows.into_iter()
                .take(n)
                .map(|(f, w)| {
                    let (name, file) = &self.frames[f as usize];
                    format!("  {:5.1}%  {name}  ({file})\n", w as f64 * 100.0 / all)
                })
                .collect::<String>()
        };
        format!("self (instructions in the function itself):\n{}total (with what it calls):\n{}", table(own), table(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(display_name("_objc_msgSend"), "objc_msgSend");
        assert_eq!(display_name("__ZN6hermes2vm8JSObject14addOwnPropertyENS0_6HandleIS1_EE"), "hermes::vm::JSObject::addOwnProperty");
        assert_eq!(display_name("+[RCTJSThreadManager runRunLoop]"), "+[RCTJSThreadManager runRunLoop]");
        assert_eq!(display_name("___ZN8facebook5react16RCTMessageThread8runAsyncENSt3__18functionIFvvEEE_block_invoke"), "block in facebook::react::RCTMessageThread::runAsync");
        assert_eq!(display_name("___40-[RCTBridge loadSource:]_block_invoke_2"), "block in -[RCTBridge loadSource:]");
    }
}
