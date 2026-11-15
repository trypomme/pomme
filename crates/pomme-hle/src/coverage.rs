//! How much of iOS an app needs, and how much of that pomme provides.
//!
//! Two views, combined in a [`Report`]:
//!
//! - **Bound** (exact, with no run): every system import the loader bound to
//!   one of pomme's stubs is either *implemented* (a host function handles
//!   it), *data* (HLE backed it with real memory: a class, a constant) or
//!   *missing* (neither: calling it, or reading it, stops the app).
//! - **Run** (what one run reached): the [`Recorder`] notes every missing
//!   function and Objective-C method the app actually called, and every
//!   message a *permissive* class answered generically (a stand-in answer, not
//!   iOS's: a place pomme may differ from iOS without stopping). With
//!   [`Recorder::keep_going`] a miss returns 0 and the app carries on, so a
//!   single run finds them all instead of stopping at the first; what
//!   happens after a miss may not be what iOS would do, so the first miss is
//!   marked.
//!
//! Functions bound to guest code from the sysroot (the Swift runtime,
//! libc++, CoreFoundation, …) aren't pomme's HLE and aren't counted here.

use crate::{Emu, Stop};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// What a run reached that pomme doesn't provide. Not part of the phone: a
/// snapshot leaves it out and restoring one keeps the recorder in use.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    /// On a miss, return 0 (nil, false) and carry on instead of stopping.
    pub keep_going: bool,
    /// Missing system functions called: (library, symbol).
    pub functions: BTreeMap<(String, String), Miss>,
    /// Messages nothing implements.
    pub methods: BTreeMap<Method, Miss>,
    /// Messages a permissive class answered generically.
    pub permissive: BTreeMap<Method, Miss>,
    /// Keys of permissive objects' stored properties that pomme's own code
    /// read (a label's `font`): a setter that stores one is implemented, in effect.
    read_keys: RefCell<BTreeSet<String>>,
    /// Misses recorded so far (to mark the first).
    order: u64,
}

/// One gap, as a run met it.
#[derive(Clone, Debug, Serialize)]
pub struct Miss {
    pub count: u64,
    /// Where it was first called from (symbolized).
    pub first_caller: String,
    /// The instruction count at that first call.
    pub first_step: u64,
    /// The run's first miss: everything after it ran on a stand-in answer.
    pub first: bool,
}

/// A message: `-[UIView foo:]`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Method {
    /// The receiver's class.
    pub class: String,
    /// The system class it is or inherits from (where pomme would implement it), if any.
    pub system_class: Option<String>,
    pub selector: String,
    pub class_method: bool,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{} {}]", if self.class_method { '+' } else { '-' }, self.class, self.selector)?;
        match &self.system_class {
            Some(s) if *s != self.class => write!(f, " (a {s})"),
            _ => Ok(()),
        }
    }
}

impl Recorder {
    /// A recorder; `keep_going` answers misses with 0 instead of stopping.
    pub fn new(keep_going: bool) -> Self {
        Recorder { keep_going, ..Default::default() }
    }

    fn note(&mut self, emu_steps: u64, caller: impl FnOnce() -> String) -> Miss {
        self.order += 1;
        Miss { count: 0, first_caller: caller(), first_step: emu_steps, first: self.order == 1 }
    }
}

/// The receiver's nearest system (pomme-synthesized) class, by name.
fn system_class(emu: &Emu, mut cls: u64) -> Option<String> {
    while let Some(c) = emu.objc.classes.get(&cls) {
        if c.host {
            return Some(c.name.clone());
        }
        cls = c.superclass;
    }
    None
}

fn method(emu: &Emu, class: &str, cls: u64, selector: &str, class_method: bool) -> Method {
    Method { class: class.to_string(), system_class: system_class(emu, cls), selector: selector.to_string(), class_method }
}

/// The guest called a system function pomme doesn't implement. With a
/// recorder that keeps going, the call returns 0; otherwise the run stops.
pub(crate) fn missing_function(emu: &mut Emu, library: &str, symbol: &str, caller: u64) -> crate::Result<()> {
    let steps = emu.cpu.steps;
    let caller_name = || emu.symbolize(caller.wrapping_sub(4));
    let Some(rec) = emu.coverage.as_ref() else {
        return Err(Stop::MissingFunction { symbol: symbol.into(), library: library.into(), caller });
    };
    let key = (library.to_string(), symbol.to_string());
    let keep_going = rec.keep_going;
    if !rec.functions.contains_key(&key) {
        let name = caller_name();
        let rec = emu.coverage.as_mut().unwrap();
        let miss = rec.note(steps, || name);
        rec.functions.insert(key.clone(), miss);
    }
    emu.coverage.as_mut().unwrap().functions.get_mut(&key).unwrap().count += 1;
    if !keep_going {
        return Err(Stop::MissingFunction { symbol: symbol.into(), library: library.into(), caller });
    }
    emu.ret(0);
    emu.cpu.q[0] = 0;
    emu.cpu.pc = caller;
    Ok(())
}

/// A message nothing implements: recorded, then `Ok(())` if the recorder
/// keeps going (the caller answers 0 / nil) or the stop to return.
pub(crate) fn missing_method(emu: &mut Emu, stop: Stop, cls: u64) -> crate::Result<()> {
    let Stop::MissingMethod { class, selector, class_method, caller } = &stop else { return Err(stop) };
    let Some(keep_going) = emu.coverage.as_ref().map(|r| r.keep_going) else { return Err(stop) };
    let key = method(emu, class, cls, selector, *class_method);
    let steps = emu.cpu.steps;
    if !emu.coverage.as_ref().unwrap().methods.contains_key(&key) {
        let name = emu.symbolize(caller.wrapping_sub(4));
        let rec = emu.coverage.as_mut().unwrap();
        let miss = rec.note(steps, || name);
        rec.methods.insert(key.clone(), miss);
    }
    emu.coverage.as_mut().unwrap().methods.get_mut(&key).unwrap().count += 1;
    if keep_going { Ok(()) } else { Err(stop) }
}

/// A permissive class answered a message generically.
pub(crate) fn permissive(emu: &mut Emu, cls: u64, selector: &str, class_method: bool, caller: u64) {
    if emu.coverage.is_none() {
        return;
    }
    let class = emu.objc.class_name(cls);
    let key = method(emu, &class, cls, selector, class_method);
    let steps = emu.cpu.steps;
    let rec = emu.coverage.as_mut().unwrap();
    if let Some(m) = rec.permissive.get_mut(&key) {
        m.count += 1;
        return;
    }
    // Permissive answers are expected stand-ins, not misses: they don't count as the first miss.
    let name = emu.symbolize(caller.wrapping_sub(4));
    let rec = emu.coverage.as_mut().unwrap();
    rec.permissive.insert(key, Miss { count: 1, first_caller: name, first_step: steps, first: false });
}

/// pomme's own code read a stored property (see [`crate::objc::bag_read`]).
pub(crate) fn read_key(emu: &Emu, key: &str) {
    if let Some(rec) = emu.coverage.as_ref()
        && !rec.read_keys.borrow().contains(key)
    {
        rec.read_keys.borrow_mut().insert(key.to_string());
    }
}

/// The property a setter stores on a permissive object: `setFont:` → `font`.
fn stored_key(m: &Method) -> Option<String> {
    let name = m.selector.strip_prefix("set")?.strip_suffix(':').filter(|n| !n.contains(':') && !n.is_empty())?;
    (!m.class_method).then(|| name[..1].to_ascii_lowercase() + &name[1..])
}

/// What a permissive class's generic answer amounted to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// A setter whose value pomme's own code reads (drawing, layout …): implemented, in effect.
    StoredAndUsed,
    /// A setter whose value nothing in pomme reads: the app's setting has no effect.
    StoredUnused,
    /// Anything else: a getter reading back what was stored, `init…` returning self,
    /// a factory returning a fresh object, or 0.
    Generic,
}

/// How pomme provides a bound system symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// A host function handles calls to it.
    Implemented,
    /// HLE backed it with real memory (a class, a constant).
    Data,
    /// Neither: calling or reading it stops the app.
    Missing,
}

/// A system symbol the app binds to pomme.
#[derive(Clone, Debug, Serialize)]
pub struct Symbol {
    pub library: String,
    pub symbol: String,
    pub status: Status,
    /// Calls during the run (0 without one, or if never called).
    pub calls: u64,
}

/// One library's share.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Library {
    pub library: String,
    pub implemented: usize,
    pub data: usize,
    pub missing: usize,
    /// Implemented functions the run called.
    pub called: usize,
}

/// A miss with what it was.
#[derive(Clone, Debug, Serialize)]
pub struct FunctionMiss {
    pub library: String,
    pub symbol: String,
    #[serde(flatten)]
    pub miss: Miss,
}

#[derive(Clone, Debug, Serialize)]
pub struct MethodMiss {
    #[serde(flatten)]
    pub method: Method,
    #[serde(flatten)]
    pub miss: Miss,
}

/// A message a permissive class answered.
#[derive(Clone, Debug, Serialize)]
pub struct PermissiveAnswer {
    #[serde(flatten)]
    pub method: Method,
    #[serde(flatten)]
    pub miss: Miss,
    pub answer: Answer,
}

/// What the run reached.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    /// Why it ended (`exit(0)`, the time limit, a miss when not keeping going …).
    pub ended: String,
    pub instructions: u64,
    pub kept_going: bool,
    pub missing_functions: Vec<FunctionMiss>,
    pub missing_methods: Vec<MethodMiss>,
    pub permissive: Vec<PermissiveAnswer>,
}

/// An app's coverage.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub app: String,
    pub libraries: Vec<Library>,
    pub symbols: Vec<Symbol>,
    /// Imports of the app's own frameworks that none of them exports.
    pub unresolved: Vec<Symbol>,
    pub run: Option<Run>,
}

/// A library's short name: `/System/Library/Frameworks/UIKit.framework/UIKit` → `UIKit`.
pub fn short_name(library: &str) -> &str {
    let last = library.rsplit('/').next().unwrap_or(library);
    last.strip_suffix(".dylib").unwrap_or(last)
}

/// The bound view (and, after [`Emu::run_main`] with a recorder, the run's).
/// `ended` is how the run ended, if there was one.
pub fn report(emu: &Emu, ended: Option<String>) -> Report {
    let unresolved: HashSet<(&str, &str)> = emu.p.unresolved.iter().map(|u| (u.library.as_str(), u.symbol.as_str())).collect();
    let mut symbols = Vec::new();
    let mut bundled = Vec::new();
    for (i, (_, stub)) in emu.p.stubs.iter().enumerate() {
        let status = if emu.handlers.get(i).is_some_and(Option::is_some) {
            Status::Implemented
        } else if emu.p.stubs.is_backed(i) {
            Status::Data
        } else {
            Status::Missing
        };
        let calls = match status {
            Status::Implemented => emu.calls.get(&stub.symbol).copied().unwrap_or(0),
            _ => emu.coverage.as_ref().and_then(|r| r.functions.get(&(stub.library.clone(), stub.symbol.clone()))).map_or(0, |m| m.count),
        };
        let row = Symbol { library: stub.library.clone(), symbol: stub.symbol.clone(), status, calls };
        if unresolved.contains(&(stub.library.as_str(), stub.symbol.as_str())) {
            bundled.push(row);
        } else {
            symbols.push(row);
        }
    }
    symbols.sort_by(|a, b| (short_name(&a.library), &a.symbol).cmp(&(short_name(&b.library), &b.symbol)));

    let mut libs: BTreeMap<&str, Library> = BTreeMap::new();
    for s in &symbols {
        let l = libs.entry(short_name(&s.library)).or_insert_with(|| Library { library: short_name(&s.library).to_string(), ..Default::default() });
        match s.status {
            Status::Implemented => l.implemented += 1,
            Status::Data => l.data += 1,
            Status::Missing => l.missing += 1,
        }
        if s.status == Status::Implemented && s.calls > 0 {
            l.called += 1;
        }
    }
    let mut libraries: Vec<Library> = libs.into_values().collect();
    libraries.sort_by_key(|l| (std::cmp::Reverse(l.implemented + l.data + l.missing), l.library.clone()));

    let run = ended.map(|ended| {
        let rec = emu.coverage.as_ref().cloned().unwrap_or_default();
        let by_count = |a: &Miss, b: &Miss| b.count.cmp(&a.count).then(a.first_step.cmp(&b.first_step));
        let mut missing_functions: Vec<FunctionMiss> = rec.functions.into_iter().map(|((library, symbol), miss)| FunctionMiss { library, symbol, miss }).collect();
        missing_functions.sort_by(|a, b| by_count(&a.miss, &b.miss));
        let methods = |m: BTreeMap<Method, Miss>| {
            let mut v: Vec<MethodMiss> = m.into_iter().map(|(method, miss)| MethodMiss { method, miss }).collect();
            v.sort_by(|a, b| by_count(&a.miss, &b.miss));
            v
        };
        let read = rec.read_keys.borrow().clone();
        let permissive = methods(rec.permissive)
            .into_iter()
            .map(|MethodMiss { method, miss }| {
                let answer = match stored_key(&method) {
                    Some(k) if read.contains(&k) => Answer::StoredAndUsed,
                    Some(_) => Answer::StoredUnused,
                    None => Answer::Generic,
                };
                PermissiveAnswer { method, miss, answer }
            })
            .collect();
        Run { ended, instructions: emu.cpu.steps, kept_going: rec.keep_going, missing_functions, missing_methods: methods(rec.methods), permissive }
    });

    Report { app: emu.p.bundle_id.clone().unwrap_or_else(|| emu.p.main().name.clone()), libraries, symbols, unresolved: bundled, run }
}

impl Report {
    /// The report as text, for a terminal, listing up to `limit` missing symbols per library.
    pub fn text(&self, limit: usize) -> String {
        let mut s = format!("coverage of {}\n", self.app);
        let total = |f: fn(&Library) -> usize| self.libraries.iter().map(f).sum::<usize>();
        let (imp, data, miss) = (total(|l| l.implemented), total(|l| l.data), total(|l| l.missing));
        let all = imp + data + miss;
        let pct = |n: usize, of: usize| if of == 0 { 100.0 } else { n as f64 * 100.0 / of as f64 };
        s += &format!(
            "\n{all} system symbols bound to pomme: {imp} implemented, {data} data, {miss} missing ({:.1}% provided)\n",
            pct(imp + data, all)
        );
        s += &format!("\n  {:<28} {:>7} {:>6} {:>8} {:>9}  {:>7}\n", "library", "symbols", "impl", "data", "missing", "called");
        for l in &self.libraries {
            let n = l.implemented + l.data + l.missing;
            let called = if self.run.is_some() { l.called.to_string() } else { "-".into() };
            s += &format!("  {:<28} {:>7} {:>6} {:>8} {:>9}  {:>7}\n", l.library, n, l.implemented, l.data, l.missing, called);
        }
        let missing: Vec<&Symbol> = self.symbols.iter().filter(|s| s.status == Status::Missing).collect();
        if !missing.is_empty() {
            s += &format!("\nbound but missing ({}; a call or read stops the app), by library:\n", missing.len());
            let mut by_lib: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
            for m in &missing {
                by_lib.entry(short_name(&m.library)).or_default().push(m);
            }
            let mut libs: Vec<_> = by_lib.into_iter().collect();
            // Called ones first: they're what the run actually needed.
            libs.sort_by_key(|(l, v)| (std::cmp::Reverse(v.iter().filter(|m| m.calls > 0).count()), std::cmp::Reverse(v.len()), *l));
            for (lib, mut syms) in libs {
                syms.sort_by_key(|m| (std::cmp::Reverse(m.calls), m.symbol.as_str()));
                s += &format!("  {lib} ({})\n", syms.len());
                for m in syms.iter().take(limit) {
                    let called = if m.calls > 0 { format!("  — called {}×", m.calls) } else { String::new() };
                    s += &format!("      {}{called}\n", m.symbol);
                }
                if syms.len() > limit {
                    s += &format!("      … {} more (--all)\n", syms.len() - limit);
                }
            }
        }
        if !self.unresolved.is_empty() {
            s += &format!("\nimports of the app's own frameworks that none of them exports ({}):\n", self.unresolved.len());
            for u in &self.unresolved {
                s += &format!("  {:<12} {}\n", short_name(&u.library), u.symbol);
            }
        }
        match &self.run {
            None => s += "\n(no run: only what's bound; run it to see what the app reaches)\n",
            Some(r) => {
                s += &format!("\nthe run: {} after {} instructions{}\n", r.ended, r.instructions, if r.kept_going { ", keeping going past misses" } else { "" });
                let first = |m: &Miss| if m.first { "  ← first miss" } else { "" };
                s += &format!("\n  called but missing — functions ({}):\n", r.missing_functions.len());
                for f in &r.missing_functions {
                    s += &format!("  {:>6}×  {} ({}), first from {}{}\n", f.miss.count, f.symbol, short_name(&f.library), f.miss.first_caller, first(&f.miss));
                }
                s += &format!("\n  called but missing — methods ({}):\n", r.missing_methods.len());
                for m in &r.missing_methods {
                    s += &format!("  {:>6}×  {}, first from {}{}\n", m.miss.count, m.method, m.miss.first_caller, first(&m.miss));
                }
                // By the system class pomme would implement it on, with the app's classes that met it.
                let sections = [
                    (Answer::StoredUnused, "set, but nothing in pomme reads it: no effect, unlike on iOS"),
                    (Answer::Generic, "answered generically (stored value, self, a new object or 0)"),
                    (Answer::StoredAndUsed, "stored generically, and pomme reads it"),
                ];
                for (kind, title) in sections {
                    let mut grouped: BTreeMap<(String, bool, String), (u64, Vec<String>)> = BTreeMap::new();
                    for m in r.permissive.iter().filter(|m| m.answer == kind) {
                        let on = m.method.system_class.clone().unwrap_or_else(|| m.method.class.clone());
                        let g = grouped.entry((on.clone(), m.method.class_method, m.method.selector.clone())).or_default();
                        g.0 += m.miss.count;
                        if m.method.class != on {
                            g.1.push(m.method.class.clone());
                        }
                    }
                    let mut grouped: Vec<_> = grouped.into_iter().collect();
                    grouped.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
                    s += &format!("\n  permissive classes — {title} ({}):\n", grouped.len());
                    for ((on, meta, sel), (count, via)) in grouped.iter().take(if kind == Answer::StoredAndUsed { limit } else { usize::MAX }) {
                        let via = match via.len() {
                            0 => String::new(),
                            1..=3 => format!("  (on {})", via.join(", ")),
                            n => format!("  (on {}, … {} classes)", via[..2].join(", "), n),
                        };
                        s += &format!("  {count:>6}×  {}[{on} {sel}]{via}\n", if *meta { '+' } else { '-' });
                    }
                    if kind == Answer::StoredAndUsed && grouped.len() > limit {
                        s += &format!("      … {} more (--all)\n", grouped.len() - limit);
                    }
                }
            }
        }
        s
    }
}
