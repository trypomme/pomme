//! NSOrderedSet and NSMutableOrderedSet: an array whose objects are distinct
//! (by `isEqual:`), in the order they were added. The objects are retained
//! while they're members.

use crate::foundation;
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;

const NOT_FOUND: u64 = i64::MAX as u64;
/// NSEnumerationReverse.
const REVERSE: u64 = 1 << 1;

const S: &str = "NSOrderedSet";
const M: &str = "NSMutableOrderedSet";

#[derive(Default)]
pub(crate) struct State {
    sets: HashMap<u64, Vec<u64>>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn items(emu: &Emu, o: u64) -> Vec<u64> {
    emu.ordered_sets.sets.get(&o).cloned().unwrap_or_default()
}

fn equal(emu: &mut Emu, a: u64, b: u64) -> Result<bool> {
    Ok(a == b || (a != 0 && b != 0 && send(emu, a, "isEqual:", &[b])? & 0xff != 0))
}

/// Where `x` is in `list` (by isEqual:).
fn position(emu: &mut Emu, list: &[u64], x: u64) -> Result<Option<usize>> {
    for (i, &y) in list.iter().enumerate() {
        if equal(emu, x, y)? {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// `objs` without repeats, first occurrence kept.
fn distinct(emu: &mut Emu, objs: &[u64]) -> Result<Vec<u64>> {
    let mut out: Vec<u64> = Vec::with_capacity(objs.len());
    for &x in objs {
        if x != 0 && position(emu, &out, x)?.is_none() {
            out.push(x);
        }
    }
    Ok(out)
}

/// Make `o` hold `objs` (retained; what it held is released).
fn store(emu: &mut Emu, o: u64, objs: Vec<u64>) -> Result<()> {
    for &x in &objs {
        objc::objc_retain_obj(emu, x)?;
    }
    let old = emu.ordered_sets.sets.insert(o, objs).unwrap_or_default();
    for x in old {
        objc::objc_release_obj(emu, x)?;
    }
    Ok(())
}

/// A new ordered set of `class` holding `objs` (distinct already).
fn new_set(emu: &mut Emu, class: &str, objs: Vec<u64>) -> Result<u64> {
    let cls = objc::host_class(emu, class)?;
    let o = objc::create_instance(emu, cls, 0)?;
    store(emu, o, objs)?;
    Ok(o)
}

/// `+orderedSet…`: a new set of the receiving class (mutable for NSMutableOrderedSet and its subclasses).
fn make(emu: &mut Emu, objs: Vec<u64>) -> Result<()> {
    let cls = emu.arg(0);
    let mutable_cls = objc::host_class(emu, M)?;
    let mutable = send(emu, cls, "isSubclassOfClass:", &[mutable_cls])? & 0xff != 0;
    let objs = distinct(emu, &objs)?;
    let o = new_set(emu, if mutable { M } else { S }, objs)?;
    ret(emu, o)
}

/// `init…`: fill the receiver.
fn init_with(emu: &mut Emu, objs: Vec<u64>) -> Result<()> {
    let o = emu.arg(0);
    let objs = distinct(emu, &objs)?;
    store(emu, o, objs)?;
    ret(emu, o)
}

/// Objects from a C array (`…WithObjects:count:`).
fn c_array(emu: &mut Emu, p: u64, n: u64) -> Result<Vec<u64>> {
    (0..n).map(|i| emu.read_u64(p + i * 8)).collect()
}

/// A nil-terminated variadic list: the first in x2, the rest in stack slots.
fn var_list(emu: &mut Emu) -> Result<Vec<u64>> {
    let first = emu.arg(2);
    let mut out = Vec::new();
    if first == 0 {
        return Ok(out);
    }
    out.push(first);
    let mut at = emu.cpu.sp;
    loop {
        let x = emu.read_u64(at)?;
        if x == 0 {
            return Ok(out);
        }
        out.push(x);
        at += 8;
    }
}

/// Any collection's objects: an array, a set, or an ordered set.
fn objects_of(emu: &mut Emu, c: u64) -> Result<Vec<u64>> {
    if c == 0 {
        return Ok(Vec::new());
    }
    if emu.ordered_sets.sets.contains_key(&c) {
        return Ok(items(emu, c));
    }
    let set_cls = objc::host_class(emu, "NSSet")?;
    if send(emu, c, "isKindOfClass:", &[set_cls])? & 0xff != 0 {
        return foundation::set_values(emu, c);
    }
    foundation::array_values(emu, c)
}

fn out_of_range(i: u64, n: usize) -> Stop {
    Stop::Unsupported(format!("uncaught Objective-C exception NSRangeException: index {i} beyond bounds [0 .. {}]", n as i64 - 1))
}

fn at(emu: &mut Emu) -> Result<()> {
    let (o, i) = (emu.arg(0), emu.arg(2));
    let list = items(emu, o);
    match list.get(i as usize) {
        Some(&x) => ret(emu, x),
        None => Err(out_of_range(i, list.len())),
    }
}

fn index_of(emu: &mut Emu) -> Result<()> {
    let (o, x) = (emu.arg(0), emu.arg(2));
    let list = items(emu, o);
    let i = position(emu, &list, x)?.map_or(NOT_FOUND, |i| i as u64);
    ret(emu, i)
}

fn description(emu: &mut Emu) -> Result<()> {
    let list = items(emu, emu.arg(0));
    let mut parts = Vec::new();
    for x in list {
        let d = send(emu, x, "description", &[])?;
        parts.push(format!("    {}", foundation::nsstring(emu, d)?));
    }
    let text = if parts.is_empty() { "{(\n)}".to_string() } else { format!("{{(\n{}\n)}}", parts.join(",\n")) };
    let s = foundation::new_nsstring(emu, &text)?;
    ret(emu, s)
}

/// `enumerateObjectsUsingBlock:` / `…WithOptions:usingBlock:`: block(obj, idx, &stop).
fn each(emu: &mut Emu, options: u64, block: u64) -> Result<()> {
    let list = items(emu, emu.arg(0));
    let mut order: Vec<usize> = (0..list.len()).collect();
    if options & REVERSE != 0 {
        order.reverse();
    }
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for i in order {
        crate::libsystem::call_block(emu, block, &[list[i], i as u64, stop])?;
        if emu.p.memory.read(stop, 1)?[0] != 0 {
            break;
        }
    }
    emu.heap.free(&emu.p.memory, stop)?;
    Ok(())
}

// ---- mutation ----

fn mutate(emu: &mut Emu, f: impl FnOnce(&mut Emu, &mut Vec<u64>) -> Result<()>) -> Result<()> {
    let o = emu.arg(0);
    let mut list = items(emu, o);
    f(emu, &mut list)?;
    store(emu, o, list)
}

fn add_all(emu: &mut Emu, list: &mut Vec<u64>, objs: &[u64]) -> Result<()> {
    for &x in objs {
        if x != 0 && position(emu, list, x)?.is_none() {
            list.push(x);
        }
    }
    Ok(())
}

fn remove_all_of(emu: &mut Emu, list: &mut Vec<u64>, objs: &[u64]) -> Result<()> {
    let mut keep = Vec::with_capacity(list.len());
    for &x in list.iter() {
        if position(emu, objs, x)?.is_none() {
            keep.push(x);
        }
    }
    *list = keep;
    Ok(())
}

fn insert(emu: &mut Emu) -> Result<()> {
    let (x, i) = (emu.arg(2), emu.arg(3));
    mutate(emu, |emu, list| {
        if i as usize > list.len() {
            return Err(out_of_range(i, list.len() + 1));
        }
        if x != 0 && position(emu, list, x)?.is_none() {
            list.insert(i as usize, x);
        }
        Ok(())
    })
}

fn replace(emu: &mut Emu) -> Result<()> {
    let (i, x) = (emu.arg(2), emu.arg(3));
    mutate(emu, |emu, list| {
        if i as usize >= list.len() {
            return Err(out_of_range(i, list.len()));
        }
        // Replacing with an object that's elsewhere in the set does nothing.
        match position(emu, list, x)? {
            Some(j) if j != i as usize => {}
            _ => list[i as usize] = x,
        }
        Ok(())
    })
}

fn remove_at(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(2);
    mutate(emu, |_, list| {
        if i as usize >= list.len() {
            return Err(out_of_range(i, list.len()));
        }
        list.remove(i as usize);
        Ok(())
    })
}

/// `sortUsingComparator:`: a stable sort by block(a, b) → NSComparisonResult.
fn sort(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(2);
    mutate(emu, |emu, list| {
        for i in 1..list.len() {
            let mut j = i;
            while j > 0 && (crate::libsystem::call_block(emu, block, &[list[j - 1], list[j]])? as i64) > 0 {
                list.swap(j - 1, j);
                j -= 1;
            }
        }
        Ok(())
    })
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // Making one.
    (S, true, "orderedSet", |e| make(e, vec![])),
    (S, true, "orderedSetWithObject:", |e| {
        let x = e.arg(2);
        make(e, vec![x])
    }),
    (S, true, "orderedSetWithObjects:", |e| {
        let v = var_list(e)?;
        make(e, v)
    }),
    (S, true, "orderedSetWithObjects:count:", |e| {
        let v = c_array(e, e.arg(2), e.arg(3))?;
        make(e, v)
    }),
    (S, true, "orderedSetWithArray:", |e| {
        let v = objects_of(e, e.arg(2))?;
        make(e, v)
    }),
    (S, true, "orderedSetWithSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        make(e, v)
    }),
    (S, true, "orderedSetWithOrderedSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        make(e, v)
    }),
    (M, true, "orderedSetWithCapacity:", |e| make(e, vec![])),
    (S, false, "init", |e| init_with(e, vec![])),
    (M, false, "initWithCapacity:", |e| init_with(e, vec![])),
    (S, false, "initWithObject:", |e| {
        let x = e.arg(2);
        init_with(e, vec![x])
    }),
    (S, false, "initWithObjects:", |e| {
        let v = var_list(e)?;
        init_with(e, v)
    }),
    (S, false, "initWithObjects:count:", |e| {
        let v = c_array(e, e.arg(2), e.arg(3))?;
        init_with(e, v)
    }),
    (S, false, "initWithArray:", |e| {
        let v = objects_of(e, e.arg(2))?;
        init_with(e, v)
    }),
    (S, false, "initWithSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        init_with(e, v)
    }),
    (S, false, "initWithOrderedSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        init_with(e, v)
    }),
    (S, false, "copy", |e| {
        let v = items(e, e.arg(0));
        let o = new_set(e, S, v)?;
        ret(e, o)
    }),
    (S, false, "copyWithZone:", |e| {
        let v = items(e, e.arg(0));
        let o = new_set(e, S, v)?;
        ret(e, o)
    }),
    (S, false, "mutableCopy", |e| {
        let v = items(e, e.arg(0));
        let o = new_set(e, M, v)?;
        ret(e, o)
    }),
    (S, false, "mutableCopyWithZone:", |e| {
        let v = items(e, e.arg(0));
        let o = new_set(e, M, v)?;
        ret(e, o)
    }),
    // Reading.
    (S, false, "count", |e| {
        let n = items(e, e.arg(0)).len();
        ret(e, n as u64)
    }),
    (S, false, "objectAtIndex:", at),
    (S, false, "objectAtIndexedSubscript:", at),
    (S, false, "firstObject", |e| {
        let v = items(e, e.arg(0)).first().copied().unwrap_or(0);
        ret(e, v)
    }),
    (S, false, "lastObject", |e| {
        let v = items(e, e.arg(0)).last().copied().unwrap_or(0);
        ret(e, v)
    }),
    (S, false, "indexOfObject:", index_of),
    (S, false, "containsObject:", |e| {
        let (o, x) = (e.arg(0), e.arg(2));
        let list = items(e, o);
        let has = position(e, &list, x)?.is_some();
        ret(e, has as u64)
    }),
    (S, false, "objectsAtIndexes:", |e| {
        let (o, ix) = (e.arg(0), e.arg(2));
        let list = items(e, o);
        let mut out = Vec::new();
        for i in crate::indexset::indexes_of(e, ix) {
            out.push(*list.get(i as usize).ok_or_else(|| out_of_range(i, list.len()))?);
        }
        let a = foundation::new_array_of(e, &out)?;
        ret(e, a)
    }),
    (S, false, "array", |e| {
        let v = items(e, e.arg(0));
        let a = foundation::new_array_of(e, &v)?;
        ret(e, a)
    }),
    (S, false, "set", |e| {
        let v = items(e, e.arg(0));
        let a = foundation::new_array_of(e, &v)?;
        let cls = objc::host_class(e, "NSSet")?;
        let s = send(e, cls, "setWithArray:", &[a])?;
        ret(e, s)
    }),
    (S, false, "reversedOrderedSet", |e| {
        let mut v = items(e, e.arg(0));
        v.reverse();
        let o = new_set(e, S, v)?;
        ret(e, o)
    }),
    (S, false, "objectEnumerator", |e| {
        let v = items(e, e.arg(0));
        let en = foundation::new_enumerator(e, &v)?;
        ret(e, en)
    }),
    (S, false, "reverseObjectEnumerator", |e| {
        let mut v = items(e, e.arg(0));
        v.reverse();
        let en = foundation::new_enumerator(e, &v)?;
        ret(e, en)
    }),
    (S, false, "countByEnumeratingWithState:objects:count:", |e| {
        let o = e.arg(0);
        foundation::fast_enumerate(e, |e| Ok(items(e, o)))
    }),
    (S, false, "enumerateObjectsUsingBlock:", |e| {
        let b = e.arg(2);
        each(e, 0, b)
    }),
    (S, false, "enumerateObjectsWithOptions:usingBlock:", |e| {
        let (opts, b) = (e.arg(2), e.arg(3));
        each(e, opts, b)
    }),
    (S, false, "isEqualToOrderedSet:", |e| {
        let (a, b) = (items(e, e.arg(0)), objects_of(e, e.arg(2))?);
        let mut same = a.len() == b.len();
        for (x, y) in a.iter().zip(&b) {
            if !same {
                break;
            }
            same = equal(e, *x, *y)?;
        }
        ret(e, same as u64)
    }),
    (S, false, "isEqual:", |e| {
        let other = e.arg(2);
        if !e.ordered_sets.sets.contains_key(&other) {
            return ret(e, (other == e.arg(0)) as u64);
        }
        let (a, b) = (items(e, e.arg(0)), items(e, other));
        let mut same = a.len() == b.len();
        for (x, y) in a.iter().zip(&b) {
            if !same {
                break;
            }
            same = equal(e, *x, *y)?;
        }
        ret(e, same as u64)
    }),
    (S, false, "hash", |e| {
        let n = items(e, e.arg(0)).len();
        ret(e, n as u64)
    }),
    (S, false, "intersectsOrderedSet:", |e| {
        let (a, b) = (items(e, e.arg(0)), objects_of(e, e.arg(2))?);
        let mut any = false;
        for x in a {
            if position(e, &b, x)?.is_some() {
                any = true;
                break;
            }
        }
        ret(e, any as u64)
    }),
    (S, false, "isSubsetOfOrderedSet:", |e| {
        let (a, b) = (items(e, e.arg(0)), objects_of(e, e.arg(2))?);
        let mut all = true;
        for x in a {
            if position(e, &b, x)?.is_none() {
                all = false;
                break;
            }
        }
        ret(e, all as u64)
    }),
    (S, false, "description", description),
    (S, false, "debugDescription", description),
    // Changing a mutable one.
    (M, false, "addObject:", |e| {
        let x = e.arg(2);
        mutate(e, |e, list| add_all(e, list, &[x]))
    }),
    (M, false, "addObjectsFromArray:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| add_all(e, list, &v))
    }),
    (M, false, "unionOrderedSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| add_all(e, list, &v))
    }),
    (M, false, "unionSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| add_all(e, list, &v))
    }),
    (M, false, "insertObject:atIndex:", insert),
    (M, false, "replaceObjectAtIndex:withObject:", replace),
    (M, false, "setObject:atIndexedSubscript:", |e| {
        // (obj, idx): at the end it appends.
        let (x, i) = (e.arg(2), e.arg(3));
        if i as usize == items(e, e.arg(0)).len() {
            return mutate(e, |e, list| add_all(e, list, &[x]));
        }
        e.cpu.x[2] = i;
        e.cpu.x[3] = x;
        replace(e)
    }),
    (M, false, "removeObject:", |e| {
        let x = e.arg(2);
        mutate(e, |e, list| remove_all_of(e, list, &[x]))
    }),
    (M, false, "removeObjectAtIndex:", remove_at),
    (M, false, "removeObjectsInArray:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| remove_all_of(e, list, &v))
    }),
    (M, false, "minusOrderedSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| remove_all_of(e, list, &v))
    }),
    (M, false, "minusSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| remove_all_of(e, list, &v))
    }),
    (M, false, "intersectOrderedSet:", |e| {
        let v = objects_of(e, e.arg(2))?;
        mutate(e, |e, list| {
            let mut keep = Vec::new();
            for &x in list.iter() {
                if position(e, &v, x)?.is_some() {
                    keep.push(x);
                }
            }
            *list = keep;
            Ok(())
        })
    }),
    (M, false, "removeObjectsAtIndexes:", |e| {
        let ix = crate::indexset::indexes_of(e, e.arg(2));
        mutate(e, |_, list| {
            for &i in ix.iter().rev() {
                if (i as usize) < list.len() {
                    list.remove(i as usize);
                }
            }
            Ok(())
        })
    }),
    (M, false, "removeAllObjects", |e| mutate(e, |_, list| {
        list.clear();
        Ok(())
    })),
    (M, false, "sortUsingComparator:", sort),
];
