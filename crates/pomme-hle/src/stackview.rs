//! UIStackView: its arranged subviews in a row (or column), sized and spaced
//! by its distribution and placed across it by its alignment, from the
//! leading edge (the right one, in a right-to-left layout: fixtures/rtl).
//!
//! The stack lays its arranged subviews out after Auto Layout has placed the
//! stack itself. Its content size (the subviews' sizes and the spacing) is
//! its intrinsic content size, so constraints that leave its size open get
//! it. A subview's size along the stack is what a required constraint gives
//! it, else its intrinsic content size; a subview with neither shares the
//! room that's left (`fill`).

use crate::foundation;
use crate::objc;
use crate::uikit::ret_rect;
use crate::uiview::{set_frame_of, view};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct State {
    /// Stack → its arranged subviews, in order.
    arranged: HashMap<u64, Vec<u64>>,
}

fn get(emu: &Emu, v: u64, key: &str) -> u64 {
    objc::bag_get(emu, v, key).unwrap_or(0)
}

fn spacing(emu: &Emu, v: u64) -> f64 {
    objc::bag_get_f64(emu, v, "spacing").unwrap_or(0.0)
}

/// The arranged subviews that take part: still in the stack, and not hidden.
fn items(emu: &Emu, stack: u64) -> Vec<u64> {
    let list = emu.stacks.arranged.get(&stack).cloned().unwrap_or_default();
    list.into_iter().filter(|&v| emu.ui.views.views.get(&v).is_some_and(|x| x.superview == stack && !x.hidden)).collect()
}

/// A subview's own size along `axis` (a required constraint's, else its intrinsic one).
fn own_size(emu: &mut Emu, v: u64, axis: usize) -> Result<Option<f64>> {
    if let Some(s) = crate::autolayout::fixed_size(emu, v, axis) {
        return Ok(Some(s));
    }
    let r = objc::send_fp(emu, v, "intrinsicContentSize", &[], &[])?;
    Ok((r[axis] >= 0.0).then_some(r[axis]))
}

/// The stack's content size: (along, across); None where a subview's is unknown.
fn content_size(emu: &mut Emu, stack: u64) -> Result<(Option<f64>, Option<f64>)> {
    let axis = get(emu, stack, "axis") as usize;
    let list = items(emu, stack);
    let mut along = Some(spacing(emu, stack) * list.len().saturating_sub(1) as f64);
    let mut across: Option<f64> = Some(0.0);
    for &v in &list {
        along = match (along, own_size(emu, v, axis)?) {
            (Some(a), Some(s)) => Some(a + s),
            _ => None,
        };
        across = match (across, own_size(emu, v, 1 - axis)?) {
            (Some(a), Some(s)) => Some(a.max(s)),
            _ => None,
        };
    }
    Ok((along, across))
}

/// Where a stack's subviews go along it (start, size), in `length`.
fn place(distribution: u64, length: f64, gap: f64, sizes: &[Option<f64>]) -> Vec<(f64, f64)> {
    let n = sizes.len();
    if n == 0 {
        return Vec::new();
    }
    let gaps = gap * (n - 1) as f64;
    let known: f64 = sizes.iter().flatten().sum();
    let mut out = Vec::with_capacity(n);
    match distribution {
        // Fill equally: the same size each.
        1 => {
            let each = (length - gaps) / n as f64;
            for i in 0..n {
                out.push((i as f64 * (each + gap), each));
            }
        }
        // Equal spacing, and equal centering: what's left between them.
        3 | 4 if n > 1 => {
            let sizes: Vec<f64> = sizes.iter().map(|s| s.unwrap_or(0.0)).collect();
            let total: f64 = sizes.iter().sum();
            if distribution == 3 {
                let between = ((length - total) / (n - 1) as f64).max(gap);
                let mut x = 0.0;
                for s in sizes {
                    out.push((x, s));
                    x += s + between;
                }
            } else {
                let (first, last) = (sizes[0] / 2.0, length - sizes[n - 1] / 2.0);
                for (i, s) in sizes.iter().enumerate() {
                    let c = first + (last - first) * i as f64 / (n - 1) as f64;
                    out.push((c - s / 2.0, *s));
                }
            }
        }
        // Fill (and fill proportionally): subviews without a size share
        // what's left; with none, the last one takes it.
        _ => {
            let open = sizes.iter().filter(|s| s.is_none()).count();
            let spare = length - gaps - known;
            let mut x = 0.0;
            for (i, s) in sizes.iter().enumerate() {
                let size = match s {
                    Some(s) if open == 0 && i == n - 1 => s + spare,
                    Some(s) => *s,
                    None => (spare / open as f64).max(0.0),
                };
                out.push((x, size));
                x += size + gap;
            }
        }
    }
    out
}

/// Whether `v` is one of a stack's arranged subviews.
pub(crate) fn is_arranged(emu: &Emu, v: u64) -> bool {
    emu.stacks.arranged.values().any(|l| l.contains(&v))
}

/// Lay out every stack's arranged subviews (outer stacks first); whether any moved.
pub fn arrange(emu: &mut Emu) -> Result<bool> {
    let mut stacks: Vec<(usize, u64)> = Vec::new();
    let keys: Vec<u64> = emu.stacks.arranged.keys().copied().collect();
    for s in keys {
        if !emu.ui.views.views.contains_key(&s) {
            continue;
        }
        let (mut d, mut v) = (0, s);
        while v != 0 {
            v = view(emu, v).superview;
            d += 1;
        }
        stacks.push((d, s));
    }
    stacks.sort();
    let mut moved = false;
    for (_, stack) in stacks {
        let axis = get(emu, stack, "axis") as usize;
        let (distribution, alignment) = (get(emu, stack, "distribution"), get(emu, stack, "alignment"));
        let b = view(emu, stack).bounds;
        let (length, cross) = if axis == 0 { (b[2], b[3]) } else { (b[3], b[2]) };
        let list = items(emu, stack);
        let mut sizes = Vec::new();
        for &v in &list {
            sizes.push(if distribution == 1 { None } else { own_size(emu, v, axis)? });
        }
        let along = place(distribution, length, spacing(emu, stack), &sizes);
        let rtl = crate::direction::view_rtl(emu, stack)?;
        for (&v, (start, size)) in list.iter().zip(along) {
            let own = own_size(emu, v, 1 - axis)?.unwrap_or(cross);
            // UIStackViewAlignment: fill 0, leading/top 1, first baseline 2,
            // center 3, trailing/bottom 4, last baseline 5.
            let (c0, cs) = match alignment {
                0 => (0.0, cross),
                3 => ((cross - own) / 2.0, own),
                4 | 5 => (cross - own, own),
                _ => (0.0, own),
            };
            let r = if axis == 0 {
                let x = if rtl { length - start - size } else { start };
                [b[0] + x, b[1] + c0, size, cs]
            } else {
                // Across a column, leading is the right side in right to left.
                let x = if rtl && matches!(alignment, 1 | 2 | 4 | 5) { cross - c0 - cs } else { c0 };
                [b[0] + x, b[1] + start, cs, size]
            };
            let cur = crate::uiview::frame_of(emu, v);
            if cur.iter().zip(r).any(|(a, b)| (a - b).abs() > 1e-6) {
                set_frame_of(emu, v, r);
                moved = true;
            }
        }
    }
    Ok(moved)
}

fn add_arranged(emu: &mut Emu, stack: u64, v: u64, at: Option<usize>) -> Result<()> {
    if v == 0 {
        return Ok(());
    }
    let list = emu.stacks.arranged.entry(stack).or_default();
    list.retain(|&x| x != v);
    let i = at.unwrap_or(list.len()).min(list.len());
    list.insert(i, v);
    objc::send(emu, stack, "addSubview:", &[v])?;
    crate::autolayout::set_auto(emu, v);
    Ok(())
}

fn set_prop(emu: &mut Emu, key: &str) -> Result<()> {
    let (v, x) = (emu.arg(0), emu.arg(2));
    objc::bag_set(emu, v, key, x);
    crate::autolayout::mark_dirty(emu);
    Ok(())
}

const S: &str = "UIStackView";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (S, false, "initWithArrangedSubviews:", |e| {
        let (s, list) = (e.arg(0), e.arg(2));
        let s = objc::send(e, s, "init", &[])?;
        e.stacks.arranged.entry(s).or_default();
        if list != 0 {
            for v in foundation::array_values(e, list)? {
                add_arranged(e, s, v, None)?;
            }
        }
        e.ret(s);
        Ok(())
    }),
    (S, false, "addArrangedSubview:", |e| {
        let (s, v) = (e.arg(0), e.arg(2));
        add_arranged(e, s, v, None)
    }),
    (S, false, "insertArrangedSubview:atIndex:", |e| {
        let (s, v, i) = (e.arg(0), e.arg(2), e.arg(3) as usize);
        add_arranged(e, s, v, Some(i))
    }),
    (S, false, "removeArrangedSubview:", |e| {
        let (s, v) = (e.arg(0), e.arg(2));
        if let Some(l) = e.stacks.arranged.get_mut(&s) {
            l.retain(|&x| x != v);
        }
        crate::autolayout::mark_dirty(e);
        Ok(())
    }),
    (S, false, "arrangedSubviews", |e| {
        let l = e.stacks.arranged.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = foundation::new_array_of(e, &l)?;
        e.ret(a);
        Ok(())
    }),
    (S, false, "setAxis:", |e| set_prop(e, "axis")),
    (S, false, "setDistribution:", |e| set_prop(e, "distribution")),
    (S, false, "setAlignment:", |e| set_prop(e, "alignment")),
    (S, false, "axis", |e| {
        let v = get(e, e.arg(0), "axis");
        e.ret(v);
        Ok(())
    }),
    (S, false, "distribution", |e| {
        let v = get(e, e.arg(0), "distribution");
        e.ret(v);
        Ok(())
    }),
    (S, false, "alignment", |e| {
        let v = get(e, e.arg(0), "alignment");
        e.ret(v);
        Ok(())
    }),
    (S, false, "setSpacing:", |e| {
        let (v, q) = (e.arg(0), e.cpu.q[0]);
        e.objc.bag.insert((v, "spacing".to_string()), (0, q));
        crate::autolayout::mark_dirty(e);
        Ok(())
    }),
    (S, false, "spacing", |e| {
        let s = spacing(e, e.arg(0));
        e.cpu.q[0] = s.to_bits() as u128;
        Ok(())
    }),
    // The content size, where the subviews' sizes are known (−1, none, where not).
    (S, false, "intrinsicContentSize", |e| {
        let s = e.arg(0);
        let axis = get(e, s, "axis") as usize;
        let (along, across) = content_size(e, s)?;
        let (along, across) = (along.unwrap_or(-1.0), across.unwrap_or(-1.0));
        let r = if axis == 0 { [along, across] } else { [across, along] };
        ret_rect(e, [r[0], r[1], 0.0, 0.0]);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributions() {
        // fixtures/rtl: 300 wide, 8 apart.
        let fill = place(0, 300.0, 8.0, &[Some(40.0), None, Some(30.0)]);
        assert_eq!(fill, vec![(0.0, 40.0), (48.0, 214.0), (270.0, 30.0)]);
        let eq = place(1, 300.0, 8.0, &[None, None, None]);
        assert!((eq[1].0 - 102.6667).abs() < 1e-3 && (eq[1].1 - 94.6667).abs() < 1e-3);
        let spaced = place(3, 300.0, 8.0, &[Some(40.0), Some(60.0), Some(30.0)]);
        assert_eq!(spaced, vec![(0.0, 40.0), (125.0, 60.0), (270.0, 30.0)]);
    }
}
