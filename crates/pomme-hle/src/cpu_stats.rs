//! Host time spent inside the CPU, and the CPU's exits to pomme's Rust code
//! (the `cpu-stats` feature, off by default). Only counts: results are the same.

use pomme_cpu::Exit;
use std::cell::RefCell;
use std::time::Duration;

#[derive(Default, Debug, Clone)]
pub struct CpuStats {
    /// Host time inside `Cpu::run`.
    pub in_cpu: Duration,
    /// Exits to the host: stub calls (`BRK` on a stub), other `BRK`s (hooks, thunks), `SVC`s, the rest.
    pub stub_exits: u64,
    pub brk_exits: u64,
    pub svc_exits: u64,
    pub other_exits: u64,
}

thread_local! {
    static STATS: RefCell<CpuStats> = RefCell::new(CpuStats::default());
}

pub(crate) fn add(d: Duration) {
    STATS.with(|s| s.borrow_mut().in_cpu += d);
}

pub(crate) fn exit(exit: &Exit, stub: bool) {
    STATS.with(|s| {
        let mut s = s.borrow_mut();
        match exit {
            Exit::Brk { .. } if stub => s.stub_exits += 1,
            Exit::Brk { .. } => s.brk_exits += 1,
            Exit::Svc { .. } => s.svc_exits += 1,
            _ => s.other_exits += 1,
        }
    });
}

pub fn snapshot() -> CpuStats {
    STATS.with(|s| s.borrow().clone())
}
