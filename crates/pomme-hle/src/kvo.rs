//! Key-value observing: `addObserver:forKeyPath:options:context:` and its
//! removal. An observer hears `observeValueForKeyPath:ofObject:change:context:`
//! at once if it asked for the initial value (NSKeyValueObservingOptionInitial),
//! when the object says a key changed (`willChangeValueForKey:` /
//! `didChangeValueForKey:`), and when a view's `frame`, `center`, `bounds`
//! or `hidden` changes (pomme's UIView setters say so). The change dictionary
//! has the kind (setting) and, if asked for, the new and old values.

use crate::objc::send;
use crate::{Emu, HostFn, Result, foundation};

/// NSKeyValueObservingOptions.
const NEW: u64 = 1;
const OLD: u64 = 2;
const INITIAL: u64 = 4;
const PRIOR: u64 = 8;
/// NSKeyValueChangeSetting.
const SETTING: i64 = 1;

/// One registration: `observer` watches `object`'s `key`.
#[derive(Clone, Debug)]
pub struct Observation {
    object: u64,
    key: String,
    observer: u64,
    options: u64,
    context: u64,
}

/// The value of `key` on `object` now: a view's geometry and visibility
/// directly (boxed as UIKit boxes them), anything else by key-value coding.
fn value(emu: &mut Emu, object: u64, key: &str) -> Result<u64> {
    if emu.ui.views.views.contains_key(&object) {
        let vw = crate::uiview::view(emu, object);
        match key {
            "center" => return crate::nsvalue::new_point_value(emu, vw.center),
            "bounds" => return crate::nsvalue::new_rect_value(emu, vw.bounds),
            "frame" => {
                let f = crate::uiview::frame_of(emu, object);
                return crate::nsvalue::new_rect_value(emu, f);
            }
            "hidden" => return Ok(foundation::boolean(emu, vw.hidden)),
            _ => {}
        }
    }
    let k = foundation::new_nsstring(emu, key)?;
    send(emu, object, "valueForKey:", &[k])
}

/// Tell one observer: its change dictionary (the kind, and the values it asked for).
fn notify(emu: &mut Emu, o: &Observation, old: Option<u64>, prior: bool) -> Result<()> {
    let kind_key = foundation::new_nsstring(emu, "kind")?;
    let kind = foundation::new_number_i64(emu, SETTING)?;
    let mut pairs = vec![(kind_key, kind)];
    if prior {
        let k = foundation::new_nsstring(emu, "notificationIsPrior")?;
        pairs.push((k, foundation::boolean(emu, true)));
    } else if o.options & NEW != 0 {
        let k = foundation::new_nsstring(emu, "new")?;
        let v = value(emu, o.object, &o.key)?;
        pairs.push((k, if v == 0 { foundation::null(emu) } else { v }));
    }
    if let Some(old) = old.filter(|_| o.options & OLD != 0) {
        let k = foundation::new_nsstring(emu, "old")?;
        pairs.push((k, if old == 0 { foundation::null(emu) } else { old }));
    }
    let change = foundation::new_dict_of(emu, &pairs)?;
    let key = foundation::new_nsstring(emu, &o.key)?;
    send(emu, o.observer, "observeValueForKeyPath:ofObject:change:context:", &[key, o.object, change, o.context])?;
    Ok(())
}

fn observers_of(emu: &Emu, object: u64, key: &str) -> Vec<Observation> {
    emu.fnd.kvo.iter().filter(|o| o.object == object && o.key == key).cloned().collect()
}

/// Before `object`'s `key` changes: the old values for observers that asked,
/// and the prior notification for those that asked for it.
pub(crate) fn will_change(emu: &mut Emu, object: u64, key: &str) -> Result<Vec<(Observation, Option<u64>)>> {
    if emu.fnd.kvo.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for o in observers_of(emu, object, key) {
        let old = if o.options & OLD != 0 { Some(value(emu, object, key)?) } else { None };
        if o.options & PRIOR != 0 {
            notify(emu, &o, old, true)?;
        }
        out.push((o, old));
    }
    Ok(out)
}

/// After it changed: each observer hears of it.
pub(crate) fn did_change(emu: &mut Emu, pending: Vec<(Observation, Option<u64>)>) -> Result<()> {
    for (o, old) in pending {
        // (Still registered? An observer may have stopped while hearing of another.)
        if emu.fnd.kvo.iter().any(|x| x.object == o.object && x.key == o.key && x.observer == o.observer && x.context == o.context) {
            notify(emu, &o, old, false)?;
        }
    }
    Ok(())
}

/// A view's geometry or visibility is about to change: what its observers need.
pub(crate) fn view_will_change(emu: &mut Emu, v: u64, keys: &[&str]) -> Result<Vec<(Observation, Option<u64>)>> {
    if emu.fnd.kvo.is_empty() {
        return Ok(Vec::new());
    }
    let mut all = Vec::new();
    for key in keys {
        all.extend(will_change(emu, v, key)?);
    }
    Ok(all)
}

fn add_observer(emu: &mut Emu) -> Result<()> {
    let (object, observer, path, options, context) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let key = foundation::nsstring(emu, path)?;
    let o = Observation { object, key, observer, options, context };
    emu.fnd.kvo.push(o.clone());
    if options & INITIAL != 0 {
        notify(emu, &o, None, false)?;
    }
    Ok(())
}

fn remove_observer(emu: &mut Emu, context: Option<u64>) -> Result<()> {
    let (object, observer, path) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let key = foundation::nsstring(emu, path)?;
    // The most recent matching registration goes, as Foundation removes them.
    if let Some(i) = emu.fnd.kvo.iter().rposition(|o| o.object == object && o.observer == observer && o.key == key && context.is_none_or(|c| o.context == c)) {
        emu.fnd.kvo.remove(i);
    }
    Ok(())
}

/// pomme moved or resized a view itself (laying it out): its observers hear
/// of it on the main run loop (pomme's layout can't call into the app where
/// it is), with the value then.
pub(crate) fn view_changed_later(emu: &mut Emu, v: u64, keys: &[&'static str]) -> Result<()> {
    if emu.fnd.kvo.is_empty() || !emu.fnd.kvo.iter().any(|o| o.object == v && keys.contains(&o.key.as_str())) {
        return Ok(());
    }
    for &key in keys {
        if !emu.fnd.kvo_later.contains(&(v, key)) {
            emu.fnd.kvo_later.push((v, key));
        }
    }
    crate::uikit::after_on_main(emu, 0.0, |e| {
        for (v, key) in std::mem::take(&mut e.fnd.kvo_later) {
            let pending = observers_of(e, v, key).into_iter().map(|o| (o, None)).collect();
            did_change(e, pending)?;
        }
        Ok(())
    }, 0)
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSObject", false, "addObserver:forKeyPath:options:context:", add_observer),
    ("NSObject", false, "removeObserver:forKeyPath:", |e| remove_observer(e, None)),
    ("NSObject", false, "removeObserver:forKeyPath:context:", |e| {
        let c = e.arg(4);
        remove_observer(e, Some(c))
    }),
    ("NSObject", false, "willChangeValueForKey:", |e| {
        let (o, k) = (e.arg(0), e.arg(2));
        if e.fnd.kvo.is_empty() {
            return Ok(());
        }
        let key = foundation::nsstring(e, k)?;
        let pending = will_change(e, o, &key)?;
        e.fnd.kvo_pending.push((o, key, pending));
        Ok(())
    }),
    ("NSObject", false, "didChangeValueForKey:", |e| {
        let (o, k) = (e.arg(0), e.arg(2));
        if e.fnd.kvo.is_empty() {
            return Ok(());
        }
        let key = foundation::nsstring(e, k)?;
        let pending = match e.fnd.kvo_pending.iter().rposition(|(po, pk, _)| *po == o && *pk == key) {
            Some(i) => e.fnd.kvo_pending.remove(i).2,
            // (No willChange: no old values.)
            None => observers_of(e, o, &key).into_iter().map(|ob| (ob, None)).collect(),
        };
        did_change(e, pending)
    }),
    // The default: an observer that doesn't override it ignores the news.
    ("NSObject", false, "observeValueForKeyPath:ofObject:change:context:", |_| Ok(())),
];
