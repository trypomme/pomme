//! Locks and one-time initialization: pthread mutexes and read-write locks,
//! `os_unfair_lock` (and its recursive form), `OSSpinLock`, `pthread_once` and
//! `dispatch_once`.
//!
//! Threads are preempted (see threads.rs), so a critical section really can
//! be interrupted half-way: a lock must keep other threads out until it's
//! released. Each lock's state (its owner, how deep a recursive owner has
//! locked it, its readers) lives here, by the lock's address. A thread that
//! can't take a lock blocks ([`crate::threads::Wait::Lock`]) and holds it when
//! its call returns.
//!
//! As on Darwin:
//! - A mutex is normal, recursive or error-checking. Its type comes from
//!   `pthread_mutexattr_settype`, or from the signature of the static
//!   initializer it was made with.
//! - An error-checking or recursive mutex reports EDEADLK or EPERM where a
//!   normal one deadlocks or misbehaves.
//! - `os_unfair_lock` stores its owner's thread port in the lock word, and
//!   aborts when it's unlocked by a thread that doesn't own it.
//! - A once-initializer runs exactly once; other callers wait until it has
//!   returned.

use crate::threads::{Wait, block};
use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;

const EBUSY: u64 = 16;
const EDEADLK: u64 = 11;
const EINVAL: u64 = 22;
const EPERM: u64 = 1;

// pthread_mutex_t signatures: the static initializers, and an initialized mutex.
const SIG_RECURSIVE_INIT: u64 = 0x32AA_ABA2;
const SIG_ERRORCHECK_INIT: u64 = 0x32AA_ABA1;
const SIG_MUTEX: u64 = 0x4D55_5458;

// pthread_mutexattr types.
const PTHREAD_MUTEX_NORMAL: u64 = 0;
const PTHREAD_MUTEX_ERRORCHECK: u64 = 1;
const PTHREAD_MUTEX_RECURSIVE: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
enum Kind {
    #[default]
    Normal,
    ErrorCheck,
    Recursive,
}

#[derive(Clone, Default, Debug)]
struct Lock {
    kind: Kind,
    owner: Option<usize>,
    depth: u32,
    /// Read-write locks: the threads reading (one entry per read lock taken).
    readers: Vec<usize>,
}

/// What a blocked thread is waiting to take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Want {
    /// A mutex or unfair lock (and the lock word to set to the owner, for unfair locks).
    Exclusive { word: bool },
    Read,
    Write,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    locks: HashMap<u64, Lock>,
    /// Mutex attributes' types, by the attribute's address.
    attr_types: HashMap<u64, u64>,
}

fn me(emu: &Emu) -> usize {
    emu.th.current
}

/// The thread port an unfair lock records as its owner.
fn port(tid: usize) -> u32 {
    0x2000 + tid as u32
}

/// A mutex's state, typed by its signature on first use.
fn mutex(emu: &mut Emu, m: u64) -> Result<&mut Lock> {
    if !emu.locks.locks.contains_key(&m) {
        let kind = match emu.read_u64(m)? {
            SIG_RECURSIVE_INIT => Kind::Recursive,
            SIG_ERRORCHECK_INIT => Kind::ErrorCheck,
            _ => Kind::Normal,
        };
        emu.locks.locks.insert(m, Lock { kind, ..Default::default() });
    }
    Ok(emu.locks.locks.get_mut(&m).expect("just inserted"))
}

/// Try to take lock `addr` for the current thread; whether it did.
pub(crate) fn try_take(emu: &mut Emu, addr: u64, want: Want) -> Result<bool> {
    let tid = me(emu);
    let l = emu.locks.locks.entry(addr).or_default();
    let took = match want {
        Want::Exclusive { .. } => match l.owner {
            None if l.readers.is_empty() => {
                l.owner = Some(tid);
                l.depth = 1;
                true
            }
            Some(o) if o == tid && l.kind == Kind::Recursive => {
                l.depth += 1;
                true
            }
            _ => false,
        },
        Want::Read => {
            if l.owner.is_none() {
                l.readers.push(tid);
                true
            } else {
                false
            }
        }
        Want::Write => {
            if l.owner.is_none() && l.readers.is_empty() {
                l.owner = Some(tid);
                l.depth = 1;
                true
            } else {
                false
            }
        }
    };
    if took && want == (Want::Exclusive { word: true }) {
        emu.p.memory.write_u32(addr, port(tid))?;
    }
    Ok(took)
}

/// Take lock `addr`, blocking until it's free. Ok(true) once held.
fn take(emu: &mut Emu, addr: u64, want: Want) -> Result<()> {
    if try_take(emu, addr, want)? {
        return Ok(());
    }
    block(emu, Wait::Lock { addr, want })?;
    Ok(())
}

/// Release one level of an exclusive lock the current thread holds.
fn release(emu: &mut Emu, addr: u64, word: bool) -> Result<bool> {
    let tid = me(emu);
    let Some(l) = emu.locks.locks.get_mut(&addr) else { return Ok(false) };
    if l.owner != Some(tid) {
        return Ok(false);
    }
    l.depth -= 1;
    if l.depth == 0 {
        l.owner = None;
        if word {
            emu.p.memory.write_u32(addr, 0)?;
        }
    }
    Ok(true)
}

// ---- pthread mutexes ----

fn pthread_mutexattr_init(emu: &mut Emu) -> Result<()> {
    emu.locks.attr_types.insert(emu.arg(0), PTHREAD_MUTEX_NORMAL);
    emu.ret(0);
    Ok(())
}

fn pthread_mutexattr_settype(emu: &mut Emu) -> Result<()> {
    let (attr, ty) = (emu.arg(0), emu.arg(1) & 0xffff_ffff);
    if ty > PTHREAD_MUTEX_RECURSIVE {
        emu.ret(EINVAL);
        return Ok(());
    }
    emu.locks.attr_types.insert(attr, ty);
    emu.ret(0);
    Ok(())
}

fn pthread_mutexattr_gettype(emu: &mut Emu) -> Result<()> {
    let (attr, out) = (emu.arg(0), emu.arg(1));
    let ty = emu.locks.attr_types.get(&attr).copied().unwrap_or(PTHREAD_MUTEX_NORMAL);
    emu.p.memory.write_u32(out, ty as u32)?;
    emu.ret(0);
    Ok(())
}

fn pthread_mutexattr_destroy(emu: &mut Emu) -> Result<()> {
    emu.locks.attr_types.remove(&emu.arg(0));
    emu.ret(0);
    Ok(())
}

/// `pthread_mutex_init(mutex, attr)`
fn pthread_mutex_init(emu: &mut Emu) -> Result<()> {
    let (m, attr) = (emu.arg(0), emu.arg(1));
    let kind = match emu.locks.attr_types.get(&attr).copied().unwrap_or(PTHREAD_MUTEX_NORMAL) {
        PTHREAD_MUTEX_ERRORCHECK => Kind::ErrorCheck,
        PTHREAD_MUTEX_RECURSIVE => Kind::Recursive,
        _ => Kind::Normal,
    };
    emu.write_u64(m, SIG_MUTEX)?;
    emu.locks.locks.insert(m, Lock { kind, ..Default::default() });
    emu.ret(0);
    Ok(())
}

fn pthread_mutex_destroy(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    if emu.locks.locks.get(&m).is_some_and(|l| l.owner.is_some()) {
        emu.ret(EBUSY);
        return Ok(());
    }
    emu.locks.locks.remove(&m);
    emu.ret(0);
    Ok(())
}

fn pthread_mutex_lock(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    let tid = me(emu);
    let l = mutex(emu, m)?;
    if l.owner == Some(tid) && l.kind == Kind::ErrorCheck {
        emu.ret(EDEADLK);
        return Ok(());
    }
    take(emu, m, Want::Exclusive { word: false })?;
    emu.ret(0);
    Ok(())
}

fn pthread_mutex_trylock(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    mutex(emu, m)?;
    let took = try_take(emu, m, Want::Exclusive { word: false })?;
    emu.ret(if took { 0 } else { EBUSY });
    Ok(())
}

fn pthread_mutex_unlock(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    mutex(emu, m)?;
    let ok = release(emu, m, false)?;
    emu.ret(if ok { 0 } else { EPERM });
    Ok(())
}

/// Release a mutex entirely for a condition wait: the depth it was held at.
pub(crate) fn release_for_wait(emu: &mut Emu, m: u64) -> Result<u32> {
    let tid = me(emu);
    let l = mutex(emu, m)?;
    if l.owner != Some(tid) {
        return Ok(0);
    }
    let depth = l.depth;
    l.owner = None;
    l.depth = 0;
    Ok(depth)
}

/// Take a mutex back after a condition wait, at the depth it was held (whether it could).
pub(crate) fn retake_after_wait(emu: &mut Emu, m: u64, depth: u32) -> Result<bool> {
    if depth == 0 {
        return Ok(true);
    }
    let tid = me(emu);
    let l = mutex(emu, m)?;
    if l.owner.is_some() || !l.readers.is_empty() {
        return Ok(false);
    }
    l.owner = Some(tid);
    l.depth = depth;
    Ok(true)
}

// ---- read-write locks ----

fn rwlock_init(emu: &mut Emu) -> Result<()> {
    emu.locks.locks.insert(emu.arg(0), Lock::default());
    emu.ret(0);
    Ok(())
}

fn rwlock_destroy(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    if emu.locks.locks.get(&l).is_some_and(|l| l.owner.is_some() || !l.readers.is_empty()) {
        emu.ret(EBUSY);
        return Ok(());
    }
    emu.locks.locks.remove(&l);
    emu.ret(0);
    Ok(())
}

fn rwlock_lock(emu: &mut Emu, want: Want) -> Result<()> {
    let l = emu.arg(0);
    let tid = me(emu);
    if emu.locks.locks.get(&l).is_some_and(|x| x.owner == Some(tid)) {
        emu.ret(EDEADLK);
        return Ok(());
    }
    take(emu, l, want)?;
    emu.ret(0);
    Ok(())
}

fn rwlock_trylock(emu: &mut Emu, want: Want) -> Result<()> {
    let took = try_take(emu, emu.arg(0), want)?;
    emu.ret(if took { 0 } else { EBUSY });
    Ok(())
}

fn rwlock_unlock(emu: &mut Emu) -> Result<()> {
    let addr = emu.arg(0);
    let tid = me(emu);
    let Some(l) = emu.locks.locks.get_mut(&addr) else {
        emu.ret(EPERM);
        return Ok(());
    };
    if l.owner == Some(tid) {
        l.owner = None;
        l.depth = 0;
    } else if let Some(i) = l.readers.iter().position(|&r| r == tid) {
        l.readers.swap_remove(i);
    } else {
        emu.ret(EPERM);
        return Ok(());
    }
    emu.ret(0);
    Ok(())
}

// ---- unfair locks and spin locks ----

fn unfair_lock(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    let tid = me(emu);
    if emu.locks.locks.get(&l).is_some_and(|x| x.owner == Some(tid)) {
        return Err(Stop::Unsupported("BUG IN CLIENT OF LIBPLATFORM: Trying to recursively lock an os_unfair_lock".into()));
    }
    take(emu, l, Want::Exclusive { word: true })
}

fn unfair_trylock(emu: &mut Emu) -> Result<()> {
    let took = try_take(emu, emu.arg(0), Want::Exclusive { word: true })?;
    emu.ret(took as u64);
    Ok(())
}

fn unfair_unlock(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    if !release(emu, l, true)? {
        return Err(Stop::Unsupported("BUG IN CLIENT OF LIBPLATFORM: Unlock of an os_unfair_lock not owned by current thread".into()));
    }
    Ok(())
}

fn unfair_assert_owner(emu: &mut Emu, owned: bool) -> Result<()> {
    let l = emu.arg(0);
    let tid = me(emu);
    let is = emu.locks.locks.get(&l).is_some_and(|x| x.owner == Some(tid));
    if is != owned {
        let what = if owned { "not owned by" } else { "owned by" };
        return Err(Stop::Unsupported(format!("BUG IN CLIENT OF LIBPLATFORM: Assertion failed: Lock unexpectedly {what} current thread")));
    }
    Ok(())
}

/// `os_unfair_recursive_lock`: {os_unfair_lock, uint32 count}, re-enterable by its owner.
fn recursive_unfair_lock(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    emu.locks.locks.entry(l).or_default().kind = Kind::Recursive;
    take(emu, l, Want::Exclusive { word: true })
}

fn recursive_unfair_trylock(emu: &mut Emu) -> Result<()> {
    let l = emu.arg(0);
    emu.locks.locks.entry(l).or_default().kind = Kind::Recursive;
    let took = try_take(emu, l, Want::Exclusive { word: true })?;
    emu.ret(took as u64);
    Ok(())
}

// OSSpinLock: an int, non-zero while held.
fn spin_lock(emu: &mut Emu) -> Result<()> {
    take(emu, emu.arg(0), Want::Exclusive { word: true })
}

fn spin_try(emu: &mut Emu) -> Result<()> {
    let took = try_take(emu, emu.arg(0), Want::Exclusive { word: true })?;
    emu.ret(took as u64);
    Ok(())
}

fn spin_unlock(emu: &mut Emu) -> Result<()> {
    release(emu, emu.arg(0), true)?;
    Ok(())
}

// ---- once ----

/// `dispatch_once` and `dispatch_once_f`: the predicate is ~0 once done (code
/// checks that inline before calling), the running thread's port while the
/// initializer runs; other threads wait for it.
fn once(emu: &mut Emu, pred: u64, f: u64, ctx: u64) -> Result<()> {
    match emu.read_u64(pred)? {
        u64::MAX => Ok(()),
        0 => {
            emu.write_u64(pred, port(me(emu)) as u64)?;
            emu.call(f, &[ctx])?;
            emu.write_u64(pred, u64::MAX)?;
            Ok(())
        }
        running if running == port(me(emu)) as u64 => {
            Err(Stop::Unsupported("BUG IN CLIENT OF LIBDISPATCH: trying to lock recursively (dispatch_once called from its own initializer)".into()))
        }
        _ => {
            block(emu, Wait::Once { pred })?;
            Ok(())
        }
    }
}

fn dispatch_once(emu: &mut Emu) -> Result<()> {
    let (pred, b) = (emu.arg(0), emu.arg(1));
    let invoke = emu.read_u64(b + 16)?;
    once(emu, pred, invoke, b)
}

fn dispatch_once_f(emu: &mut Emu) -> Result<()> {
    let (pred, ctx, f) = (emu.arg(0), emu.arg(1), emu.arg(2));
    once(emu, pred, f, ctx)
}

/// `pthread_once(once, init)`: pthread_once_t is {long sig; char opaque[8]}.
/// The first opaque word is the initializing thread's port while init runs,
/// and ~0 when it's done.
fn pthread_once(emu: &mut Emu) -> Result<()> {
    let (o, init) = (emu.arg(0), emu.arg(1));
    once(emu, o + 8, init, 0)?;
    emu.ret(0);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_pthread_mutexattr_init", pthread_mutexattr_init),
    ("_pthread_mutexattr_settype", pthread_mutexattr_settype),
    ("_pthread_mutexattr_gettype", pthread_mutexattr_gettype),
    ("_pthread_mutexattr_destroy", pthread_mutexattr_destroy),
    ("_pthread_mutex_init", pthread_mutex_init),
    ("_pthread_mutex_destroy", pthread_mutex_destroy),
    ("_pthread_mutex_lock", pthread_mutex_lock),
    ("_pthread_mutex_trylock", pthread_mutex_trylock),
    ("_pthread_mutex_unlock", pthread_mutex_unlock),
    ("_pthread_rwlock_init", rwlock_init),
    ("_pthread_rwlock_destroy", rwlock_destroy),
    ("_pthread_rwlock_rdlock", |e| rwlock_lock(e, Want::Read)),
    ("_pthread_rwlock_wrlock", |e| rwlock_lock(e, Want::Write)),
    ("_pthread_rwlock_tryrdlock", |e| rwlock_trylock(e, Want::Read)),
    ("_pthread_rwlock_trywrlock", |e| rwlock_trylock(e, Want::Write)),
    ("_pthread_rwlock_unlock", rwlock_unlock),
    ("_os_unfair_lock_lock", unfair_lock),
    ("_os_unfair_lock_lock_with_options", unfair_lock),
    ("_os_unfair_lock_trylock", unfair_trylock),
    ("_os_unfair_lock_unlock", unfair_unlock),
    ("_os_unfair_lock_assert_owner", |e| unfair_assert_owner(e, true)),
    ("_os_unfair_lock_assert_not_owner", |e| unfair_assert_owner(e, false)),
    ("_os_unfair_recursive_lock_lock_with_options", recursive_unfair_lock),
    ("_os_unfair_recursive_lock_trylock", recursive_unfair_trylock),
    ("_os_unfair_recursive_lock_unlock", |e| {
        release(e, e.arg(0), true)?;
        Ok(())
    }),
    ("_OSSpinLockLock", spin_lock),
    ("_OSSpinLockTry", spin_try),
    ("_OSSpinLockUnlock", spin_unlock),
    ("_dispatch_once", dispatch_once),
    ("_dispatch_once_f", dispatch_once_f),
    ("_pthread_once", pthread_once),
];
