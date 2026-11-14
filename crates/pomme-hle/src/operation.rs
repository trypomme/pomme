//! NSOperation, NSBlockOperation and NSOperationQueue.
//!
//! An operation is ready once its dependencies have finished (a cancelled one
//! is ready at once). Its default `start` runs `main` between the executing
//! and finished states; asynchronous subclasses override `start` and say when
//! they're done with KVO on `isFinished` (`willChange…`/`didChangeValueForKey:`),
//! which is how Foundation learns it too. When an operation finishes, its
//! completion block runs on a background thread and the queues start what's
//! now ready.
//!
//! A queue starts its ready operations, highest priority first, up to
//! `maxConcurrentOperationCount` at once, each on its own thread (the main
//! queue's on the main thread; with an `underlyingQueue`, on that dispatch
//! queue), unless it's suspended.

use crate::foundation;
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// NSOperationQueueDefaultMaxConcurrentOperationCount: the system decides; this many.
const SYSTEM_MAX: usize = 64;

#[derive(Default, Clone)]
struct Op {
    executing: bool,
    finished: bool,
    cancelled: bool,
    /// Handed to a thread (or `start` was called).
    started: bool,
    dependencies: Vec<u64>,
    completion: u64,
    /// NSBlockOperation's blocks.
    blocks: Vec<u64>,
    priority: i64,
    qos: i64,
    name: u64,
    queue: u64,
}

#[derive(Default, Clone)]
struct Queue {
    /// Operations not yet finished, in the order they were added.
    ops: Vec<u64>,
    running: usize,
    /// -1: the system default.
    max: i64,
    suspended: bool,
    name: u64,
    main: bool,
    qos: i64,
    /// The last barrier added (addBarrierBlock:): later operations wait for it.
    barrier: u64,
    /// The dispatch queue operations run on (`underlyingQueue`), or 0.
    underlying: u64,
}

#[derive(Clone, Default)]
pub struct State {
    ops: HashMap<u64, Op>,
    queues: HashMap<u64, Queue>,
    main_queue: u64,
    /// The queue each thread is running an operation for (NSOperationQueue.currentQueue).
    current: HashMap<usize, u64>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn op(emu: &mut Emu, o: u64) -> &mut Op {
    emu.ops.ops.entry(o).or_default()
}

fn queue(emu: &mut Emu, q: u64) -> &mut Queue {
    emu.ops.queues.entry(q).or_insert_with(|| Queue { max: -1, ..Default::default() })
}

fn flag(emu: &mut Emu, o: u64, sel: &str) -> Result<bool> {
    Ok(send(emu, o, sel, &[])? & 0xff != 0)
}

fn kvo(emu: &mut Emu, o: u64, key: &str, change: impl FnOnce(&mut Op)) -> Result<()> {
    let k = foundation::new_nsstring(emu, key)?;
    send(emu, o, "willChangeValueForKey:", &[k])?;
    change(op(emu, o));
    send(emu, o, "didChangeValueForKey:", &[k])?;
    Ok(())
}

// ---- NSOperation ----

/// `-start`: run `main` unless cancelled, between executing and finished.
fn start(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    op(emu, o).started = true;
    if !flag(emu, o, "isCancelled")? {
        kvo(emu, o, "isExecuting", |s| s.executing = true)?;
        send(emu, o, "main", &[])?;
        kvo(emu, o, "isExecuting", |s| s.executing = false)?;
    }
    kvo(emu, o, "isFinished", |s| s.finished = true)
}

/// `-isReady`: every dependency finished (a cancelled operation needn't wait).
fn is_ready(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let s = op(emu, o).clone();
    let mut ready = true;
    if !s.cancelled {
        for d in s.dependencies {
            if !flag(emu, d, "isFinished")? {
                ready = false;
                break;
            }
        }
    }
    ret(emu, ready as u64)
}

/// `-didChangeValueForKey:`: an operation that says it finished is done:
/// its completion block runs and the queues move on.
fn did_change(emu: &mut Emu) -> Result<()> {
    let (o, key) = (emu.arg(0), emu.arg(2));
    let k = foundation::nsstring(emu, key)?;
    if k == "isFinished" || k == "finished" {
        if flag(emu, o, "isFinished")? {
            finished(emu, o)?;
        }
    } else if k == "isReady" || k == "ready" {
        pump_all(emu)?;
    }
    Ok(())
}

fn finished(emu: &mut Emu, o: u64) -> Result<()> {
    let s = op(emu, o);
    if s.queue == u64::MAX {
        return Ok(()); // already handled
    }
    let (q, completion) = (s.queue, s.completion);
    s.queue = u64::MAX;
    if q != 0 {
        let qs = queue(emu, q);
        qs.ops.retain(|&x| x != o);
        qs.running = qs.running.saturating_sub(1);
    }
    if completion != 0 {
        let invoke = emu.read_u64(completion + 16)?;
        crate::threads::spawn(emu, invoke, completion, "NSOperation completion")?;
    }
    pump_all(emu)
}

// ---- NSOperationQueue ----

/// The thread an operation runs on: `[op start]`.
fn run_operation(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let q = op(emu, o).queue;
    let tid = emu.th.current;
    emu.ops.current.insert(tid, q);
    send(emu, o, "start", &[])?;
    emu.ops.current.remove(&tid);
    Ok(())
}

fn pump_all(emu: &mut Emu) -> Result<()> {
    let qs: Vec<u64> = emu.ops.queues.keys().copied().collect();
    for q in qs {
        pump(emu, q)?;
    }
    Ok(())
}

/// Start what the queue can: ready operations, by priority, up to its limit.
fn pump(emu: &mut Emu, q: u64) -> Result<()> {
    loop {
        let s = queue(emu, q).clone();
        let max = if s.max < 0 { SYSTEM_MAX } else { s.max as usize };
        if s.suspended || s.running >= max {
            return Ok(());
        }
        let mut best: Option<(i64, u64)> = None;
        for o in s.ops {
            if op(emu, o).started || !flag(emu, o, "isReady")? {
                continue;
            }
            let p = op(emu, o).priority;
            if best.is_none_or(|(bp, _)| p > bp) {
                best = Some((p, o));
            }
        }
        let Some((_, o)) = best else { return Ok(()) };
        op(emu, o).started = true;
        queue(emu, q).running += 1;
        let f = emu.host_thunk(run_operation)?;
        if s.main {
            crate::libsystem::run_on_main(emu, f, o);
        } else if s.underlying != 0 {
            crate::libsystem::dispatch_on(emu, s.underlying, f, o)?;
        } else {
            crate::threads::spawn(emu, f, o, "NSOperationQueue")?;
        }
    }
}

fn add_operation(emu: &mut Emu, q: u64, o: u64) -> Result<()> {
    if o == 0 {
        return Ok(());
    }
    send(emu, o, "retain", &[])?;
    let barrier = queue(emu, q).barrier;
    if barrier != 0 && barrier != o && !op(emu, barrier).finished && !op(emu, o).dependencies.contains(&barrier) {
        op(emu, o).dependencies.push(barrier);
    }
    op(emu, o).queue = q;
    queue(emu, q).ops.push(o);
    pump(emu, q)
}

fn new_block_operation(emu: &mut Emu, block: u64) -> Result<u64> {
    let cls = objc::host_class(emu, "NSBlockOperation")?;
    let o = send(emu, cls, "new", &[])?;
    if block != 0 {
        let b = crate::libc::copy_block(emu, block)?;
        op(emu, o).blocks.push(b);
    }
    Ok(o)
}

fn main_queue(emu: &mut Emu) -> Result<u64> {
    if emu.ops.main_queue == 0 {
        let cls = objc::host_class(emu, "NSOperationQueue")?;
        let q = send(emu, cls, "new", &[])?;
        let s = queue(emu, q);
        s.main = true;
        s.max = 1;
        emu.ops.main_queue = q;
        let name = foundation::new_nsstring(emu, "NSOperationQueue Main Queue")?;
        queue(emu, q).name = name;
    }
    Ok(emu.ops.main_queue)
}

const O: &str = "NSOperation";
const B: &str = "NSBlockOperation";
const Q: &str = "NSOperationQueue";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (O, false, "init", |e| {
        let o = e.arg(0);
        op(e, o);
        ret(e, o)
    }),
    (O, false, "start", start),
    (O, false, "main", |_| Ok(())),
    (O, false, "isReady", is_ready),
    (O, false, "isExecuting", |e| {
        let v = op(e, e.arg(0)).executing;
        ret(e, v as u64)
    }),
    (O, false, "isFinished", |e| {
        let v = op(e, e.arg(0)).finished;
        ret(e, v as u64)
    }),
    (O, false, "isCancelled", |e| {
        let v = op(e, e.arg(0)).cancelled;
        ret(e, v as u64)
    }),
    (O, false, "isConcurrent", |e| ret(e, 0)),
    (O, false, "isAsynchronous", |e| ret(e, 0)),
    (O, false, "cancel", |e| {
        let o = e.arg(0);
        if !op(e, o).cancelled {
            kvo(e, o, "isCancelled", |s| s.cancelled = true)?;
            // A cancelled operation is ready to be started (and finish) at once.
            pump_all(e)?;
        }
        Ok(())
    }),
    (O, false, "addDependency:", |e| {
        let (o, d) = (e.arg(0), e.arg(2));
        if d != 0 && !op(e, o).dependencies.contains(&d) {
            op(e, o).dependencies.push(d);
        }
        Ok(())
    }),
    (O, false, "removeDependency:", |e| {
        let (o, d) = (e.arg(0), e.arg(2));
        op(e, o).dependencies.retain(|&x| x != d);
        pump_all(e)
    }),
    (O, false, "dependencies", |e| {
        let deps = op(e, e.arg(0)).dependencies.clone();
        let a = foundation::new_array_of(e, &deps)?;
        ret(e, a)
    }),
    (O, false, "completionBlock", |e| {
        let b = op(e, e.arg(0)).completion;
        ret(e, b)
    }),
    (O, false, "setCompletionBlock:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        let b = if b == 0 { 0 } else { crate::libc::copy_block(e, b)? };
        op(e, o).completion = b;
        Ok(())
    }),
    (O, false, "queuePriority", |e| {
        let p = op(e, e.arg(0)).priority;
        ret(e, p as u64)
    }),
    (O, false, "setQueuePriority:", |e| {
        let (o, p) = (e.arg(0), e.arg(2) as i64);
        op(e, o).priority = p;
        Ok(())
    }),
    (O, false, "qualityOfService", |e| {
        let q = op(e, e.arg(0)).qos;
        ret(e, if q == 0 { u64::MAX } else { q as u64 }) // NSQualityOfServiceDefault (-1)
    }),
    (O, false, "setQualityOfService:", |e| {
        let (o, q) = (e.arg(0), e.arg(2) as i64);
        op(e, o).qos = q;
        Ok(())
    }),
    (O, false, "name", |e| {
        let n = op(e, e.arg(0)).name;
        ret(e, n)
    }),
    (O, false, "setName:", |e| {
        let (o, n) = (e.arg(0), e.arg(2));
        let n = send(e, n, "copy", &[])?;
        op(e, o).name = n;
        Ok(())
    }),
    (O, false, "didChangeValueForKey:", did_change),
    (B, true, "blockOperationWithBlock:", |e| {
        let o = new_block_operation(e, e.arg(2))?;
        ret(e, o)
    }),
    (B, false, "addExecutionBlock:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        let b = crate::libc::copy_block(e, b)?;
        op(e, o).blocks.push(b);
        Ok(())
    }),
    (B, false, "executionBlocks", |e| {
        let blocks = op(e, e.arg(0)).blocks.clone();
        let a = foundation::new_array_of(e, &blocks)?;
        ret(e, a)
    }),
    // Its blocks, one after another (Foundation may run them concurrently).
    (B, false, "main", |e| {
        let blocks = op(e, e.arg(0)).blocks.clone();
        for b in blocks {
            crate::libsystem::call_block(e, b, &[])?;
        }
        Ok(())
    }),
    (Q, true, "mainQueue", |e| {
        let q = main_queue(e)?;
        ret(e, q)
    }),
    (Q, true, "currentQueue", |e| {
        let tid = e.th.current;
        let q = match e.ops.current.get(&tid) {
            Some(&q) => q,
            None if tid == 0 => main_queue(e)?,
            None => 0,
        };
        ret(e, q)
    }),
    (Q, false, "init", |e| {
        let q = e.arg(0);
        queue(e, q);
        ret(e, q)
    }),
    (Q, false, "addOperation:", |e| {
        let (q, o) = (e.arg(0), e.arg(2));
        add_operation(e, q, o)
    }),
    (Q, false, "addOperationWithBlock:", |e| {
        let (q, b) = (e.arg(0), e.arg(2));
        let o = new_block_operation(e, b)?;
        add_operation(e, q, o)
    }),
    // (Waiting isn't supported yet: with waitUntilFinished YES this stops, rather than not waiting.)
    (Q, false, "addOperations:waitUntilFinished:", |e| {
        let (q, list, wait) = (e.arg(0), e.arg(2), e.arg(3) & 0xff != 0);
        if wait {
            return Err(crate::Stop::Unsupported("-[NSOperationQueue addOperations:waitUntilFinished:YES]".into()));
        }
        let n = send(e, list, "count", &[])?;
        for i in 0..n {
            let o = send(e, list, "objectAtIndex:", &[i])?;
            add_operation(e, q, o)?;
        }
        Ok(())
    }),
    // A barrier: runs once everything added before it has finished; what's added after waits for it.
    (Q, false, "addBarrierBlock:", |e| {
        let (q, b) = (e.arg(0), e.arg(2));
        let o = new_block_operation(e, b)?;
        let before = queue(e, q).ops.clone();
        op(e, o).dependencies.extend(before);
        add_operation(e, q, o)?;
        queue(e, q).barrier = o;
        Ok(())
    }),
    (Q, false, "operations", |e| {
        let ops = queue(e, e.arg(0)).ops.clone();
        let a = foundation::new_array_of(e, &ops)?;
        ret(e, a)
    }),
    (Q, false, "operationCount", |e| {
        let n = queue(e, e.arg(0)).ops.len();
        ret(e, n as u64)
    }),
    (Q, false, "maxConcurrentOperationCount", |e| {
        let m = queue(e, e.arg(0)).max;
        ret(e, m as u64)
    }),
    (Q, false, "setMaxConcurrentOperationCount:", |e| {
        let (q, m) = (e.arg(0), e.arg(2) as i64);
        queue(e, q).max = m;
        pump(e, q)
    }),
    (Q, false, "isSuspended", |e| {
        let s = queue(e, e.arg(0)).suspended;
        ret(e, s as u64)
    }),
    (Q, false, "setSuspended:", |e| {
        let (q, s) = (e.arg(0), e.arg(2) & 0xff != 0);
        queue(e, q).suspended = s;
        pump(e, q)
    }),
    (Q, false, "cancelAllOperations", |e| {
        let ops = queue(e, e.arg(0)).ops.clone();
        for o in ops {
            send(e, o, "cancel", &[])?;
        }
        Ok(())
    }),
    (Q, false, "name", |e| {
        let n = queue(e, e.arg(0)).name;
        ret(e, n)
    }),
    (Q, false, "setName:", |e| {
        let (q, n) = (e.arg(0), e.arg(2));
        let n = send(e, n, "copy", &[])?;
        queue(e, q).name = n;
        Ok(())
    }),
    (Q, false, "underlyingQueue", |e| {
        let q = e.arg(0);
        let u = queue(e, q).underlying;
        ret(e, u)
    }),
    (Q, false, "setUnderlyingQueue:", |e| {
        let (q, u) = (e.arg(0), e.arg(2));
        queue(e, q).underlying = u;
        Ok(())
    }),
    (Q, false, "qualityOfService", |e| {
        let q = queue(e, e.arg(0)).qos;
        ret(e, if q == 0 { u64::MAX } else { q as u64 })
    }),
    (Q, false, "setQualityOfService:", |e| {
        let (q, v) = (e.arg(0), e.arg(2) as i64);
        queue(e, q).qos = v;
        Ok(())
    }),
];
