//! Auto Layout: NSLayoutConstraint, layout anchors and layout guides (a
//! view's safeAreaLayoutGuide, layoutMarginsGuide and keyboardLayoutGuide,
//! and guides the app adds), solved at each layout pass; and the safe-area
//! callbacks (`safeAreaInsetsDidChange`, `viewSafeAreaInsetsDidChange`).
//!
//! Not Cassowary: values are propagated through the constraints. Required
//! equalities first, then optional ones, then intrinsic content sizes, then
//! inequalities fill what's still open as if they were equalities; anything
//! left keeps its current frame. That covers pinning, centering, fixed and
//! proportional sizes. Conflicting systems, and inequalities that would have
//! to push a value that's already set, settle differently from UIKit.

use crate::objc::{self, host_class, send, send_fp};
use crate::uikit::{ret_f64, ret_rect};
use crate::uiview::{frame_of, set_frame_of, view, window_origin};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attr {
    Left,
    Right,
    Top,
    Bottom,
    Width,
    Height,
    CenterX,
    CenterY,
}
use Attr::*;

impl Attr {
    fn axis(self) -> usize {
        match self {
            Left | Right | Width | CenterX => 0,
            _ => 1,
        }
    }
    /// Index into an axis's [min, max, center, size].
    fn slot(self) -> usize {
        match self {
            Left | Top => 0,
            Right | Bottom => 1,
            CenterX | CenterY => 2,
            Width | Height => 3,
        }
    }
    fn anchor_class(self) -> &'static str {
        match self {
            Width | Height => "NSLayoutDimension",
            Left | Right | CenterX => "NSLayoutXAxisAnchor",
            _ => "NSLayoutYAxisAnchor",
        }
    }
}

/// NSLayoutAttribute → the attribute, and whether it's a margin attribute
/// (which lays out against the view's layout margins guide). Baselines are
/// taken as the top and bottom edges.
fn from_ns(a: u64) -> Option<(Attr, bool)> {
    Some(match a {
        1 | 5 => (Left, false),
        2 | 6 => (Right, false),
        3 | 12 => (Top, false),
        4 | 11 => (Bottom, false),
        7 => (Width, false),
        8 => (Height, false),
        9 => (CenterX, false),
        10 => (CenterY, false),
        13 | 17 => (Left, true),
        14 | 18 => (Right, true),
        15 => (Top, true),
        16 => (Bottom, true),
        19 => (CenterX, true),
        20 => (CenterY, true),
        _ => return None,
    })
}

fn to_ns(a: Attr) -> u64 {
    match a {
        Left => 5,
        Right => 6,
        Top => 3,
        Bottom => 4,
        Width => 7,
        Height => 8,
        CenterX => 9,
        CenterY => 10,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GuideKind {
    /// `safeAreaLayoutGuide`: the owner, less what the safe area doesn't cover.
    Safe,
    /// `layoutMarginsGuide` (and `readableContentGuide`): the safe area less the margins.
    Margins,
    /// `keyboardLayoutGuide`: from the keyboard's top (or, with it down, the
    /// safe area's bottom) to the screen's bottom.
    Keyboard,
    /// A `UILayoutGuide` the app made: laid out by its constraints.
    Custom,
}

#[derive(Clone, Copy, Debug)]
struct Guide {
    owner: u64,
    kind: GuideKind,
}

#[derive(Clone, Copy, Debug)]
struct Constraint {
    first: u64,
    a1: Attr,
    /// NSLayoutRelation: −1 ≤, 0 =, 1 ≥.
    relation: i64,
    second: u64,
    a2: Option<Attr>,
    multiplier: f64,
    constant: f64,
    priority: f32,
}

const REQUIRED: f32 = 1000.0;
/// UIKit's standard spacing (`constraintEqualToSystemSpacingAfterAnchor:multiplier:`).
const SYSTEM_SPACING: f64 = 8.0;

#[derive(Debug, Default)]
pub struct State {
    constraints: HashMap<u64, Constraint>,
    /// Active constraints, in activation order.
    active: Vec<u64>,
    guides: HashMap<u64, Guide>,
    view_guides: HashMap<(u64, GuideKind), u64>,
    /// Guides added to a view (`addLayoutGuide:`).
    added_guides: HashMap<u64, Vec<u64>>,
    /// Where custom guides were laid out, in their owner's coordinates.
    guide_frames: HashMap<u64, [f64; 4]>,
    anchors: HashMap<(u64, Attr), u64>,
    anchor_of: HashMap<u64, (u64, Attr)>,
    /// Views laid out by constraints (`translatesAutoresizingMaskIntoConstraints` = NO).
    auto: HashSet<u64>,
    /// Layout margins set by the app (top, left, bottom, right).
    margins: HashMap<u64, [f64; 4]>,
    /// Constraints added through a view (`addConstraint:`), for `-constraints`.
    added: HashMap<u64, Vec<u64>>,
    /// The safe-area insets each interested view was last told about.
    told: HashMap<u64, [f64; 4]>,
    /// Classes → whether they (or their superclasses) implement the callbacks in guest code.
    listens: HashMap<(u64, &'static str), bool>,
}

// ---- objects ----

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn anchor(emu: &mut Emu, item: u64, a: Attr) -> Result<u64> {
    if let Some(&x) = emu.layout.anchors.get(&(item, a)) {
        return Ok(x);
    }
    let x = new_object(emu, a.anchor_class())?;
    emu.layout.anchors.insert((item, a), x);
    emu.layout.anchor_of.insert(x, (item, a));
    Ok(x)
}

fn view_guide(emu: &mut Emu, v: u64, kind: GuideKind) -> Result<u64> {
    if let Some(&g) = emu.layout.view_guides.get(&(v, kind)) {
        return Ok(g);
    }
    let g = new_object(emu, "UILayoutGuide")?;
    emu.layout.guides.insert(g, Guide { owner: v, kind });
    emu.layout.view_guides.insert((v, kind), g);
    Ok(g)
}

fn new_constraint(emu: &mut Emu, c: Constraint) -> Result<u64> {
    let o = new_object(emu, "NSLayoutConstraint")?;
    emu.layout.constraints.insert(o, c);
    Ok(o)
}

fn set_active(emu: &mut Emu, c: u64, on: bool) {
    let l = &mut emu.layout;
    if !l.constraints.contains_key(&c) {
        return;
    }
    let pos = l.active.iter().position(|&x| x == c);
    match (on, pos) {
        (true, None) => {
            l.active.push(c);
            // Activating a constraint between views hands their frames to Auto Layout
            // only if the app said so; UIKit does the same (the flag is the app's).
        }
        (false, Some(i)) => {
            l.active.remove(i);
        }
        _ => {}
    }
    mark_dirty(emu);
}

/// Something changed that layout depends on: lay the windows out again.
fn mark_dirty(emu: &mut Emu) {
    for w in emu.ui.windows.clone() {
        crate::uiview::view_mut(emu, w).needs_layout = true;
    }
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn fa(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret_f32(emu: &mut Emu, v: f32) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

// ---- anchors ----

/// `constraint…ToAnchor:` and friends: `self` relation `other` × multiplier + constant.
fn anchor_constraint(emu: &mut Emu, relation: i64, other: Option<u64>, multiplier: f64, constant: f64) -> Result<()> {
    let Some(&(first, a1)) = emu.layout.anchor_of.get(&emu.arg(0)) else { return ret(emu, 0) };
    let (second, a2) = match other {
        Some(o) => match emu.layout.anchor_of.get(&o) {
            Some(&(i, a)) => (i, Some(a)),
            None => (0, None),
        },
        None => (0, None),
    };
    let c = new_constraint(emu, Constraint { first, a1, relation, second, a2, multiplier, constant, priority: REQUIRED })?;
    ret(emu, c)
}

const AN: &str = "NSLayoutAnchor";
const DIM: &str = "NSLayoutDimension";
const X: &str = "NSLayoutXAxisAnchor";
const Y: &str = "NSLayoutYAxisAnchor";

// ---- solving ----

/// Per item, per axis: [min, max, center, size], as far as known (window coordinates).
type Slots = [[Option<f64>; 4]; 2];

/// Fill in an axis from any two of its values.
fn settle(s: &mut [Option<f64>; 4]) {
    let [min, max, c, size] = *s;
    let (min, size) = match (min, max, c, size) {
        (Some(a), _, _, Some(w)) => (a, w),
        (Some(a), Some(b), _, _) => (a, b - a),
        (_, Some(b), _, Some(w)) => (b - w, w),
        (_, _, Some(m), Some(w)) => (m - w / 2.0, w),
        (Some(a), _, Some(m), _) => (a, 2.0 * (m - a)),
        (_, Some(b), Some(m), _) => (2.0 * m - b, 2.0 * (b - m)),
        _ => return,
    };
    *s = [Some(min), Some(min + size), Some(min + size / 2.0), Some(size)];
}

struct Solver {
    items: HashMap<u64, Slots>,
    changed: bool,
}

impl Solver {
    fn get(&self, item: u64, a: Attr) -> Option<f64> {
        self.items.get(&item)?[a.axis()][a.slot()]
    }
    fn set(&mut self, item: u64, a: Attr, v: f64) {
        let Some(s) = self.items.get_mut(&item) else { return };
        let axis = &mut s[a.axis()];
        if axis[a.slot()].is_some() || !v.is_finite() {
            return;
        }
        axis[a.slot()] = Some(v);
        settle(axis);
        self.changed = true;
    }
    fn known(&self, item: u64, axis: usize) -> Option<(f64, f64)> {
        let s = self.items.get(&item)?[axis];
        Some((s[0]?, s[3]?))
    }
    fn fix(&mut self, item: u64, r: [f64; 4]) {
        self.set(item, Left, r[0]);
        self.set(item, Width, r[2]);
        self.set(item, Top, r[1]);
        self.set(item, Height, r[3]);
    }
}

/// A view's rect in window coordinates.
fn window_rect(emu: &mut Emu, v: u64) -> [f64; 4] {
    let o = window_origin(emu, v);
    let b = view(emu, v).bounds;
    [o[0], o[1], b[2], b[3]]
}

fn is_vc_root(emu: &Emu, v: u64) -> bool {
    emu.ui.views.vc_view.values().any(|&x| x == v)
}

/// A view's layout margins (top, left, bottom, right), not counting the safe
/// area: the app's, or UIKit's defaults: a view controller's root view gets
/// the system minimum (16 pt at the sides, 20 on wider phones), others 8 pt.
fn margins_of(emu: &Emu, v: u64) -> [f64; 4] {
    if let Some(&m) = emu.layout.margins.get(&v) {
        return m;
    }
    if is_vc_root(emu, v) {
        let side = if emu.device.width >= 414.0 { 20.0 } else { 16.0 };
        [0.0, side, 0.0, side]
    } else {
        [8.0; 4]
    }
}

/// How far a guide sits inside its owner at the left and right.
fn guide_insets_x(emu: &Emu, g: Guide) -> (f64, f64) {
    match g.kind {
        GuideKind::Margins => {
            let m = margins_of(emu, g.owner);
            (m[1], m[3])
        }
        _ => (0.0, 0.0),
    }
}

/// A guide's rect along one axis from its owner's (min, size) there.
fn guide_axis(emu: &mut Emu, g: Guide, axis: usize, (min, size): (f64, f64)) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (min, min + size);
    if axis == 1 && g.kind != GuideKind::Custom {
        let (top, bottom) = crate::navigation::safe_band(emu, g.owner);
        if g.kind == GuideKind::Keyboard {
            let kb_top = if emu.input.keyboard_shown { emu.device.height - emu.input.keyboard_height } else { bottom };
            return Some((kb_top, emu.device.height - kb_top));
        }
        lo = lo.max(top);
        hi = hi.min(bottom).max(lo);
    }
    if g.kind == GuideKind::Margins {
        let m = margins_of(emu, g.owner);
        let (a, b) = if axis == 0 { (m[1], m[3]) } else { (m[0], m[2]) };
        lo += a;
        hi = (hi - b).max(lo);
    }
    match g.kind {
        GuideKind::Custom => None,
        _ => Some((lo, hi - lo)),
    }
}

/// Lay out every view and guide that constraints govern. Returns whether a frame changed.
pub fn solve(emu: &mut Emu) -> Result<bool> {
    if emu.layout.active.is_empty() {
        return Ok(false);
    }
    let cs: Vec<Constraint> = emu.layout.active.iter().filter_map(|c| emu.layout.constraints.get(c).copied()).collect();
    // Every item involved, and the owners of the guides among them.
    let mut todo: Vec<u64> = cs.iter().flat_map(|c| [c.first, c.second]).filter(|&i| i != 0).collect();
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    while let Some(i) = todo.pop() {
        if !seen.insert(i) {
            continue;
        }
        if let Some(g) = emu.layout.guides.get(&i)
            && g.owner != 0
        {
            todo.push(g.owner);
        }
        items.push(i);
    }
    let mut s = Solver { items: items.iter().map(|&i| (i, [[None; 4]; 2])).collect(), changed: false };
    // Views that aren't Auto Layout's keep their frames.
    let auto = |emu: &Emu, i: u64| emu.layout.auto.contains(&i);
    for &i in &items {
        if emu.layout.guides.contains_key(&i) || auto(emu, i) || !emu.ui.views.views.contains_key(&i) {
            continue;
        }
        let r = window_rect(emu, i);
        s.fix(i, r);
    }

    let guides: Vec<(u64, Guide)> = items.iter().filter_map(|&i| emu.layout.guides.get(&i).map(|&g| (i, g))).collect();
    let required: Vec<&Constraint> = cs.iter().filter(|c| c.relation == 0 && c.priority >= REQUIRED).collect();
    let optional: Vec<&Constraint> = cs.iter().filter(|c| c.relation == 0 && c.priority < REQUIRED).collect();
    let inequalities: Vec<&Constraint> = cs.iter().filter(|c| c.relation != 0).collect();

    let apply = |s: &mut Solver, c: &Constraint| {
        let lhs = s.get(c.first, c.a1);
        let rhs = match c.a2 {
            Some(a2) if c.second != 0 => s.get(c.second, a2).map(|v| c.multiplier * v + c.constant),
            _ => Some(c.constant),
        };
        match (lhs, rhs) {
            (None, Some(r)) => s.set(c.first, c.a1, r),
            (Some(l), None) if c.multiplier != 0.0 => {
                if let Some(a2) = c.a2 {
                    s.set(c.second, a2, (l - c.constant) / c.multiplier)
                }
            }
            _ => {}
        }
    };
    let propagate = |emu: &mut Emu, s: &mut Solver, sets: &[&[&Constraint]]| {
        for _ in 0..64 {
            s.changed = false;
            for &(g, guide) in &guides {
                if guide.kind == GuideKind::Custom {
                    continue;
                }
                for axis in 0..2 {
                    let (a, w) = if axis == 0 { (Left, Width) } else { (Top, Height) };
                    if let Some(owner) = s.known(guide.owner, axis)
                        && let Some((min, size)) = guide_axis(emu, guide, axis, owner)
                    {
                        s.set(g, a, min);
                        s.set(g, w, size);
                    } else if axis == 0 {
                        // Across, a guide is its owner inset by fixed amounts: either sets the other.
                        let (lo, hi) = guide_insets_x(emu, guide);
                        if let Some(v) = s.get(guide.owner, Left) {
                            s.set(g, Left, v + lo);
                        }
                        if let Some(v) = s.get(guide.owner, Right) {
                            s.set(g, Right, v - hi);
                        }
                        if let Some(v) = s.get(g, Left) {
                            s.set(guide.owner, Left, v - lo);
                        }
                        if let Some(v) = s.get(g, Right) {
                            s.set(guide.owner, Right, v + hi);
                        }
                    } else if guide.kind == GuideKind::Keyboard
                        && let Some((min, size)) = guide_axis(emu, guide, 1, (0.0, 0.0))
                    {
                        s.set(g, Top, min);
                        s.set(g, Height, size);
                    }
                }
            }
            for set in sets {
                for c in *set {
                    apply(s, c);
                }
            }
            if !s.changed {
                break;
            }
        }
    };
    propagate(emu, &mut s, &[&required]);
    propagate(emu, &mut s, &[&required, &optional]);
    // Intrinsic content sizes, for views whose size is still open.
    for &i in &items {
        if !auto(emu, i) {
            continue;
        }
        let open = [s.known(i, 0).is_none(), s.known(i, 1).is_none()];
        if open[0] || open[1] {
            let r = send_fp(emu, i, "intrinsicContentSize", &[], &[])?;
            if open[0] && r[0] >= 0.0 && s.get(i, Width).is_none() {
                s.set(i, Width, r[0]);
            }
            if open[1] && r[1] >= 0.0 && s.get(i, Height).is_none() {
                s.set(i, Height, r[1]);
            }
        }
    }
    propagate(emu, &mut s, &[&required, &optional]);
    propagate(emu, &mut s, &[&required, &optional, &inequalities]);
    // What's still open keeps its current frame.
    for &i in &items {
        if !auto(emu, i) && !emu.layout.guides.contains_key(&i) {
            continue;
        }
        let cur = match emu.layout.guides.get(&i) {
            Some(g) => {
                let o = if g.owner != 0 { window_rect(emu, g.owner) } else { [0.0; 4] };
                let f = emu.layout.guide_frames.get(&i).copied().unwrap_or([0.0; 4]);
                [o[0] + f[0], o[1] + f[1], f[2], f[3]]
            }
            None => window_rect(emu, i),
        };
        s.set(i, Width, cur[2]);
        s.set(i, Height, cur[3]);
        s.set(i, Left, cur[0]);
        s.set(i, Top, cur[1]);
        propagate(emu, &mut s, &[&required, &optional, &inequalities]);
    }

    // Write the frames back, outermost views first (their children's frames are relative to them).
    let depth = |emu: &mut Emu, mut v: u64| {
        let mut d = 0;
        while v != 0 {
            v = view(emu, v).superview;
            d += 1;
        }
        d
    };
    let mut views: Vec<(usize, u64)> = Vec::new();
    for &i in &items {
        if auto(emu, i) && emu.ui.views.views.contains_key(&i) {
            views.push((depth(emu, i), i));
        }
    }
    views.sort();
    let mut moved = false;
    for (_, v) in views {
        let (Some((x, w)), Some((y, h))) = (s.known(v, 0), s.known(v, 1)) else { continue };
        let sup = view(emu, v).superview;
        let (o, sb) = if sup != 0 { (window_origin(emu, sup), view(emu, sup).bounds) } else { ([0.0; 2], [0.0; 4]) };
        let r = [x - o[0] + sb[0], y - o[1] + sb[1], w.max(0.0), h.max(0.0)];
        let cur = frame_of(emu, v);
        if cur.iter().zip(r).any(|(a, b)| (a - b).abs() > 1e-6) {
            set_frame_of(emu, v, r);
            moved = true;
        }
    }
    for (g, guide) in guides {
        if guide.kind != GuideKind::Custom {
            continue;
        }
        if let (Some((x, w)), Some((y, h))) = (s.known(g, 0), s.known(g, 1)) {
            let o = if guide.owner != 0 { window_rect(emu, guide.owner) } else { [0.0; 4] };
            emu.layout.guide_frames.insert(g, [x - o[0], y - o[1], w, h]);
        }
    }
    Ok(moved)
}

/// Whether `obj`'s class implements `sel` in the app (not just pomme's default).
fn listens(emu: &mut Emu, obj: u64, sel: &'static str) -> Result<bool> {
    let cls = objc::class_of(emu, obj)?;
    if let Some(&b) = emu.layout.listens.get(&(cls, sel)) {
        return Ok(b);
    }
    let s = objc::intern(emu, sel)?;
    let b = matches!(objc::lookup(emu, cls, s)?, Some(objc::Imp::Guest(_)));
    emu.layout.listens.insert((cls, sel), b);
    Ok(b)
}

/// The safe-area callbacks: views (and view controllers, for their root
/// views) that implement them hear when their insets change, including
/// the first time they're in a window.
pub fn safe_area_changes(emu: &mut Emu) -> Result<bool> {
    let mut todo: Vec<u64> = emu.ui.windows.clone();
    let mut any = false;
    while let Some(v) = todo.pop() {
        let vw = view(emu, v);
        todo.extend(vw.subviews.iter().rev());
        let vc = emu.ui.views.vc_view.iter().find(|&(_, &x)| x == v).map(|(&vc, _)| vc);
        let wants_view = listens(emu, v, "safeAreaInsetsDidChange")?;
        let wants_vc = match vc {
            Some(c) => listens(emu, c, "viewSafeAreaInsetsDidChange")?,
            None => false,
        };
        if !wants_view && !wants_vc {
            continue;
        }
        let insets = if emu.ui.windows.contains(&v) { emu.device.safe_area() } else { crate::navigation::safe_area(emu, v) };
        if emu.layout.told.get(&v).copied().unwrap_or([0.0; 4]) == insets {
            continue;
        }
        emu.layout.told.insert(v, insets);
        crate::uiview::view_mut(emu, v).needs_layout = true;
        any = true;
        if wants_view {
            send(emu, v, "safeAreaInsetsDidChange", &[])?;
        }
        if let Some(c) = vc
            && wants_vc
        {
            send(emu, c, "viewSafeAreaInsetsDidChange", &[])?;
        }
    }
    Ok(any)
}

// ---- host methods ----

fn view_anchor(emu: &mut Emu, a: Attr) -> Result<()> {
    let x = anchor(emu, emu.arg(0), a)?;
    ret(emu, x)
}

fn ret_view_guide(emu: &mut Emu, kind: GuideKind) -> Result<()> {
    let g = view_guide(emu, emu.arg(0), kind)?;
    ret(emu, g)
}

/// `+constraintWithItem:attribute:relatedBy:toItem:attribute:multiplier:constant:`
fn constraint_with_item(emu: &mut Emu) -> Result<()> {
    let (item1, attr1, relation, item2, attr2) = (emu.arg(2), emu.arg(3), emu.arg(4) as i64, emu.arg(5), emu.arg(6));
    let (multiplier, constant) = (fa(emu, 0), fa(emu, 1));
    let Some((a1, m1)) = from_ns(attr1) else { return ret(emu, 0) };
    let first = if m1 && !emu.layout.guides.contains_key(&item1) { view_guide(emu, item1, GuideKind::Margins)? } else { item1 };
    let (second, a2) = match from_ns(attr2) {
        Some((a2, m2)) if item2 != 0 => (if m2 && !emu.layout.guides.contains_key(&item2) { view_guide(emu, item2, GuideKind::Margins)? } else { item2 }, Some(a2)),
        _ => (0, None),
    };
    let c = new_constraint(emu, Constraint { first, a1, relation, second, a2, multiplier, constant, priority: REQUIRED })?;
    ret(emu, c)
}

fn activate_all(emu: &mut Emu, on: bool) -> Result<()> {
    let list = emu.arg(2);
    if list != 0 {
        for c in foundation::array_values(emu, list)? {
            set_active(emu, c, on);
        }
    }
    Ok(())
}

fn with_constraint(emu: &mut Emu, f: impl FnOnce(&mut Constraint)) {
    if let Some(c) = emu.layout.constraints.get_mut(&emu.cpu.x[0]) {
        f(c);
    }
    mark_dirty(emu);
}

fn constraint(emu: &Emu) -> Option<Constraint> {
    emu.layout.constraints.get(&emu.cpu.x[0]).copied()
}

fn add_constraints(emu: &mut Emu, list: Vec<u64>) {
    let v = emu.arg(0);
    for c in list {
        emu.layout.added.entry(v).or_default().push(c);
        set_active(emu, c, true);
    }
}

fn remove_constraints(emu: &mut Emu, list: Vec<u64>) {
    let v = emu.arg(0);
    for c in list {
        if let Some(l) = emu.layout.added.get_mut(&v) {
            l.retain(|&x| x != c);
        }
        set_active(emu, c, false);
    }
}

/// UIEdgeInsets / NSDirectionalEdgeInsets in d0–d3 (top, left/leading, bottom, right/trailing).
fn set_margins(emu: &mut Emu) -> Result<()> {
    let (v, m) = (emu.arg(0), [fa(emu, 0), fa(emu, 1), fa(emu, 2), fa(emu, 3)]);
    emu.layout.margins.insert(v, m);
    mark_dirty(emu);
    Ok(())
}

/// `-layoutMargins`: the margins as they apply, the safe area included.
fn get_margins(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let m = margins_of(emu, v);
    let s = if emu.ui.windows.contains(&v) { emu.device.safe_area() } else { crate::navigation::safe_area(emu, v) };
    ret_rect(emu, [m[0] + s[0], m[1] + s[1], m[2] + s[2], m[3] + s[3]]);
    Ok(())
}

fn guide_frame(emu: &mut Emu) -> Result<()> {
    let g = emu.arg(0);
    let r = match emu.layout.guides.get(&g).copied() {
        Some(Guide { owner, kind }) if kind != GuideKind::Custom && owner != 0 => {
            let o = window_rect(emu, owner);
            let x = guide_axis(emu, Guide { owner, kind }, 0, (o[0], o[2])).unwrap_or((o[0], o[2]));
            let y = guide_axis(emu, Guide { owner, kind }, 1, (o[1], o[3])).unwrap_or((o[1], o[3]));
            [x.0 - o[0], y.0 - o[1], x.1, y.1]
        }
        _ => emu.layout.guide_frames.get(&g).copied().unwrap_or([0.0; 4]),
    };
    ret_rect(emu, r);
    Ok(())
}

const V: &str = "UIView";
const G: &str = "UILayoutGuide";
const C: &str = "NSLayoutConstraint";

macro_rules! anchors {
    ($class:expr, $f:ident) => {
        [
            ($class, false, "leadingAnchor", (|e| $f(e, Left)) as HostFn),
            ($class, false, "leftAnchor", |e| $f(e, Left)),
            ($class, false, "trailingAnchor", |e| $f(e, Right)),
            ($class, false, "rightAnchor", |e| $f(e, Right)),
            ($class, false, "topAnchor", |e| $f(e, Top)),
            ($class, false, "bottomAnchor", |e| $f(e, Bottom)),
            ($class, false, "widthAnchor", |e| $f(e, Width)),
            ($class, false, "heightAnchor", |e| $f(e, Height)),
            ($class, false, "centerXAnchor", |e| $f(e, CenterX)),
            ($class, false, "centerYAnchor", |e| $f(e, CenterY)),
            ($class, false, "firstBaselineAnchor", |e| $f(e, Top)),
            ($class, false, "lastBaselineAnchor", |e| $f(e, Bottom)),
        ]
    };
}

pub(crate) const ANCHOR_METHODS: [[(&str, bool, &str, HostFn); 12]; 2] = [anchors!(V, view_anchor), anchors!(G, view_anchor)];

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // UIView
    (V, false, "safeAreaLayoutGuide", |e| ret_view_guide(e, GuideKind::Safe)),
    (V, false, "layoutMarginsGuide", |e| ret_view_guide(e, GuideKind::Margins)),
    (V, false, "readableContentGuide", |e| ret_view_guide(e, GuideKind::Margins)),
    (V, false, "keyboardLayoutGuide", |e| ret_view_guide(e, GuideKind::Keyboard)),
    (V, false, "translatesAutoresizingMaskIntoConstraints", |e| {
        let r = !e.layout.auto.contains(&e.arg(0)) as u64;
        ret(e, r)
    }),
    (V, false, "setTranslatesAutoresizingMaskIntoConstraints:", |e| {
        let (v, on) = (e.arg(0), e.arg(2) & 0xff != 0);
        if on {
            e.layout.auto.remove(&v);
        } else {
            e.layout.auto.insert(v);
        }
        mark_dirty(e);
        Ok(())
    }),
    (V, false, "addConstraint:", |e| {
        let c = e.arg(2);
        add_constraints(e, vec![c]);
        Ok(())
    }),
    (V, false, "addConstraints:", |e| {
        let l = foundation::array_values(e, e.arg(2))?;
        add_constraints(e, l);
        Ok(())
    }),
    (V, false, "removeConstraint:", |e| {
        let c = e.arg(2);
        remove_constraints(e, vec![c]);
        Ok(())
    }),
    (V, false, "removeConstraints:", |e| {
        let l = foundation::array_values(e, e.arg(2))?;
        remove_constraints(e, l);
        Ok(())
    }),
    (V, false, "constraints", |e| {
        let l = e.layout.added.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = foundation::new_array_of(e, &l)?;
        ret(e, a)
    }),
    (V, false, "addLayoutGuide:", |e| {
        let (v, g) = (e.arg(0), e.arg(2));
        e.layout.guides.insert(g, Guide { owner: v, kind: GuideKind::Custom });
        e.layout.added_guides.entry(v).or_default().push(g);
        Ok(())
    }),
    (V, false, "removeLayoutGuide:", |e| {
        let (v, g) = (e.arg(0), e.arg(2));
        if let Some(l) = e.layout.added_guides.get_mut(&v) {
            l.retain(|&x| x != g);
        }
        if let Some(guide) = e.layout.guides.get_mut(&g) {
            guide.owner = 0;
        }
        Ok(())
    }),
    (V, false, "layoutGuides", |e| {
        let l = e.layout.added_guides.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = foundation::new_array_of(e, &l)?;
        ret(e, a)
    }),
    (V, false, "layoutMargins", get_margins),
    (V, false, "directionalLayoutMargins", get_margins),
    (V, false, "setLayoutMargins:", set_margins),
    (V, false, "setDirectionalLayoutMargins:", set_margins),
    (V, false, "layoutMarginsDidChange", |_| Ok(())),
    (V, false, "setNeedsUpdateConstraints", |_| Ok(())),
    (V, false, "updateConstraintsIfNeeded", |_| Ok(())),
    (V, false, "updateConstraints", |_| Ok(())),
    (V, false, "needsUpdateConstraints", |e| ret(e, 0)),
    (V, false, "invalidateIntrinsicContentSize", |e| {
        mark_dirty(e);
        Ok(())
    }),
    (V, false, "setContentHuggingPriority:forAxis:", |_| Ok(())),
    (V, false, "setContentCompressionResistancePriority:forAxis:", |_| Ok(())),
    (V, false, "contentHuggingPriorityForAxis:", |e| {
        ret_f32(e, 250.0);
        Ok(())
    }),
    (V, false, "contentCompressionResistancePriorityForAxis:", |e| {
        ret_f32(e, 750.0);
        Ok(())
    }),
    (V, false, "systemLayoutSizeFittingSize:", |e| {
        let b = view(e, e.arg(0)).bounds;
        ret_rect(e, [b[2], b[3], 0.0, 0.0]);
        Ok(())
    }),
    (V, false, "systemLayoutSizeFittingSize:withHorizontalFittingPriority:verticalFittingPriority:", |e| {
        let b = view(e, e.arg(0)).bounds;
        ret_rect(e, [b[2], b[3], 0.0, 0.0]);
        Ok(())
    }),
    ("UIViewController", false, "viewSafeAreaInsetsDidChange", |_| Ok(())),
    ("UIViewController", false, "viewLayoutMarginsDidChange", |_| Ok(())),
    ("UIViewController", false, "updateViewConstraints", |_| Ok(())),
    // UILayoutGuide
    (G, false, "init", |e| {
        let g = e.arg(0);
        e.layout.guides.insert(g, Guide { owner: 0, kind: GuideKind::Custom });
        ret(e, g)
    }),
    (G, false, "owningView", |e| {
        let o = e.layout.guides.get(&e.arg(0)).map(|g| g.owner).unwrap_or(0);
        ret(e, o)
    }),
    (G, false, "layoutFrame", guide_frame),
    // Anchors
    (AN, false, "constraintEqualToAnchor:", |e| {
        let o = e.arg(2);
        anchor_constraint(e, 0, Some(o), 1.0, 0.0)
    }),
    (AN, false, "constraintGreaterThanOrEqualToAnchor:", |e| {
        let o = e.arg(2);
        anchor_constraint(e, 1, Some(o), 1.0, 0.0)
    }),
    (AN, false, "constraintLessThanOrEqualToAnchor:", |e| {
        let o = e.arg(2);
        anchor_constraint(e, -1, Some(o), 1.0, 0.0)
    }),
    (AN, false, "constraintEqualToAnchor:constant:", |e| {
        let (o, k) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 0, Some(o), 1.0, k)
    }),
    (AN, false, "constraintGreaterThanOrEqualToAnchor:constant:", |e| {
        let (o, k) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 1, Some(o), 1.0, k)
    }),
    (AN, false, "constraintLessThanOrEqualToAnchor:constant:", |e| {
        let (o, k) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, -1, Some(o), 1.0, k)
    }),
    (DIM, false, "constraintEqualToConstant:", |e| {
        let k = fa(e, 0);
        anchor_constraint(e, 0, None, 1.0, k)
    }),
    (DIM, false, "constraintGreaterThanOrEqualToConstant:", |e| {
        let k = fa(e, 0);
        anchor_constraint(e, 1, None, 1.0, k)
    }),
    (DIM, false, "constraintLessThanOrEqualToConstant:", |e| {
        let k = fa(e, 0);
        anchor_constraint(e, -1, None, 1.0, k)
    }),
    (DIM, false, "constraintEqualToAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 0, Some(o), m, 0.0)
    }),
    (DIM, false, "constraintGreaterThanOrEqualToAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 1, Some(o), m, 0.0)
    }),
    (DIM, false, "constraintLessThanOrEqualToAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, -1, Some(o), m, 0.0)
    }),
    (DIM, false, "constraintEqualToAnchor:multiplier:constant:", |e| {
        let (o, m, k) = (e.arg(2), fa(e, 0), fa(e, 1));
        anchor_constraint(e, 0, Some(o), m, k)
    }),
    (DIM, false, "constraintGreaterThanOrEqualToAnchor:multiplier:constant:", |e| {
        let (o, m, k) = (e.arg(2), fa(e, 0), fa(e, 1));
        anchor_constraint(e, 1, Some(o), m, k)
    }),
    (DIM, false, "constraintLessThanOrEqualToAnchor:multiplier:constant:", |e| {
        let (o, m, k) = (e.arg(2), fa(e, 0), fa(e, 1));
        anchor_constraint(e, -1, Some(o), m, k)
    }),
    (X, false, "constraintEqualToSystemSpacingAfterAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 0, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    (X, false, "constraintGreaterThanOrEqualToSystemSpacingAfterAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 1, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    (X, false, "constraintLessThanOrEqualToSystemSpacingAfterAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, -1, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    (Y, false, "constraintEqualToSystemSpacingBelowAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 0, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    (Y, false, "constraintGreaterThanOrEqualToSystemSpacingBelowAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, 1, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    (Y, false, "constraintLessThanOrEqualToSystemSpacingBelowAnchor:multiplier:", |e| {
        let (o, m) = (e.arg(2), fa(e, 0));
        anchor_constraint(e, -1, Some(o), 1.0, SYSTEM_SPACING * m)
    }),
    // NSLayoutConstraint
    (C, true, "constraintWithItem:attribute:relatedBy:toItem:attribute:multiplier:constant:", constraint_with_item),
    (C, true, "activateConstraints:", |e| activate_all(e, true)),
    (C, true, "deactivateConstraints:", |e| activate_all(e, false)),
    (C, false, "isActive", |e| {
        let on = e.layout.active.contains(&e.arg(0)) as u64;
        ret(e, on)
    }),
    (C, false, "active", |e| {
        let on = e.layout.active.contains(&e.arg(0)) as u64;
        ret(e, on)
    }),
    (C, false, "setActive:", |e| {
        let (c, on) = (e.arg(0), e.arg(2) & 0xff != 0);
        set_active(e, c, on);
        Ok(())
    }),
    (C, false, "constant", |e| {
        let k = constraint(e).map(|c| c.constant).unwrap_or(0.0);
        ret_f64(e, k);
        Ok(())
    }),
    (C, false, "setConstant:", |e| {
        let k = fa(e, 0);
        with_constraint(e, |c| c.constant = k);
        Ok(())
    }),
    (C, false, "multiplier", |e| {
        let m = constraint(e).map(|c| c.multiplier).unwrap_or(1.0);
        ret_f64(e, m);
        Ok(())
    }),
    (C, false, "priority", |e| {
        let p = constraint(e).map(|c| c.priority).unwrap_or(REQUIRED);
        ret_f32(e, p);
        Ok(())
    }),
    (C, false, "setPriority:", |e| {
        let p = f32::from_bits(e.cpu.q[0] as u32);
        with_constraint(e, |c| c.priority = p);
        Ok(())
    }),
    (C, false, "relation", |e| {
        let r = constraint(e).map(|c| c.relation).unwrap_or(0) as u64;
        ret(e, r)
    }),
    (C, false, "firstItem", |e| {
        let i = constraint(e).map(|c| c.first).unwrap_or(0);
        ret(e, i)
    }),
    (C, false, "secondItem", |e| {
        let i = constraint(e).map(|c| c.second).unwrap_or(0);
        ret(e, i)
    }),
    (C, false, "firstAttribute", |e| {
        let a = constraint(e).map(|c| to_ns(c.a1)).unwrap_or(0);
        ret(e, a)
    }),
    (C, false, "secondAttribute", |e| {
        let a = constraint(e).and_then(|c| c.a2).map(to_ns).unwrap_or(0);
        ret(e, a)
    }),
    (C, false, "firstAnchor", |e| {
        let Some(c) = constraint(e) else { return ret(e, 0) };
        let x = anchor(e, c.first, c.a1)?;
        ret(e, x)
    }),
    (C, false, "secondAnchor", |e| {
        let Some(c) = constraint(e) else { return ret(e, 0) };
        let x = match c.a2 {
            Some(a) if c.second != 0 => anchor(e, c.second, a)?,
            _ => 0,
        };
        ret(e, x)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_axis_settles_from_any_two_values() {
        let full = [Some(10.0), Some(30.0), Some(20.0), Some(20.0)];
        for (i, j) in [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)] {
            let mut s = [None; 4];
            s[i] = full[i];
            s[j] = full[j];
            settle(&mut s);
            assert_eq!(s, full, "from slots {i} and {j}");
        }
        let mut s = [Some(1.0), None, None, None];
        settle(&mut s);
        assert_eq!(s, [Some(1.0), None, None, None]);
    }

    #[test]
    fn attributes_from_nslayoutattribute() {
        assert_eq!(from_ns(5), Some((Left, false)));
        assert_eq!(from_ns(18), Some((Right, true)));
        assert_eq!(from_ns(0), None);
        for a in [Left, Right, Top, Bottom, Width, Height, CenterX, CenterY] {
            assert_eq!(from_ns(to_ns(a)), Some((a, false)));
        }
    }
}
