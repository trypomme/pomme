//! NSPointerArray: an ordered list of pointers that may hold NULL, with weak
//! (unretained) or strong (retained) object entries.
//!
//! Weak entries are plain pointers here, like pomme's other weak references
//! (objc_storeWeak): they aren't zeroed when the object is deallocated.

use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    /// Array → (entries, strong).
    arrays: HashMap<u64, (Vec<u64>, bool)>,
}

/// NSPointerFunctionsOptions: StrongMemory 0, WeakMemory 5 (OpaqueMemory 2).
const WEAK_MEMORY: u64 = 5;
const OPAQUE_MEMORY: u64 = 2;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn make(emu: &mut Emu, strong: bool) -> Result<u64> {
    let cls = host_class(emu, "NSPointerArray")?;
    let a = objc::create_instance(emu, cls, 0)?;
    emu.pointer_arrays.arrays.insert(a, (Vec::new(), strong));
    Ok(a)
}

fn is_strong(options: u64) -> bool {
    options & 0xff != WEAK_MEMORY && options & 0xff != OPAQUE_MEMORY
}

fn entries(emu: &Emu, a: u64) -> (Vec<u64>, bool) {
    emu.pointer_arrays.arrays.get(&a).cloned().unwrap_or_default()
}

fn set_entries(emu: &mut Emu, a: u64, v: Vec<u64>) {
    emu.pointer_arrays.arrays.entry(a).or_default().0 = v;
}

fn hold(emu: &mut Emu, strong: bool, p: u64) -> Result<()> {
    if strong && p != 0 {
        send(emu, p, "retain", &[])?;
    }
    Ok(())
}

fn drop_ref(emu: &mut Emu, strong: bool, p: u64) -> Result<()> {
    if strong && p != 0 {
        send(emu, p, "release", &[])?;
    }
    Ok(())
}

const P: &str = "NSPointerArray";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (P, true, "weakObjectsPointerArray", |e| {
        let a = make(e, false)?;
        send(e, a, "autorelease", &[])?;
        ret(e, a)
    }),
    (P, true, "strongObjectsPointerArray", |e| {
        let a = make(e, true)?;
        send(e, a, "autorelease", &[])?;
        ret(e, a)
    }),
    (P, true, "pointerArrayWithOptions:", |e| {
        let strong = is_strong(e.arg(2));
        let a = make(e, strong)?;
        send(e, a, "autorelease", &[])?;
        ret(e, a)
    }),
    (P, false, "initWithOptions:", |e| {
        let (a, strong) = (e.arg(0), is_strong(e.arg(2)));
        e.pointer_arrays.arrays.insert(a, (Vec::new(), strong));
        ret(e, a)
    }),
    (P, false, "init", |e| {
        let a = e.arg(0);
        e.pointer_arrays.arrays.insert(a, (Vec::new(), true));
        ret(e, a)
    }),
    (P, false, "count", |e| {
        let n = entries(e, e.arg(0)).0.len() as u64;
        ret(e, n)
    }),
    (P, false, "setCount:", |e| {
        let (a, n) = (e.arg(0), e.arg(2) as usize);
        let (mut v, strong) = entries(e, a);
        for p in v.split_off(n.min(v.len())) {
            drop_ref(e, strong, p)?;
        }
        v.resize(n, 0);
        set_entries(e, a, v);
        Ok(())
    }),
    (P, false, "pointerAtIndex:", |e| {
        let (a, i) = (e.arg(0), e.arg(2) as usize);
        let p = entries(e, a).0.get(i).copied().unwrap_or(0);
        ret(e, p)
    }),
    (P, false, "addPointer:", |e| {
        let (a, p) = (e.arg(0), e.arg(2));
        let (mut v, strong) = entries(e, a);
        hold(e, strong, p)?;
        v.push(p);
        set_entries(e, a, v);
        Ok(())
    }),
    (P, false, "insertPointer:atIndex:", |e| {
        let (a, p, i) = (e.arg(0), e.arg(2), e.arg(3) as usize);
        let (mut v, strong) = entries(e, a);
        hold(e, strong, p)?;
        v.insert(i.min(v.len()), p);
        set_entries(e, a, v);
        Ok(())
    }),
    (P, false, "removePointerAtIndex:", |e| {
        let (a, i) = (e.arg(0), e.arg(2) as usize);
        let (mut v, strong) = entries(e, a);
        if i < v.len() {
            let p = v.remove(i);
            drop_ref(e, strong, p)?;
        }
        set_entries(e, a, v);
        Ok(())
    }),
    (P, false, "replacePointerAtIndex:withPointer:", |e| {
        let (a, i, p) = (e.arg(0), e.arg(2) as usize, e.arg(3));
        let (mut v, strong) = entries(e, a);
        if i < v.len() {
            hold(e, strong, p)?;
            let old = std::mem::replace(&mut v[i], p);
            drop_ref(e, strong, old)?;
        }
        set_entries(e, a, v);
        Ok(())
    }),
    (P, false, "compact", |e| {
        let a = e.arg(0);
        let (mut v, _) = entries(e, a);
        v.retain(|&p| p != 0);
        set_entries(e, a, v);
        Ok(())
    }),
    // for-in visits every entry, NULLs included, as Foundation's does.
    (P, false, "countByEnumeratingWithState:objects:count:", |e| {
        let a = e.arg(0);
        crate::foundation::fast_enumerate(e, |e| Ok(entries(e, a).0))
    }),
    (P, false, "allObjects", |e| {
        let v: Vec<u64> = entries(e, e.arg(0)).0.into_iter().filter(|&p| p != 0).collect();
        let arr = crate::foundation::new_array_of(e, &v)?;
        ret(e, arr)
    }),
];
