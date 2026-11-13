//! NSIndexSet and NSMutableIndexSet: sorted, disjoint, non-adjacent ranges
//! of unsigned integers (what Foundation keeps too), with the queries,
//! enumeration and mutation apps use (table and collection view updates,
//! acceptable HTTP status codes, …).

use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

const NOT_FOUND: u64 = i64::MAX as u64;
/// NSEnumerationReverse.
const REVERSE: u64 = 1 << 1;

/// Half-open ranges [start, end), sorted, merged when they touch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Indexes(Vec<(u64, u64)>);

impl Indexes {
    pub fn add(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        let end = start.saturating_add(len);
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let (mut s, mut e) = (start, end);
        let mut placed = false;
        for &(a, b) in &self.0 {
            if b < s {
                out.push((a, b));
            } else if a > e {
                if !placed {
                    out.push((s, e));
                    placed = true;
                }
                out.push((a, b));
            } else {
                s = s.min(a);
                e = e.max(b);
            }
        }
        if !placed {
            out.push((s, e));
        }
        self.0 = out;
    }

    pub fn remove(&mut self, start: u64, len: u64) {
        let end = start.saturating_add(len);
        let mut out = Vec::new();
        for &(a, b) in &self.0 {
            if b <= start || a >= end {
                out.push((a, b));
                continue;
            }
            if a < start {
                out.push((a, start));
            }
            if b > end {
                out.push((end, b));
            }
        }
        self.0 = out;
    }

    pub fn contains(&self, i: u64) -> bool {
        self.0.iter().any(|&(a, b)| a <= i && i < b)
    }

    pub fn count(&self) -> u64 {
        self.0.iter().map(|&(a, b)| b - a).sum()
    }

    pub fn count_in(&self, start: u64, len: u64) -> u64 {
        let end = start.saturating_add(len);
        self.0.iter().map(|&(a, b)| b.min(end).saturating_sub(a.max(start))).sum()
    }

    pub fn first(&self) -> u64 {
        self.0.first().map_or(NOT_FOUND, |r| r.0)
    }

    pub fn last(&self) -> u64 {
        self.0.last().map_or(NOT_FOUND, |r| r.1 - 1)
    }

    /// The first index above `i`.
    pub fn greater_than(&self, i: u64) -> u64 {
        for &(a, b) in &self.0 {
            if i.saturating_add(1) < b && i < u64::MAX {
                return a.max(i + 1);
            }
        }
        NOT_FOUND
    }

    /// The last index below `i`.
    pub fn less_than(&self, i: u64) -> u64 {
        for &(a, b) in self.0.iter().rev() {
            if a < i {
                return (b - 1).min(i - 1);
            }
        }
        NOT_FOUND
    }

    pub fn indexes(&self) -> impl DoubleEndedIterator<Item = u64> + '_ {
        self.0.iter().flat_map(|&(a, b)| a..b)
    }

    /// Insert `delta` empty slots at `at` (or remove them, negative), as
    /// `shiftIndexesStartingAtIndex:by:` does.
    pub fn shift(&mut self, at: u64, delta: i64) {
        let all: Vec<u64> = self.indexes().collect();
        let mut out = Indexes::default();
        for i in all {
            if i < at {
                out.add(i, 1);
            } else if delta >= 0 {
                out.add(i + delta as u64, 1);
            } else if i >= at + delta.unsigned_abs() {
                out.add(i - delta.unsigned_abs(), 1);
            } else {
                // Removed slots: indexes in them are dropped.
            }
        }
        *self = out;
    }
}

#[derive(Default)]
pub struct State {
    sets: HashMap<u64, Indexes>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// An NSIndexSet's indexes, ascending.
pub(crate) fn index_list(emu: &Emu, o: u64) -> Vec<u64> {
    set(emu, o).indexes().collect()
}

/// An NSIndexSet's indexes, in order (for other modules).
pub(crate) fn indexes_of(emu: &Emu, o: u64) -> Vec<u64> {
    set(emu, o).indexes().collect()
}

fn set(emu: &Emu, o: u64) -> Indexes {
    emu.indexsets.sets.get(&o).cloned().unwrap_or_default()
}

fn set_mut(emu: &mut Emu, o: u64) -> &mut Indexes {
    emu.indexsets.sets.entry(o).or_default()
}

/// The receiver (an init) holding `ix`.
fn make(emu: &mut Emu, ix: Indexes) -> Result<()> {
    let obj = emu.arg(0);
    emu.indexsets.sets.insert(obj, ix);
    ret(emu, obj)
}

/// A new instance of the receiving class (a class method) holding `ix`.
fn make_new(emu: &mut Emu, ix: Indexes) -> Result<()> {
    let recv = emu.arg(0);
    let obj = {
        let o = send(emu, recv, "alloc", &[])?;
        send(emu, o, "init", &[])?
    };
    emu.indexsets.sets.insert(obj, ix);
    ret(emu, obj)
}

fn range(start: u64, len: u64) -> Indexes {
    let mut ix = Indexes::default();
    ix.add(start, len);
    ix
}

fn ret_range(emu: &mut Emu, start: u64, len: u64) -> Result<()> {
    emu.cpu.x[0] = start;
    emu.cpu.x[1] = len;
    Ok(())
}

/// Call `block(index, &stop)` for each index (or `(range, &stop)` for each
/// range) until it sets stop.
fn enumerate(emu: &mut Emu, items: Vec<(u64, u64)>, block: u64, ranges: bool) -> Result<()> {
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for (a, b) in items {
        if ranges {
            crate::libsystem::call_block(emu, block, &[a, b, stop])?;
        } else {
            crate::libsystem::call_block(emu, block, &[a, stop])?;
        }
        if emu.p.memory.read(stop, 1)?[0] != 0 {
            break;
        }
    }
    Ok(())
}

fn each_index(emu: &mut Emu, o: u64, options: u64, block: u64, within: Option<(u64, u64)>) -> Result<()> {
    let ix = set(emu, o);
    let mut list: Vec<(u64, u64)> = ix.indexes().filter(|&i| within.is_none_or(|(s, l)| i >= s && i < s + l)).map(|i| (i, 0)).collect();
    if options & REVERSE != 0 {
        list.reverse();
    }
    enumerate(emu, list, block, false)
}

fn each_range(emu: &mut Emu, o: u64, options: u64, block: u64) -> Result<()> {
    let mut list: Vec<(u64, u64)> = set(emu, o).0.iter().map(|&(a, b)| (a, b - a)).collect();
    if options & REVERSE != 0 {
        list.reverse();
    }
    enumerate(emu, list, block, true)
}

/// The first index (in order) for which `block(index, &stop)` returns YES.
fn index_passing(emu: &mut Emu, o: u64, options: u64, block: u64) -> Result<u64> {
    let mut list: Vec<u64> = set(emu, o).indexes().collect();
    if options & REVERSE != 0 {
        list.reverse();
    }
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for i in list {
        if crate::libsystem::call_block(emu, block, &[i, stop])? & 0xff != 0 {
            return Ok(i);
        }
        if emu.p.memory.read(stop, 1)?[0] != 0 {
            break;
        }
    }
    Ok(NOT_FOUND)
}

/// The indexes for which `block(index, &stop)` returns YES, as a new set.
fn indexes_passing(emu: &mut Emu, o: u64, block: u64) -> Result<()> {
    let list: Vec<u64> = set(emu, o).indexes().collect();
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    let mut out = Indexes::default();
    for i in list {
        if crate::libsystem::call_block(emu, block, &[i, stop])? & 0xff != 0 {
            out.add(i, 1);
        }
        if emu.p.memory.read(stop, 1)?[0] != 0 {
            break;
        }
    }
    new_set(emu, "NSIndexSet", out)
}

fn new_set(emu: &mut Emu, class: &str, ix: Indexes) -> Result<()> {
    let cls = objc::host_class(emu, class)?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.indexsets.sets.insert(o, ix);
    ret(emu, o)
}

const S: &str = "NSIndexSet";
const M: &str = "NSMutableIndexSet";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (S, true, "indexSet", |e| make_new(e, Indexes::default())),
    (S, true, "indexSetWithIndex:", |e| {
        let i = e.arg(2);
        make_new(e, range(i, 1))
    }),
    (S, true, "indexSetWithIndexesInRange:", |e| {
        let (s, l) = (e.arg(2), e.arg(3));
        make_new(e, range(s, l))
    }),
    (S, false, "init", |e| {
        let o = e.arg(0);
        e.indexsets.sets.entry(o).or_default();
        ret(e, o)
    }),
    (S, false, "initWithIndex:", |e| {
        let i = e.arg(2);
        make(e, range(i, 1))
    }),
    (S, false, "initWithIndexesInRange:", |e| {
        let (s, l) = (e.arg(2), e.arg(3));
        make(e, range(s, l))
    }),
    (S, false, "initWithIndexSet:", |e| {
        let other = set(e, e.arg(2));
        make(e, other)
    }),
    (S, true, "indexSetWithIndexSet:", |e| {
        let other = set(e, e.arg(2));
        make_new(e, other)
    }),
    (S, false, "copyWithZone:", |e| {
        let ix = set(e, e.arg(0));
        new_set(e, S, ix)
    }),
    (S, false, "copy", |e| {
        let ix = set(e, e.arg(0));
        new_set(e, S, ix)
    }),
    (S, false, "mutableCopyWithZone:", |e| {
        let ix = set(e, e.arg(0));
        new_set(e, M, ix)
    }),
    (S, false, "mutableCopy", |e| {
        let ix = set(e, e.arg(0));
        new_set(e, M, ix)
    }),
    (S, false, "count", |e| {
        let n = set(e, e.arg(0)).count();
        ret(e, n)
    }),
    (S, false, "firstIndex", |e| {
        let i = set(e, e.arg(0)).first();
        ret(e, i)
    }),
    (S, false, "lastIndex", |e| {
        let i = set(e, e.arg(0)).last();
        ret(e, i)
    }),
    (S, false, "indexGreaterThanIndex:", |e| {
        let i = set(e, e.arg(0)).greater_than(e.arg(2));
        ret(e, i)
    }),
    (S, false, "indexGreaterThanOrEqualToIndex:", |e| {
        let (ix, i) = (set(e, e.arg(0)), e.arg(2));
        let r = if ix.contains(i) { i } else { ix.greater_than(i) };
        ret(e, r)
    }),
    (S, false, "indexLessThanIndex:", |e| {
        let i = set(e, e.arg(0)).less_than(e.arg(2));
        ret(e, i)
    }),
    (S, false, "indexLessThanOrEqualToIndex:", |e| {
        let (ix, i) = (set(e, e.arg(0)), e.arg(2));
        let r = if ix.contains(i) { i } else { ix.less_than(i) };
        ret(e, r)
    }),
    (S, false, "containsIndex:", |e| {
        let c = set(e, e.arg(0)).contains(e.arg(2));
        ret(e, c as u64)
    }),
    (S, false, "containsIndexesInRange:", |e| {
        let (s, l) = (e.arg(2), e.arg(3));
        let c = set(e, e.arg(0)).count_in(s, l) == l;
        ret(e, c as u64)
    }),
    (S, false, "intersectsIndexesInRange:", |e| {
        let (s, l) = (e.arg(2), e.arg(3));
        let c = set(e, e.arg(0)).count_in(s, l) > 0;
        ret(e, c as u64)
    }),
    (S, false, "containsIndexes:", |e| {
        let (a, b) = (set(e, e.arg(0)), set(e, e.arg(2)));
        let c = b.indexes().all(|i| a.contains(i));
        ret(e, c as u64)
    }),
    (S, false, "countOfIndexesInRange:", |e| {
        let (s, l) = (e.arg(2), e.arg(3));
        let n = set(e, e.arg(0)).count_in(s, l);
        ret(e, n)
    }),
    (S, false, "isEqualToIndexSet:", |e| {
        let same = set(e, e.arg(0)) == set(e, e.arg(2));
        ret(e, same as u64)
    }),
    (S, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let cls = objc::host_class(e, S)?;
        let same = a == b || (b != 0 && send(e, b, "isKindOfClass:", &[cls])? & 0xff != 0 && set(e, a) == set(e, b));
        ret(e, same as u64)
    }),
    (S, false, "hash", |e| {
        let n = set(e, e.arg(0)).count();
        ret(e, n)
    }),
    // (buffer, maxCount, NSRangePointer inRange): copies indexes out, advancing the range.
    (S, false, "getIndexes:maxCount:inIndexRange:", |e| {
        let (buf, max, rp) = (e.arg(2), e.arg(3), e.arg(4));
        let (start, len) = if rp == 0 { (0, u64::MAX) } else { (e.read_u64(rp)?, e.read_u64(rp + 8)?) };
        let end = start.saturating_add(len);
        let list: Vec<u64> = set(e, e.arg(0)).indexes().filter(|&i| i >= start && i < end).take(max as usize).collect();
        for (k, &i) in list.iter().enumerate() {
            e.write_u64(buf + 8 * k as u64, i)?;
        }
        if rp != 0 {
            let next = list.last().map_or(end, |&i| i + 1);
            e.write_u64(rp, next)?;
            e.write_u64(rp + 8, end - next)?;
        }
        ret(e, list.len() as u64)
    }),
    (S, false, "rangeAtIndex:", |e| {
        let (s, l) = set(e, e.arg(0)).0.get(e.arg(2) as usize).map_or((NOT_FOUND, 0), |&(a, b)| (a, b - a));
        ret_range(e, s, l)
    }),
    (S, false, "enumerateIndexesUsingBlock:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        each_index(e, o, 0, b, None)
    }),
    (S, false, "enumerateIndexesWithOptions:usingBlock:", |e| {
        let (o, opts, b) = (e.arg(0), e.arg(2), e.arg(3));
        each_index(e, o, opts, b, None)
    }),
    (S, false, "enumerateIndexesInRange:options:usingBlock:", |e| {
        let (o, s, l, opts, b) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
        each_index(e, o, opts, b, Some((s, l)))
    }),
    (S, false, "enumerateRangesUsingBlock:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        each_range(e, o, 0, b)
    }),
    (S, false, "enumerateRangesWithOptions:usingBlock:", |e| {
        let (o, opts, b) = (e.arg(0), e.arg(2), e.arg(3));
        each_range(e, o, opts, b)
    }),
    (S, false, "indexPassingTest:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        let i = index_passing(e, o, 0, b)?;
        ret(e, i)
    }),
    (S, false, "indexWithOptions:passingTest:", |e| {
        let (o, opts, b) = (e.arg(0), e.arg(2), e.arg(3));
        let i = index_passing(e, o, opts, b)?;
        ret(e, i)
    }),
    (S, false, "indexesPassingTest:", |e| {
        let (o, b) = (e.arg(0), e.arg(2));
        indexes_passing(e, o, b)
    }),
    (S, false, "description", |e| {
        let ix = set(e, e.arg(0));
        let parts: Vec<String> = ix.0.iter().map(|&(a, b)| if b - a == 1 { format!("{a}") } else { format!("{a}-{}", b - 1) }).collect();
        let s = crate::foundation::new_nsstring(e, &format!("<NSIndexSet>[number of indexes: {} (in {} ranges), indexes: ({})]", ix.count(), ix.0.len(), parts.join(" ")))?;
        ret(e, s)
    }),
    (M, false, "addIndex:", |e| {
        let (o, i) = (e.arg(0), e.arg(2));
        set_mut(e, o).add(i, 1);
        Ok(())
    }),
    (M, false, "addIndexesInRange:", |e| {
        let (o, s, l) = (e.arg(0), e.arg(2), e.arg(3));
        set_mut(e, o).add(s, l);
        Ok(())
    }),
    (M, false, "addIndexes:", |e| {
        let (o, other) = (e.arg(0), set(e, e.arg(2)));
        for (a, b) in other.0 {
            set_mut(e, o).add(a, b - a);
        }
        Ok(())
    }),
    (M, false, "removeIndex:", |e| {
        let (o, i) = (e.arg(0), e.arg(2));
        set_mut(e, o).remove(i, 1);
        Ok(())
    }),
    (M, false, "removeIndexesInRange:", |e| {
        let (o, s, l) = (e.arg(0), e.arg(2), e.arg(3));
        set_mut(e, o).remove(s, l);
        Ok(())
    }),
    (M, false, "removeIndexes:", |e| {
        let (o, other) = (e.arg(0), set(e, e.arg(2)));
        for (a, b) in other.0 {
            set_mut(e, o).remove(a, b - a);
        }
        Ok(())
    }),
    (M, false, "removeAllIndexes", |e| {
        let o = e.arg(0);
        *set_mut(e, o) = Indexes::default();
        Ok(())
    }),
    (M, false, "shiftIndexesStartingAtIndex:by:", |e| {
        let (o, at, by) = (e.arg(0), e.arg(2), e.arg(3) as i64);
        set_mut(e, o).shift(at, by);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_split() {
        let mut ix = Indexes::default();
        ix.add(200, 100); // 200–299
        ix.add(5, 1);
        ix.add(6, 2); // touches 5: 5–7
        assert_eq!(ix.0, vec![(5, 8), (200, 300)]);
        assert_eq!(ix.count(), 103);
        assert!(ix.contains(204) && !ix.contains(300) && !ix.contains(8));
        ix.remove(250, 10);
        assert_eq!(ix.0, vec![(5, 8), (200, 250), (260, 300)]);
        assert_eq!((ix.first(), ix.last()), (5, 299));
        assert_eq!((ix.greater_than(7), ix.greater_than(249), ix.less_than(260)), (200, 260, 249));
        assert_eq!(ix.greater_than(299), NOT_FOUND);
        assert_eq!(ix.count_in(0, 210), 13);
        ix.add(8, 192); // fills the gap
        assert_eq!(ix.0, vec![(5, 250), (260, 300)]);
    }

    #[test]
    fn shifting() {
        let mut ix = Indexes::default();
        ix.add(1, 1);
        ix.add(5, 2);
        ix.shift(3, 2);
        assert_eq!(ix.indexes().collect::<Vec<_>>(), vec![1, 7, 8]);
        ix.shift(2, -5); // drops 2…6, pulls 7, 8 down
        assert_eq!(ix.indexes().collect::<Vec<_>>(), vec![1, 2, 3]);
    }
}
