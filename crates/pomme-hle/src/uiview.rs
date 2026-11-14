//! UIView, UIWindow, UIViewController, UIColor, CALayer.
//!
//! View state is kept host-side, keyed by object address, not in the object:
//! apps subclass UIView (React Native's component views do) with ivars laid
//! out after the real UIView's, so pomme must not use the object's memory.
//!
//! Geometry follows the ABI: CGRect/CGPoint/CGSize are homogeneous float
//! aggregates, passed and returned in d0–d3.

use crate::objc::{self, host_class, send};
use crate::uikit::{ret_f64, ret_rect};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct View {
    /// Position of the bounds origin in the superview: frame = (center - size/2, size).
    pub center: [f64; 2],
    /// bounds = (origin, size)
    pub bounds: [f64; 4],
    pub superview: u64,
    pub subviews: Vec<u64>,
    pub layer: u64,
    pub background: u64,
    pub hidden: bool,
    pub alpha: f64,
    pub tag: u64,
    pub user_interaction: bool,
    pub clips: bool,
    pub autoresizing: u64,
    pub content_mode: u64,
    pub window: u64,
    /// `layoutSubviews` is due at the next layout pass.
    pub needs_layout: bool,
    /// `drawRect:` is due at the next frame (`setNeedsDisplay`).
    pub needs_display: bool,
}

#[derive(Debug, Default)]
pub struct Views {
    pub views: HashMap<u64, View>,
    /// UIViewController → its view, and root view controllers of windows.
    pub vc_view: HashMap<u64, u64>,
    pub root_vc: HashMap<u64, u64>,
    /// Layer → view.
    pub layer_view: HashMap<u64, u64>,
    /// UIColor objects: rgba.
    pub colors: HashMap<u64, [f64; 4]>,
    pub device: u64,
    /// Layer state (see calayer.rs).
    pub layers: HashMap<u64, crate::calayer::Layer>,
}

impl Views {
    pub fn count(&self) -> usize {
        self.views.len()
    }
}

fn f(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn rect_arg(emu: &Emu) -> [f64; 4] {
    [f(emu, 0), f(emu, 1), f(emu, 2), f(emu, 3)]
}

/// A view's state, to change: the screen is due a new frame.
pub(crate) fn view_mut(emu: &mut Emu, v: u64) -> &mut View {
    crate::host::changed(emu);
    view_entry(emu, v)
}

/// A view's state (made on first use), without marking the screen changed:
/// for reading, and for the renderer's own bookkeeping.
pub(crate) fn view_entry(emu: &mut Emu, v: u64) -> &mut View {
    if !emu.ui.views.views.contains_key(&v) {
        let user_interaction = interactive_by_default(emu, v);
        emu.ui.views.views.insert(v, View { alpha: 1.0, user_interaction, needs_layout: true, ..Default::default() });
    }
    emu.ui.views.views.get_mut(&v).unwrap()
}

/// UIKit's default `userInteractionEnabled`: YES, but NO for labels and image
/// views (touches on them go to the view behind: a button's title, say).
fn interactive_by_default(emu: &Emu, v: u64) -> bool {
    let Ok(mut cls) = emu.read_u64(v) else { return true };
    while let Some(c) = emu.objc.class(cls) {
        if c.host && matches!(c.name.as_str(), "UILabel" | "UIImageView") {
            return false;
        }
        cls = c.superclass;
    }
    true
}

pub(crate) fn view(emu: &mut Emu, v: u64) -> View {
    view_entry(emu, v).clone()
}

pub(crate) fn set_frame_of(emu: &mut Emu, v: u64, r: [f64; 4]) {
    use crate::animation::{Prop, after, before};
    let (pos, size) = (before(emu, v, Prop::Position), before(emu, v, Prop::Bounds));
    set_frame_now(emu, v, r);
    after(emu, v, Prop::Position, pos);
    after(emu, v, Prop::Bounds, size);
}

fn set_frame_now(emu: &mut Emu, v: u64, r: [f64; 4]) {
    let vw = view_mut(emu, v);
    let old = [vw.bounds[2], vw.bounds[3]];
    if vw.bounds[2] != r[2] || vw.bounds[3] != r[3] {
        vw.needs_layout = true;
    }
    vw.bounds[2] = r[2];
    vw.bounds[3] = r[3];
    vw.center = [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
    autoresize_subviews(emu, v, old, [r[2], r[3]]);
}

/// One axis of autoresizing: the parent grew by `delta`; the flexible parts
/// of (leading margin, size, trailing margin) share it in proportion to their
/// sizes (equally when they're all zero). Returns the new (origin, size).
pub(crate) fn autoresize_axis(origin: f64, size: f64, parent: f64, delta: f64, flexible: [bool; 3]) -> (f64, f64) {
    let parts = [origin, size, parent - origin - size];
    let total: f64 = (0..3).filter(|&i| flexible[i]).map(|i| parts[i]).sum();
    let n = flexible.iter().filter(|&&f| f).count() as f64;
    if n == 0.0 {
        return (origin, size);
    }
    let share = |i: usize| if !flexible[i] { 0.0 } else if total > 0.0 { delta * parts[i] / total } else { delta / n };
    (origin + share(0), size + share(1))
}

/// UIKit's autoresizing: when `v` changes size, subviews with an
/// autoresizingMask follow (FlexibleLeftMargin 1, Width 2, RightMargin 4,
/// TopMargin 8, Height 16, BottomMargin 32).
fn autoresize_subviews(emu: &mut Emu, v: u64, old: [f64; 2], new: [f64; 2]) {
    if old == new {
        return;
    }
    for s in view(emu, v).subviews {
        let m = view(emu, s).autoresizing;
        if m == 0 {
            continue;
        }
        let f = frame_of(emu, s);
        let (x, w) = autoresize_axis(f[0], f[2], old[0], new[0] - old[0], [m & 1 != 0, m & 2 != 0, m & 4 != 0]);
        let (y, h) = autoresize_axis(f[1], f[3], old[1], new[1] - old[1], [m & 8 != 0, m & 16 != 0, m & 32 != 0]);
        set_frame_of(emu, s, [x, y, w, h]);
    }
}

pub(crate) fn frame_of(emu: &mut Emu, v: u64) -> [f64; 4] {
    let vw = view(emu, v);
    [vw.center[0] - vw.bounds[2] / 2.0, vw.center[1] - vw.bounds[3] / 2.0, vw.bounds[2], vw.bounds[3]]
}

// ---- UIView ----

pub(crate) fn view_init_with_frame(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), rect_arg(emu));
    // As UIKit does, the view makes its layer as it's initialized (so it can be animated from the start).
    send(emu, v, "layer", &[])?;
    set_frame_of(emu, v, r);
    emu.ret(v);
    Ok(()) // returns self
}

/// `-init` is `-initWithFrame:CGRectZero`, so subclasses' initWithFrame: runs.
fn view_init(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    view_mut(emu, v);
    emu.cpu.q[..4].fill(0);
    let r = send(emu, v, "initWithFrame:", &[])?;
    emu.ret(r);
    Ok(())
}

fn view_frame(emu: &mut Emu) -> Result<()> {
    let r = frame_of(emu, emu.arg(0));
    ret_rect(emu, r);
    Ok(())
}

fn view_set_frame(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), rect_arg(emu));
    set_frame_of(emu, v, r);
    Ok(())
}

fn view_bounds(emu: &mut Emu) -> Result<()> {
    let b = view(emu, emu.arg(0)).bounds;
    ret_rect(emu, b);
    Ok(())
}

fn view_set_bounds(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), rect_arg(emu));
    let old = crate::animation::before(emu, v, crate::animation::Prop::Bounds);
    set_bounds_now(emu, v, r);
    crate::animation::after(emu, v, crate::animation::Prop::Bounds, old);
    Ok(())
}

fn set_bounds_now(emu: &mut Emu, v: u64, r: [f64; 4]) {
    let vw = view_mut(emu, v);
    let old = [vw.bounds[2], vw.bounds[3]];
    if vw.bounds[2] != r[2] || vw.bounds[3] != r[3] {
        vw.needs_layout = true;
    }
    vw.bounds = r;
    autoresize_subviews(emu, v, old, [r[2], r[3]]);
}

fn view_center(emu: &mut Emu) -> Result<()> {
    let c = view(emu, emu.arg(0)).center;
    emu.cpu.q[0] = c[0].to_bits() as u128;
    emu.cpu.q[1] = c[1].to_bits() as u128;
    Ok(())
}

fn view_set_center(emu: &mut Emu) -> Result<()> {
    let (v, x, y) = (emu.arg(0), f(emu, 0), f(emu, 1));
    let old = crate::animation::before(emu, v, crate::animation::Prop::Position);
    view_mut(emu, v).center = [x, y];
    crate::animation::after(emu, v, crate::animation::Prop::Position, old);
    Ok(())
}

fn detach(emu: &mut Emu, v: u64) {
    let sup = view(emu, v).superview;
    if sup != 0 {
        view_mut(emu, sup).subviews.retain(|&s| s != v);
    }
    view_mut(emu, v).superview = 0;
}

fn set_window_recursive(emu: &mut Emu, v: u64, w: u64) {
    view_mut(emu, v).window = w;
    for s in view(emu, v).subviews {
        set_window_recursive(emu, s, w);
    }
}

/// Send `sel` (with `args`) if the view implements it.
fn notify(emu: &mut Emu, v: u64, sel: &str, args: &[u64]) -> Result<()> {
    let s = objc::intern(emu, sel)?;
    if objc::responds(emu, v, s)? {
        send(emu, v, sel, args)?;
    }
    Ok(())
}

/// `sel` to `v` and every view under it, parents first (the window callbacks).
fn notify_tree(emu: &mut Emu, v: u64, sel: &str, args: &[u64]) -> Result<()> {
    notify(emu, v, sel, args)?;
    for s in view(emu, v).subviews {
        notify_tree(emu, s, sel, args)?;
    }
    Ok(())
}

/// Move `child` under `parent` (0: out of the hierarchy) with UIKit's
/// callbacks: willRemoveSubview: / willMoveToSuperview: / willMoveToWindow:
/// (whole subtree, when the window changes) before, and the did… ones after.
fn move_view(emu: &mut Emu, child: u64, parent: u64, index: Option<usize>) -> Result<()> {
    let old_parent = view(emu, child).superview;
    let old_window = view(emu, child).window;
    let new_window = if parent == 0 { 0 } else if emu.ui.windows.contains(&parent) { parent } else { view(emu, parent).window };
    let reparent = old_parent != parent;
    let rewindow = old_window != new_window;
    if reparent {
        if old_parent != 0 {
            notify(emu, old_parent, "willRemoveSubview:", &[child])?;
        }
        notify(emu, child, "willMoveToSuperview:", &[parent])?;
    }
    if rewindow {
        notify_tree(emu, child, "willMoveToWindow:", &[new_window])?;
    }
    detach(emu, child);
    if parent != 0 {
        let p = view_mut(emu, parent);
        let i = index.unwrap_or(p.subviews.len()).min(p.subviews.len());
        p.subviews.insert(i, child);
        p.needs_layout = true;
        view_mut(emu, child).superview = parent;
        view_mut(emu, child).needs_layout = true;
    }
    set_window_recursive(emu, child, new_window);
    if reparent {
        notify(emu, child, "didMoveToSuperview", &[])?;
    }
    if rewindow {
        notify_tree(emu, child, "didMoveToWindow", &[])?;
    }
    if reparent && parent != 0 {
        notify(emu, parent, "didAddSubview:", &[child])?;
    }
    Ok(())
}

fn insert_subview(emu: &mut Emu, parent: u64, child: u64, index: Option<usize>) -> Result<()> {
    if child == 0 || child == parent {
        return Ok(());
    }
    move_view(emu, child, parent, index)
}

fn add_subview(emu: &mut Emu) -> Result<()> {
    insert_subview(emu, emu.arg(0), emu.arg(2), None)
}

fn insert_subview_at(emu: &mut Emu) -> Result<()> {
    insert_subview(emu, emu.arg(0), emu.arg(2), Some(emu.arg(3) as usize))
}

fn insert_subview_relative(emu: &mut Emu, above: bool) -> Result<()> {
    let (parent, child, sibling) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let subs = view(emu, parent).subviews;
    let i = subs.iter().position(|&s| s == sibling).map(|i| if above { i + 1 } else { i });
    insert_subview(emu, parent, child, i)
}

fn remove_from_superview(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    if view(emu, v).superview == 0 {
        return Ok(());
    }
    move_view(emu, v, 0, None)
}

fn subviews(emu: &mut Emu) -> Result<()> {
    let s = view(emu, emu.arg(0)).subviews;
    let a = foundation::new_array_of(emu, &s)?;
    emu.ret(a);
    Ok(())
}

fn superview(emu: &mut Emu) -> Result<()> {
    let s = view(emu, emu.arg(0)).superview;
    emu.ret(s);
    Ok(())
}

fn view_window(emu: &mut Emu) -> Result<()> {
    let w = view(emu, emu.arg(0)).window;
    emu.ret(w);
    Ok(())
}

fn bring_to_front(emu: &mut Emu) -> Result<()> {
    let (p, c) = (emu.arg(0), emu.arg(2));
    let subs = &mut view_mut(emu, p).subviews;
    if let Some(i) = subs.iter().position(|&s| s == c) {
        subs.remove(i);
        subs.push(c);
    }
    Ok(())
}

fn background_color(emu: &mut Emu) -> Result<()> {
    let c = view(emu, emu.arg(0)).background;
    emu.ret(c);
    Ok(())
}

fn set_background_color(emu: &mut Emu) -> Result<()> {
    let (v, c) = (emu.arg(0), emu.arg(2));
    let old = crate::animation::before(emu, v, crate::animation::Prop::Background);
    view_mut(emu, v).background = c;
    crate::animation::after(emu, v, crate::animation::Prop::Background, old);
    Ok(())
}

fn is_hidden(emu: &mut Emu) -> Result<()> {
    let h = view(emu, emu.arg(0)).hidden;
    emu.ret(h as u64);
    Ok(())
}

fn set_hidden(emu: &mut Emu) -> Result<()> {
    let (v, h) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    view_mut(emu, v).hidden = h;
    Ok(())
}

fn alpha(emu: &mut Emu) -> Result<()> {
    let a = view(emu, emu.arg(0)).alpha;
    ret_f64(emu, a);
    Ok(())
}

fn set_alpha(emu: &mut Emu) -> Result<()> {
    let (v, a) = (emu.arg(0), f(emu, 0));
    let old = crate::animation::before(emu, v, crate::animation::Prop::Opacity);
    view_mut(emu, v).alpha = a;
    crate::animation::after(emu, v, crate::animation::Prop::Opacity, old);
    Ok(())
}

fn tag(emu: &mut Emu) -> Result<()> {
    let t = view(emu, emu.arg(0)).tag;
    emu.ret(t);
    Ok(())
}

fn set_tag(emu: &mut Emu) -> Result<()> {
    let (v, t) = (emu.arg(0), emu.arg(2));
    view_mut(emu, v).tag = t;
    Ok(())
}

fn set_user_interaction(emu: &mut Emu) -> Result<()> {
    let (v, b) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    view_mut(emu, v).user_interaction = b;
    Ok(())
}

fn user_interaction(emu: &mut Emu) -> Result<()> {
    let b = view(emu, emu.arg(0)).user_interaction;
    emu.ret(b as u64);
    Ok(())
}

fn set_clips(emu: &mut Emu) -> Result<()> {
    let (v, b) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    view_mut(emu, v).clips = b;
    Ok(())
}

fn clips(emu: &mut Emu) -> Result<()> {
    let b = view(emu, emu.arg(0)).clips;
    emu.ret(b as u64);
    Ok(())
}

fn set_autoresizing(emu: &mut Emu) -> Result<()> {
    let (v, m) = (emu.arg(0), emu.arg(2));
    view_mut(emu, v).autoresizing = m;
    Ok(())
}

fn autoresizing(emu: &mut Emu) -> Result<()> {
    let m = view(emu, emu.arg(0)).autoresizing;
    emu.ret(m);
    Ok(())
}

/// `-setContentMode:`: and, as UIKit does, the layer's contentsGravity to match.
fn set_content_mode(emu: &mut Emu) -> Result<()> {
    let (v, m) = (emu.arg(0), emu.arg(2));
    view_mut(emu, v).content_mode = m;
    // UIViewContentMode → kCAGravity (top and bottom swap: layers name them y-up).
    let gravity = match m {
        1 => "resizeAspect",
        2 => "resizeAspectFill",
        4 => "center",
        5 => "bottom",
        6 => "top",
        7 => "left",
        8 => "right",
        9 => "bottomLeft",
        10 => "bottomRight",
        11 => "topLeft",
        12 => "topRight",
        _ => "resize", // scale to fill, redraw
    };
    let layer = send(emu, v, "layer", &[])?;
    let g = foundation::new_nsstring(emu, gravity)?;
    crate::calayer::layer_mut(emu, layer).contents_gravity = g;
    Ok(())
}

fn content_mode(emu: &mut Emu) -> Result<()> {
    let m = view(emu, emu.arg(0)).content_mode;
    emu.ret(m);
    Ok(())
}

/// `-layer`: one CALayer per view, created on demand.
fn view_layer(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let mut l = view(emu, v).layer;
    if l == 0 {
        // +layerClass: views can ask for CAShapeLayer, CAGradientLayer, ...
        let vcls = emu.read_u64(v)?;
        let cls = send(emu, vcls, "layerClass", &[])?;
        l = objc::create_instance(emu, cls, 0)?;
        view_mut(emu, v).layer = l;
        emu.ui.views.layer_view.insert(l, v);
        // `[[layerClass alloc] init]`: an app's own layer class sets itself up
        // there (FlutterMetalLayer picks its device and pixel format).
        let inited = send(emu, l, "init", &[])?;
        if inited != 0 && inited != l {
            emu.ui.views.layer_view.remove(&l);
            l = inited;
            view_mut(emu, v).layer = l;
            emu.ui.views.layer_view.insert(l, v);
        }
    }
    emu.ret(l);
    Ok(())
}

/// Run `layoutSubviews` on every view in the tree that needs it, parents first.
/// Whether any view was laid out.
pub fn layout_tree(emu: &mut Emu, v: u64) -> Result<bool> {
    let mut any = false;
    // Taking the flag isn't a change on screen; what layoutSubviews moves is.
    if std::mem::take(&mut view_entry(emu, v).needs_layout) {
        any = true;
        // A view controller's view brackets its layout with the controller's callbacks.
        let vc = emu.ui.views.vc_view.iter().find(|&(_, &view)| view == v).map(|(&vc, _)| vc);
        if let Some(vc) = vc {
            send(emu, vc, "viewWillLayoutSubviews", &[])?;
        }
        crate::navigation::layout_container(emu, v)?;
        send(emu, v, "layoutSubviews", &[])?;
        if let Some(vc) = vc {
            send(emu, vc, "viewDidLayoutSubviews", &[])?;
        }
    }
    for s in view(emu, v).subviews {
        any |= layout_tree(emu, s)?;
    }
    Ok(any)
}

/// UIKit's layout pass before each frame (CoreAnimation's commit): every window.
/// Whether any view was laid out.
pub fn layout_windows(emu: &mut Emu) -> Result<bool> {
    let mut any = false;
    // Constraints and safe areas settle frames, which layoutSubviews may move
    // again (and constraints and safe areas follow): a few rounds at most.
    for _ in 0..3 {
        let mut round = crate::autolayout::solve(emu)?;
        round |= crate::autolayout::safe_area_changes(emu)?;
        for w in emu.ui.windows.clone() {
            round |= layout_tree(emu, w)?;
        }
        any |= round;
        if !round {
            break;
        }
    }
    Ok(any)
}

fn layout_if_needed(emu: &mut Emu) -> Result<()> {
    crate::autolayout::solve(emu)?;
    layout_tree(emu, emu.arg(0))?;
    Ok(())
}

fn set_needs_layout(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    // Not a change on screen until the layout moves something.
    view_entry(emu, v).needs_layout = true;
    Ok(())
}

fn size_that_fits(emu: &mut Emu) -> Result<()> {
    // Default: the size passed in (d0, d1) — UIKit returns the current size; close enough.
    let b = view(emu, emu.arg(0)).bounds;
    emu.cpu.q[0] = b[2].to_bits() as u128;
    emu.cpu.q[1] = b[3].to_bits() as u128;
    Ok(())
}

fn intrinsic_size(emu: &mut Emu) -> Result<()> {
    // UIViewNoIntrinsicMetric = -1
    emu.cpu.q[0] = (-1f64).to_bits() as u128;
    emu.cpu.q[1] = (-1f64).to_bits() as u128;
    Ok(())
}

fn trait_collection(emu: &mut Emu) -> Result<()> {
    let t = current_traits(emu)?;
    emu.ret(t);
    Ok(())
}

/// A trait collection with the given (horizontal, vertical) size classes.
pub(crate) fn new_traits(emu: &mut Emu, size_classes: (u64, u64)) -> Result<u64> {
    let cls = host_class(emu, "UITraitCollection")?;
    let t = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, t, "horizontalSizeClass", size_classes.0);
    objc::bag_set(emu, t, "verticalSizeClass", size_classes.1);
    Ok(t)
}

/// The traits as they are now (the size classes follow the orientation; the
/// style, the appearance).
pub(crate) fn current_traits(emu: &mut Emu) -> Result<u64> {
    let classes = crate::orientation::size_classes(emu);
    let t = new_traits(emu, classes)?;
    objc::bag_set(emu, t, "userInterfaceStyle", crate::appearance::style(emu));
    Ok(t)
}

/// A size class the collection was made with, else the current one.
fn size_class(emu: &mut Emu, vertical: bool) -> Result<()> {
    let key = if vertical { "verticalSizeClass" } else { "horizontalSizeClass" };
    let now = crate::orientation::size_classes(emu);
    let v = objc::bag_get(emu, emu.arg(0), key).unwrap_or(if vertical { now.1 } else { now.0 });
    emu.ret(v);
    Ok(())
}

/// `setNeedsDisplay` / `setNeedsDisplayInRect:`: redraw at the next frame.
pub(crate) fn set_needs_display(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    if emu.ui.views.views.contains_key(&v) {
        view_mut(emu, v).needs_display = true;
        crate::host::frame_changed(emu);
    }
    Ok(())
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_zero(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn ret_one(emu: &mut Emu) -> Result<()> {
    emu.ret(1);
    Ok(())
}

// ---- UIWindow ----

fn window_init_with_scene(emu: &mut Emu) -> Result<()> {
    let w = emu.arg(0);
    let (sw, sh) = crate::orientation::screen(emu);
    set_frame_of(emu, w, [0.0, 0.0, sw, sh]);
    view_mut(emu, w).window = w;
    if !emu.ui.windows.contains(&w) {
        emu.ui.windows.push(w);
    }
    Ok(())
}

fn window_init_with_frame(emu: &mut Emu) -> Result<()> {
    view_init_with_frame(emu)?;
    let w = emu.arg(0);
    view_mut(emu, w).window = w;
    if !emu.ui.windows.contains(&w) {
        emu.ui.windows.push(w);
    }
    Ok(())
}

fn make_key_and_visible(emu: &mut Emu) -> Result<()> {
    let w = emu.arg(0);
    emu.ui.key_window = w;
    view_mut(emu, w).hidden = false;
    Ok(())
}

fn make_key(emu: &mut Emu) -> Result<()> {
    emu.ui.key_window = emu.arg(0);
    Ok(())
}

fn is_key_window(emu: &mut Emu) -> Result<()> {
    emu.ret((emu.ui.key_window == emu.arg(0)) as u64);
    Ok(())
}

fn root_view_controller(emu: &mut Emu) -> Result<()> {
    let vc = emu.ui.views.root_vc.get(&emu.arg(0)).copied().unwrap_or(0);
    emu.ret(vc);
    Ok(())
}

fn set_root_view_controller(emu: &mut Emu) -> Result<()> {
    let (w, vc) = (emu.arg(0), emu.arg(2));
    emu.ui.views.root_vc.insert(w, vc);
    if vc != 0 {
        let v = send(emu, vc, "view", &[])?;
        let b = view(emu, w).bounds;
        set_frame_of(emu, v, [0.0, 0.0, b[2], b[3]]);
        insert_subview(emu, w, v, None)?;
    }
    Ok(())
}

fn window_scene(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.scene);
    Ok(())
}

fn window_screen(emu: &mut Emu) -> Result<()> {
    let s = crate::uikit::main_screen_obj(emu)?;
    emu.ret(s);
    Ok(())
}

// ---- UIViewController ----

fn vc_view(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(0);
    if let Some(&v) = emu.ui.views.vc_view.get(&vc) {
        emu.ret(v);
        return Ok(());
    }
    send(emu, vc, "loadView", &[])?;
    let v = emu.ui.views.vc_view.get(&vc).copied().unwrap_or(0);
    send(emu, vc, "viewDidLoad", &[])?;
    emu.ret(v);
    Ok(())
}

fn vc_load_view(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(0);
    let cls = host_class(emu, "UIView")?;
    let v = send(emu, cls, "alloc", &[])?;
    let d = emu.device;
    emu.cpu.q[..4].copy_from_slice(&[0, 0, d.width.to_bits() as u128, d.height.to_bits() as u128]);
    let v = send(emu, v, "initWithFrame:", &[])?;
    emu.ui.views.vc_view.insert(vc, v);
    Ok(())
}

fn vc_set_view(emu: &mut Emu) -> Result<()> {
    let (vc, v) = (emu.arg(0), emu.arg(2));
    emu.ui.views.vc_view.insert(vc, v);
    Ok(())
}

fn vc_view_if_loaded(emu: &mut Emu) -> Result<()> {
    let v = emu.ui.views.vc_view.get(&emu.arg(0)).copied().unwrap_or(0);
    emu.ret(v);
    Ok(())
}

fn vc_is_view_loaded(emu: &mut Emu) -> Result<()> {
    let l = emu.ui.views.vc_view.contains_key(&emu.arg(0));
    emu.ret(l as u64);
    Ok(())
}

// ---- UIColor ----

pub(crate) fn new_color(emu: &mut Emu, rgba: [f64; 4]) -> Result<u64> {
    let cls = host_class(emu, "UIColor")?;
    let c = objc::create_instance(emu, cls, 0)?;
    emu.ui.views.colors.insert(c, rgba);
    Ok(c)
}

fn ret_color(emu: &mut Emu, rgba: [f64; 4]) -> Result<()> {
    let c = new_color(emu, rgba)?;
    emu.ret(c);
    Ok(())
}

fn color_rgba(emu: &mut Emu) -> Result<()> {
    let rgba = [f(emu, 0), f(emu, 1), f(emu, 2), f(emu, 3)];
    ret_color(emu, rgba)
}

fn color_white(emu: &mut Emu) -> Result<()> {
    let (w, a) = (f(emu, 0), f(emu, 1));
    ret_color(emu, [w, w, w, a])
}

fn color_hsba(emu: &mut Emu) -> Result<()> {
    let (h, s, v, a) = (f(emu, 0), f(emu, 1), f(emu, 2), f(emu, 3));
    let h6 = (h.fract() + if h < 0.0 { 1.0 } else { 0.0 }) * 6.0;
    let (i, fr) = (h6.floor() as i32 % 6, h6 - h6.floor());
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * fr), v * (1.0 - s * (1.0 - fr)));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ret_color(emu, [r, g, b, a])
}

fn color_with_alpha(emu: &mut Emu) -> Result<()> {
    let c = emu.ui.views.colors.get(&emu.arg(0)).copied().unwrap_or([0.0; 4]);
    let a = f(emu, 0);
    ret_color(emu, [c[0], c[1], c[2], a])
}

/// `-getRed:green:blue:alpha:` (CGFloat out-pointers)
fn color_get_rgba(emu: &mut Emu) -> Result<()> {
    let c = emu.ui.views.colors.get(&emu.arg(0)).copied().unwrap_or([0.0; 4]);
    for (i, reg) in (2..6).enumerate() {
        let p = emu.arg(reg);
        if p != 0 {
            emu.write_u64(p, c[i].to_bits())?;
        }
    }
    emu.ret(1);
    Ok(())
}

/// `-safeAreaInsets` (UIEdgeInsets, in d0–d3): the device insets for windows
/// and views that cover the screen; zero for views inside the safe area.
pub(crate) fn safe_area_insets(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let insets = crate::autolayout::view_safe_area(emu, v);
    ret_rect(emu, insets);
    Ok(())
}

// ---- UIFont {isa, name (NSString), point size (f64 bits), weight (f64 bits)} ----
// A font named for a registered face (an app's own: coretext) is that face;
// any other is the system font, with SF Pro's proportions.

/// The face a UIFont is set in (coretext's faces; 0: the system font).
pub(crate) fn font_face(emu: &mut Emu, font: u64) -> Result<usize> {
    let name = foundation::nsstring(emu, emu.read_u64(font + 8)?)?;
    Ok(crate::coretext::face_named(emu, &name).unwrap_or(0))
}

/// An app's face's metric, in points: (ascender, descender, line gap, cap height, x-height) by `pick`.
fn face_metric(emu: &mut Emu, font: u64, pick: fn(&ttf_parser::Face) -> f64) -> Result<Option<f64>> {
    let face = font_face(emu, font)?;
    if face == 0 {
        return Ok(None);
    }
    let size = font_size(emu, font)?;
    let Some((bytes, index, _)) = crate::coretext::face_data(emu, face) else { return Ok(None) };
    let Ok(f) = ttf_parser::Face::parse(&bytes, index) else { return Ok(None) };
    Ok(Some(pick(&f) / f.units_per_em() as f64 * size))
}

/// A metric: the app's face's own, else the system font's proportion of the size.
fn font_metric_of(emu: &mut Emu, factor: f64, pick: fn(&ttf_parser::Face) -> f64) -> Result<()> {
    let font = emu.arg(0);
    let v = match face_metric(emu, font, pick)? {
        Some(v) => v,
        None => font_size(emu, font)? * factor,
    };
    ret_f64(emu, v);
    Ok(())
}

pub(crate) fn new_font(emu: &mut Emu, name: &str, size: f64, weight: f64) -> Result<u64> {
    let cls = host_class(emu, "UIFont")?;
    let font = objc::create_instance(emu, cls, 0)?;
    let n = foundation::new_nsstring(emu, name)?;
    emu.write_u64(font + 8, n)?;
    emu.write_u64(font + 16, size.to_bits())?;
    emu.write_u64(font + 24, weight.to_bits())?;
    Ok(font)
}

fn font_size(emu: &Emu, font: u64) -> Result<f64> {
    Ok(f64::from_bits(emu.read_u64(font + 16)?))
}

fn ret_font(emu: &mut Emu, name: &str, size: f64, weight: f64) -> Result<()> {
    let f = new_font(emu, name, size, weight)?;
    emu.ret(f);
    Ok(())
}

/// `preferredFontForTextStyle:`: the style's font at the default text size
/// (Large), as iOS sizes them.
fn preferred_font(emu: &mut Emu) -> Result<()> {
    let style = emu.arg(2);
    let name = if style == 0 { String::new() } else { foundation::nsstring(emu, style)? };
    // (UIFontTextStyleHeadline, or UIKit's own UICTFontTextStyleHeadline.)
    let style = name.rsplit("TextStyle").next().unwrap_or("");
    let (size, semibold) = match style {
        "ExtraLargeTitle" => (36.0, true),
        "ExtraLargeTitle2" => (28.0, true),
        "LargeTitle" | "Title0" => (34.0, false),
        "Title1" => (28.0, false),
        "Title2" => (22.0, false),
        "Title3" => (20.0, false),
        "Headline" => (17.0, true),
        "Callout" => (16.0, false),
        "Subheadline" => (15.0, false),
        "Footnote" => (13.0, false),
        "Caption1" => (12.0, false),
        "Caption2" => (11.0, false),
        _ => (17.0, false), // Body
    };
    if semibold { ret_font(emu, ".SFUI-Semibold", size, 0.3) } else { ret_font(emu, ".SFUI-Regular", size, 0.0) }
}

/// UIFontDescriptorSymbolicTraits: italic (1), bold (2).
fn font_symbolic_traits(emu: &mut Emu, font: u64) -> Result<u32> {
    if font == 0 {
        return Ok(0);
    }
    let face = font_face(emu, font)?;
    let weight = f64::from_bits(emu.read_u64(font + 24)?);
    let italic = face != 0 && crate::coretext::face_italic(emu, face);
    Ok(italic as u32 | ((weight >= 0.4) as u32) << 1)
}

/// `-[UIFontDescriptor objectForKey:]`: the name, family, size, and traits
/// (UIFontWeightTrait, UIFontSymbolicTrait).
fn font_descriptor_value(emu: &mut Emu) -> Result<()> {
    let (d, key) = (emu.arg(0), emu.arg(2));
    let font = objc::bag_get(emu, d, "font").unwrap_or(0);
    let key = foundation::nsstring(emu, key)?;
    if font == 0 {
        emu.ret(0);
        return Ok(());
    }
    let v = match key.as_str() {
        "UIFontDescriptorNameAttribute" | "NSFontNameAttribute" => emu.read_u64(font + 8)?,
        "UIFontDescriptorFamilyAttribute" | "NSFontFamilyAttribute" => objc::send(emu, font, "familyName", &[])?,
        "UIFontDescriptorSizeAttribute" | "NSFontSizeAttribute" => foundation::new_number_f64(emu, font_size(emu, font)?)?,
        "UIFontDescriptorTraitsAttribute" | "NSCTFontTraitsAttribute" => {
            let weight = f64::from_bits(emu.read_u64(font + 24)?);
            let sym = font_symbolic_traits(emu, font)?;
            let (wk, sk) = (foundation::new_nsstring(emu, "UIFontWeightTrait")?, foundation::new_nsstring(emu, "UIFontSymbolicTrait")?);
            let (wv, sv) = (foundation::new_number_f64(emu, weight)?, foundation::new_number_i64(emu, sym as i64)?);
            foundation::new_dict_of(emu, &[(wk, wv), (sk, sv)])?
        }
        _ => 0,
    };
    emu.ret(v);
    Ok(())
}

fn font_metric(emu: &mut Emu, factor: f64) -> Result<()> {
    let s = font_size(emu, emu.arg(0))?;
    ret_f64(emu, s * factor);
    Ok(())
}

/// The view's origin in window coordinates (frame offsets up the tree; no transforms yet).
pub(crate) fn window_origin(emu: &mut Emu, mut v: u64) -> [f64; 2] {
    let mut p = [0.0, 0.0];
    while v != 0 {
        let vw = view(emu, v);
        let f = frame_of(emu, v);
        let sup = vw.superview;
        if sup == 0 {
            break; // the window: its frame is the screen
        }
        let sb = view(emu, sup).bounds;
        p[0] += f[0] - sb[0];
        p[1] += f[1] - sb[1];
        v = sup;
    }
    p
}

/// `-convertPoint:toView:` / `fromView:` (point in d0, d1; view in x2; nil = window)
fn convert_point(emu: &mut Emu, to: bool) -> Result<()> {
    let (v, other, x, y) = (emu.arg(0), emu.arg(2), f(emu, 0), f(emu, 1));
    // Where each view's coordinates start in the window: its frame's origin,
    // less its bounds' (a scroll view's content offset).
    let space = |emu: &mut Emu, v: u64| {
        let o = window_origin(emu, v);
        let b = view(emu, v).bounds;
        [o[0] - b[0], o[1] - b[1]]
    };
    let a = space(emu, v);
    let b = if other == 0 { [0.0, 0.0] } else { space(emu, other) };
    let (dx, dy) = if to { (a[0] - b[0], a[1] - b[1]) } else { (b[0] - a[0], b[1] - a[1]) };
    emu.cpu.q[0] = (x + dx).to_bits() as u128;
    emu.cpu.q[1] = (y + dy).to_bits() as u128;
    Ok(())
}

// ---- UIView block animations ----
// pomme doesn't interpolate: the animations block runs now, so models take
// their end values (and presentation layers are the model layers); the
// completion block runs, finished, once the delay and duration have passed.

pub(crate) fn copy_block(emu: &mut Emu, b: u64) -> Result<u64> {
    let saved = emu.cpu.x[0];
    emu.cpu.x[0] = b;
    crate::libc::block_copy(emu)?;
    let copy = emu.cpu.x[0];
    emu.cpu.x[0] = saved;
    Ok(copy)
}

fn completion_fire(emu: &mut Emu) -> Result<()> {
    let b = emu.arg(1);
    crate::libsystem::call_block(emu, b, &[1])?;
    Ok(())
}

/// Which `+animateWith…` (or transition) form was called: where its arguments are.
#[derive(Clone, Copy)]
enum Form {
    /// `animateWithDuration:animations:[completion:]`: d0, x2, x3.
    Plain,
    /// `animateWithDuration:delay:options:animations:completion:`: d0, d1, x2, x3, x4.
    Options,
    /// `…usingSpringWithDamping:initialSpringVelocity:options:…`: d0, d1, d2, d3, x2, x3, x4.
    Spring,
    /// `animateKeyframesWithDuration:delay:options:animations:completion:`: as Options.
    Keyframes,
    /// `transitionWithView:duration:options:animations:completion:`: x2, d0, x3, x4, x5.
    Transition,
}

/// Run a UIView animation block: property changes inside it animate (see
/// animation.rs) from what's on screen to their new values, starting after
/// the delay; the completion runs (finished = YES) when they're done.
fn animate(emu: &mut Emu, form: Form, completion: bool) -> Result<()> {
    let duration = f(emu, 0).max(0.0);
    let (delay, options, block, done) = match form {
        Form::Plain => (0.0, 0, emu.arg(2), if completion { emu.arg(3) } else { 0 }),
        Form::Options | Form::Keyframes | Form::Spring => (f(emu, 1).max(0.0), emu.arg(2), emu.arg(3), emu.arg(4)),
        Form::Transition => (0.0, emu.arg(3), emu.arg(4), emu.arg(5)),
    };
    let timing = match form {
        Form::Spring => crate::animation::view_spring(duration, f(emu, 2), f(emu, 3)),
        // Keyframes run linearly between their keyframes (calculation mode linear).
        Form::Keyframes => crate::animation::Timing::Linear,
        _ => crate::animation::block_timing(options),
    };
    if block != 0 {
        crate::animation::in_block(emu, delay, duration, timing, options, |e| crate::libsystem::call_block(e, block, &[]).map(|_| ()))?;
    }
    // A repeating animation never finishes.
    if done != 0 && options & (1 << 3) == 0 {
        let c = copy_block(emu, done)?;
        let reverses = if options & (1 << 4) != 0 { 2.0 } else { 1.0 };
        crate::animation::after_commit(emu, delay + duration * reverses, completion_fire, c)?;
    }
    Ok(())
}

/// `-convertPoint/Rect:to/fromCoordinateSpace:`: a view, or the screen (whose
/// space is the window's: pomme's windows fill the screen at the origin).
fn convert_space(emu: &mut Emu, rect: bool, to: bool) -> Result<()> {
    let space = emu.arg(2);
    let is_view = {
        let cls = host_class(emu, "UIView")?;
        space != 0 && crate::objc::is_kind_of(emu, space, cls)?
    };
    if !is_view {
        emu.cpu.x[2] = 0;
    }
    if rect { convert_rect(emu, to) } else { convert_point(emu, to) }
}

/// `-convertRect:toView:` / `fromView:` (rect in d0–d3)
fn convert_rect(emu: &mut Emu, to: bool) -> Result<()> {
    let (w, h) = (f(emu, 2), f(emu, 3));
    convert_point(emu, to)?;
    emu.cpu.q[2] = w.to_bits() as u128;
    emu.cpu.q[3] = h.to_bits() as u128;
    Ok(())
}

fn is_descendant(emu: &mut Emu) -> Result<()> {
    let (mut v, other) = (emu.arg(0), emu.arg(2));
    while v != 0 && v != other {
        v = view(emu, v).superview;
    }
    emu.ret((v != 0) as u64);
    Ok(())
}

fn view_with_tag(emu: &mut Emu) -> Result<()> {
    fn find(emu: &mut Emu, v: u64, tag: u64) -> u64 {
        if view(emu, v).tag == tag {
            return v;
        }
        for s in view(emu, v).subviews {
            let r = find(emu, s, tag);
            if r != 0 {
                return r;
            }
        }
        0
    }
    let r = find(emu, emu.arg(0), emu.arg(2));
    emu.ret(r);
    Ok(())
}

/// `-sizeToFit`: keeps the current size until there's real text and image measurement.
/// `-sizeToFit`: the size `sizeThatFits:` asks for, keeping the frame's origin.
fn size_to_fit(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let f = frame_of(emu, v);
    let size = objc::send_fp(emu, v, "sizeThatFits:", &[], &[f[2], f[3]])?;
    objc::send_fp(emu, v, "setFrame:", &[], &[f[0], f[1], size[0], size[1]])?;
    Ok(())
}

fn point_inside(emu: &mut Emu) -> Result<()> {
    let (v, x, y) = (emu.arg(0), f(emu, 0), f(emu, 1));
    let b = view(emu, v).bounds;
    let inside = x >= b[0] && y >= b[1] && x < b[0] + b[2] && y < b[1] + b[3];
    emu.ret(inside as u64);
    Ok(())
}

fn content_scale(emu: &mut Emu) -> Result<()> {
    ret_f64(emu, emu.device.scale);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIView", false, "convertPoint:toView:", |e| convert_point(e, true)),
    ("UIView", false, "convertPoint:fromView:", |e| convert_point(e, false)),
    ("UIView", false, "convertRect:toView:", |e| convert_rect(e, true)),
    ("UIView", false, "convertRect:fromView:", |e| convert_rect(e, false)),
    ("UIView", true, "animateWithDuration:animations:", |e| animate(e, Form::Plain, false)),
    ("UIView", true, "animateWithDuration:animations:completion:", |e| animate(e, Form::Plain, true)),
    ("UIView", true, "animateWithDuration:delay:options:animations:completion:", |e| animate(e, Form::Options, true)),
    ("UIView", true, "animateWithDuration:delay:usingSpringWithDamping:initialSpringVelocity:options:animations:completion:", |e| animate(e, Form::Spring, true)),
    ("UIView", true, "animateKeyframesWithDuration:delay:options:animations:completion:", |e| animate(e, Form::Keyframes, true)),
    ("UIView", true, "transitionWithView:duration:options:animations:completion:", |e| animate(e, Form::Transition, true)),
    // Within an animateKeyframes block: this part runs over its share of the whole (d0 start, d1 length).
    ("UIView", true, "addKeyframeWithRelativeStartTime:relativeDuration:animations:", |e| {
        let (b, start, length) = (e.arg(2), f(e, 0), f(e, 1));
        crate::animation::in_keyframe(e, start, length, |e| crate::libsystem::call_block(e, b, &[]).map(|_| ()))
    }),
    ("UIView", true, "performWithoutAnimation:", |e| {
        let b = e.arg(2);
        crate::animation::without(e, |e| crate::libsystem::call_block(e, b, &[]).map(|_| ()))
    }),
    ("UIView", false, "convertRect:toCoordinateSpace:", |e| convert_space(e, true, true)),
    ("UIView", false, "convertRect:fromCoordinateSpace:", |e| convert_space(e, true, false)),
    ("UIView", false, "convertPoint:toCoordinateSpace:", |e| convert_space(e, false, true)),
    ("UIView", false, "convertPoint:fromCoordinateSpace:", |e| convert_space(e, false, false)),
    ("UIView", false, "isDescendantOfView:", is_descendant),
    ("UIView", false, "viewWithTag:", view_with_tag),
    ("UIView", false, "sizeToFit", size_to_fit),
    ("UIView", false, "pointInside:withEvent:", point_inside),
    ("UIView", false, "contentScaleFactor", content_scale),
    ("UIView", false, "setContentScaleFactor:", noop),
    ("UIView", false, "updateConstraintsIfNeeded", noop),
    ("UIView", false, "setSemanticContentAttribute:", noop),
    ("UIView", false, "semanticContentAttribute", ret_zero),
    ("UIView", false, "setInsetsLayoutMarginsFromSafeArea:", noop),
    ("UIView", false, "setPreservesSuperviewLayoutMargins:", noop),
    ("UIView", false, "setAccessibilityElementsHidden:", noop),
    ("UIView", false, "setAccessibilityTraits:", noop),
    ("UIView", false, "setAccessibilityViewIsModal:", noop),
    ("UIView", false, "setAccessibilityIgnoresInvertColors:", noop),
    ("UIView", false, "setShouldGroupAccessibilityChildren:", noop),
    ("UIView", false, "tintColor", ret_zero),
    ("UIView", false, "isExclusiveTouch", ret_zero),
    ("UIView", false, "isMultipleTouchEnabled", ret_zero),
    ("UIView", false, "layoutSubviewsIfNeeded", noop),
    ("UIFont", true, "systemFontOfSize:", |e| {
        let s = f(e, 0);
        ret_font(e, ".SFUI-Regular", s, 0.0)
    }),
    ("UIFont", true, "systemFontOfSize:weight:", |e| {
        let (s, w) = (f(e, 0), f(e, 1));
        ret_font(e, ".SFUI-Regular", s, w)
    }),
    ("UIFont", true, "boldSystemFontOfSize:", |e| {
        let s = f(e, 0);
        ret_font(e, ".SFUI-Bold", s, 0.4)
    }),
    ("UIFont", true, "italicSystemFontOfSize:", |e| {
        let s = f(e, 0);
        ret_font(e, ".SFUI-Italic", s, 0.0)
    }),
    ("UIFont", true, "monospacedDigitSystemFontOfSize:weight:", |e| {
        let (s, w) = (f(e, 0), f(e, 1));
        ret_font(e, ".SFUI-Regular", s, w)
    }),
    // An app's registered face, by its PostScript, full or family name, at its
    // weight; any other name, the system font under that name.
    ("UIFont", true, "fontWithName:size:", |e| {
        let name = foundation::nsstring(e, e.arg(2))?;
        let s = f(e, 0);
        match crate::coretext::face_named(e, &name).filter(|&i| i != 0) {
            Some(i) => {
                let (ps, w) = crate::coretext::face_names(e, i).map(|n| (n.1, n.2)).unwrap_or((name, 0.0));
                ret_font(e, &ps, s, w)
            }
            None => ret_font(e, &name, s, 0.0),
        }
    }),
    ("UIFont", true, "preferredFontForTextStyle:", preferred_font),
    // The registered families (the app's fonts, and the system's that pomme has).
    ("UIFont", true, "familyNames", |e| {
        let names: Vec<String> = crate::coretext::families(e);
        let mut v = Vec::new();
        for n in names {
            v.push(foundation::new_nsstring(e, &n)?);
        }
        let a = foundation::new_array_of(e, &v)?;
        e.ret(a);
        Ok(())
    }),
    // A registered family's fonts, by PostScript name.
    ("UIFont", true, "fontNamesForFamilyName:", |e| {
        let family = foundation::nsstring(e, e.arg(2))?;
        let names = crate::coretext::family_members(e, &family);
        let mut v = Vec::new();
        for n in names {
            v.push(foundation::new_nsstring(e, &n)?);
        }
        let a = foundation::new_array_of(e, &v)?;
        e.ret(a);
        Ok(())
    }),
    ("UIFont", false, "fontWithSize:", |e| {
        let (font, s) = (e.arg(0), f(e, 0));
        let name = foundation::nsstring(e, e.read_u64(font + 8)?)?;
        let w = f64::from_bits(e.read_u64(font + 24)?);
        ret_font(e, &name, s, w)
    }),
    ("UIFont", false, "fontName", |e| {
        let n = e.read_u64(e.arg(0) + 8)?;
        e.ret(n);
        Ok(())
    }),
    ("UIFont", false, "familyName", |e| {
        let face = font_face(e, e.arg(0))?;
        let family = match crate::coretext::face_names(e, face).filter(|_| face != 0) {
            Some((family, _, _)) => family,
            None => ".SF UI".to_string(),
        };
        let s = foundation::new_nsstring(e, &family)?;
        e.ret(s);
        Ok(())
    }),
    ("UIFont", false, "pointSize", |e| font_metric(e, 1.0)),
    ("UIFont", false, "lineHeight", |e| font_metric_of(e, 1.193, |f| (f.ascender() - f.descender() + f.line_gap()) as f64)),
    ("UIFont", false, "ascender", |e| font_metric_of(e, 0.952, |f| f.ascender() as f64)),
    ("UIFont", false, "descender", |e| font_metric_of(e, -0.241, |f| f.descender() as f64)),
    ("UIFont", false, "capHeight", |e| font_metric_of(e, 0.705, |f| f.capital_height().unwrap_or(0) as f64)),
    ("UIFont", false, "xHeight", |e| font_metric_of(e, 0.529, |f| f.x_height().unwrap_or(0) as f64)),
    ("UIFont", false, "leading", |e| font_metric_of(e, 0.0, |f| f.line_gap() as f64)),
    // Its descriptor: what it is (name, family, size) and its traits.
    ("UIFont", false, "fontDescriptor", |e| {
        let cls = host_class(e, "UIFontDescriptor")?;
        let d = objc::create_instance(e, cls, 0)?;
        objc::bag_set(e, d, "font", e.arg(0));
        e.ret(d);
        Ok(())
    }),
    ("UIFontDescriptor", false, "objectForKey:", font_descriptor_value),
    ("UIFontDescriptor", false, "symbolicTraits", |e| {
        let font = objc::bag_get(e, e.arg(0), "font").unwrap_or(0);
        let t = font_symbolic_traits(e, font)?;
        e.ret(t as u64);
        Ok(())
    }),
    ("UIFontDescriptor", false, "postscriptName", |e| {
        let font = objc::bag_get(e, e.arg(0), "font").unwrap_or(0);
        let n = if font == 0 { 0 } else { e.read_u64(font + 8)? };
        e.ret(n);
        Ok(())
    }),
    ("UIFontDescriptor", false, "pointSize", |e| {
        let font = objc::bag_get(e, e.arg(0), "font").unwrap_or(0);
        let s = if font == 0 { 0.0 } else { font_size(e, font)? };
        ret_f64(e, s);
        Ok(())
    }),
    ("UIView", false, "safeAreaInsets", safe_area_insets),
    ("UIView", false, "safeAreaInsetsDidChange", noop),
    ("UIView", false, "init", view_init),
    ("UIView", false, "initWithFrame:", view_init_with_frame),
    ("UIView", false, "initWithCoder:", |e| {
        let v = e.arg(0);
        view_mut(e, v);
        Ok(())
    }),
    ("UIView", false, "frame", view_frame),
    ("UIView", false, "setFrame:", view_set_frame),
    ("UIView", false, "bounds", view_bounds),
    ("UIView", false, "setBounds:", view_set_bounds),
    ("UIView", false, "center", view_center),
    ("UIView", false, "setCenter:", view_set_center),
    ("UIView", false, "addSubview:", add_subview),
    ("UIView", false, "insertSubview:atIndex:", insert_subview_at),
    ("UIView", false, "insertSubview:aboveSubview:", |e| insert_subview_relative(e, true)),
    ("UIView", false, "insertSubview:belowSubview:", |e| insert_subview_relative(e, false)),
    ("UIView", false, "removeFromSuperview", remove_from_superview),
    ("UIView", false, "bringSubviewToFront:", bring_to_front),
    ("UIView", false, "subviews", subviews),
    ("UIView", false, "superview", superview),
    ("UIView", false, "window", view_window),
    ("UIView", false, "backgroundColor", background_color),
    ("UIView", false, "setBackgroundColor:", set_background_color),
    ("UIView", false, "isHidden", is_hidden),
    ("UIView", false, "setHidden:", set_hidden),
    ("UIView", false, "alpha", alpha),
    ("UIView", false, "setAlpha:", set_alpha),
    ("UIView", false, "tag", tag),
    ("UIView", false, "setTag:", set_tag),
    ("UIView", false, "isUserInteractionEnabled", user_interaction),
    ("UIView", false, "setUserInteractionEnabled:", set_user_interaction),
    ("UIView", false, "clipsToBounds", clips),
    ("UIView", false, "setClipsToBounds:", set_clips),
    ("UIView", false, "autoresizingMask", autoresizing),
    ("UIView", false, "setAutoresizingMask:", set_autoresizing),
    ("UIView", false, "contentMode", content_mode),
    ("UIView", false, "setContentMode:", set_content_mode),
    ("UIView", false, "layer", view_layer),
    ("UIView", true, "layerClass", |e| {
        let c = host_class(e, "CALayer")?;
        e.ret(c);
        Ok(())
    }),
    ("UIView", false, "setNeedsLayout", set_needs_layout),
    ("UIView", false, "setNeedsDisplay", set_needs_display),
    ("UIView", false, "setNeedsDisplayInRect:", set_needs_display),
    ("UIView", false, "layoutSubviews", noop),
    ("UIView", false, "layoutIfNeeded", layout_if_needed),
    ("UIView", false, "didMoveToSuperview", noop),
    ("UIView", false, "didMoveToWindow", noop),
    ("UIView", false, "didAddSubview:", noop),
    ("UIView", false, "willMoveToSuperview:", noop),
    ("UIView", false, "willMoveToWindow:", noop),
    ("UIView", false, "sizeThatFits:", size_that_fits),
    ("UIView", false, "intrinsicContentSize", intrinsic_size),
    ("UIView", false, "traitCollection", trait_collection),
    ("UIView", false, "setTranslatesAutoresizingMaskIntoConstraints:", noop),
    ("UIView", false, "setAccessibilityIdentifier:", noop),
    ("UIView", false, "setAccessibilityLabel:", noop),
    ("UIView", false, "setIsAccessibilityElement:", noop),
    ("UIView", false, "setOpaque:", noop),
    ("UIView", false, "isOpaque", ret_zero),
    ("UIView", false, "setMultipleTouchEnabled:", noop),
    ("UIView", false, "setExclusiveTouch:", noop),
    ("UIView", false, "setTintColor:", noop),
    ("UIView", true, "areAnimationsEnabled", ret_one),
    ("UIView", true, "setAnimationsEnabled:", noop),
    ("UIView", true, "userInterfaceLayoutDirectionForSemanticContentAttribute:", ret_zero),
    ("UIWindow", false, "initWithWindowScene:", window_init_with_scene),
    ("UIWindow", false, "initWithFrame:", window_init_with_frame),
    ("UIWindow", false, "makeKeyAndVisible", make_key_and_visible),
    ("UIWindow", false, "makeKeyWindow", make_key),
    ("UIWindow", false, "isKeyWindow", is_key_window),
    ("UIWindow", false, "rootViewController", root_view_controller),
    ("UIWindow", false, "setRootViewController:", set_root_view_controller),
    ("UIWindow", false, "windowScene", window_scene),
    ("UIWindow", false, "setWindowScene:", noop),
    ("UIWindow", false, "screen", window_screen),
    ("UIWindow", false, "windowLevel", |e| {
        ret_f64(e, 0.0);
        Ok(())
    }),
    // `init` goes through the designated initializer, as UIKit's does: Swift
    // subclasses set up their stored properties in initWithNibName:bundle:.
    ("UIViewController", false, "init", |e| {
        let vc = e.arg(0);
        let r = send(e, vc, "initWithNibName:bundle:", &[0, 0])?;
        e.ret(r);
        Ok(())
    }),
    ("UIViewController", false, "initWithNibName:bundle:", |_| Ok(())),
    ("UIViewController", false, "view", vc_view),
    ("UIViewController", false, "setView:", vc_set_view),
    ("UIViewController", false, "loadView", vc_load_view),
    ("UIViewController", false, "viewIfLoaded", vc_view_if_loaded),
    ("UIViewController", false, "isViewLoaded", vc_is_view_loaded),
    ("UIViewController", false, "viewDidLoad", noop),
    ("UIViewController", false, "prefersStatusBarHidden", ret_zero),
    ("UIViewController", false, "preferredStatusBarStyle", ret_zero),
    // The status bar asks the controllers again when it's next drawn.
    ("UIViewController", false, "setNeedsStatusBarAppearanceUpdate", |e| {
        crate::host::frame_changed(e);
        Ok(())
    }),
    ("UIViewController", false, "childViewControllerForStatusBarStyle", ret_zero),
    ("UIViewController", false, "childViewControllerForStatusBarHidden", ret_zero),
    ("UIViewController", false, "viewWillAppear:", noop),
    ("UIViewController", false, "viewDidAppear:", noop),
    ("UIViewController", false, "viewWillLayoutSubviews", noop),
    ("UIViewController", false, "viewDidLayoutSubviews", noop),
    ("UIViewController", false, "traitCollection", trait_collection),
    ("UIColor", true, "colorWithRed:green:blue:alpha:", color_rgba),
    ("UIColor", false, "initWithRed:green:blue:alpha:", color_rgba),
    ("UIColor", true, "colorWithWhite:alpha:", color_white),
    ("UIColor", true, "whiteColor", |e| ret_color(e, [1.0, 1.0, 1.0, 1.0])),
    ("UIColor", true, "blackColor", |e| ret_color(e, [0.0, 0.0, 0.0, 1.0])),
    ("UIColor", true, "clearColor", |e| ret_color(e, [0.0, 0.0, 0.0, 0.0])),
    ("UIColor", true, "grayColor", |e| ret_color(e, [0.5, 0.5, 0.5, 1.0])),
    ("UIColor", true, "redColor", |e| ret_color(e, [1.0, 0.0, 0.0, 1.0])),
    ("UIColor", true, "colorWithCGColor:", |e| {
        let c = e.arg(2);
        e.ret(c);
        Ok(())
    }),
    ("UIColor", false, "initWithCGColor:", |e| {
        let c = e.arg(2);
        e.ret(c);
        Ok(())
    }),
    ("UIColor", true, "colorWithHue:saturation:brightness:alpha:", color_hsba),
    ("UIColor", false, "initWithWhite:alpha:", color_white),
    ("UIColor", true, "colorNamed:", ret_zero),
    ("UIColor", true, "darkGrayColor", |e| ret_color(e, [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 1.0])),
    ("UIColor", true, "lightGrayColor", |e| ret_color(e, [2.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 1.0])),
    ("UIColor", true, "blueColor", |e| ret_color(e, [0.0, 0.0, 1.0, 1.0])),
    ("UIColor", true, "greenColor", |e| ret_color(e, [0.0, 1.0, 0.0, 1.0])),
    // UIKit's fixed named colours (sRGB, not the dynamic system ones).
    ("UIColor", true, "yellowColor", |e| ret_color(e, [1.0, 1.0, 0.0, 1.0])),
    ("UIColor", true, "cyanColor", |e| ret_color(e, [0.0, 1.0, 1.0, 1.0])),
    ("UIColor", true, "magentaColor", |e| ret_color(e, [1.0, 0.0, 1.0, 1.0])),
    ("UIColor", true, "orangeColor", |e| ret_color(e, [1.0, 0.5, 0.0, 1.0])),
    ("UIColor", true, "purpleColor", |e| ret_color(e, [0.5, 0.0, 0.5, 1.0])),
    ("UIColor", true, "brownColor", |e| ret_color(e, [0.6, 0.4, 0.2, 1.0])),
    ("UIColor", false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let ca = e.ui.views.colors.get(&a).copied();
        let eq = a == b || (ca.is_some() && ca == e.ui.views.colors.get(&b).copied());
        e.ret(eq as u64);
        Ok(())
    }),
    ("UIColor", false, "alphaComponent", |e| {
        let a = e.ui.views.colors.get(&e.arg(0)).map_or(0.0, |c| c[3]);
        ret_f64(e, a);
        Ok(())
    }),
    ("UIColor", false, "colorWithAlphaComponent:", color_with_alpha),
    ("UIColor", false, "getRed:green:blue:alpha:", color_get_rgba),
    ("UITraitCollection", true, "currentTraitCollection", trait_collection),
    ("UITraitCollection", true, "setCurrentTraitCollection:", noop),
    ("UITraitCollection", false, "displayScale", |e| {
        ret_f64(e, e.device.scale);
        Ok(())
    }),
    ("UITraitCollection", false, "horizontalSizeClass", |e| size_class(e, false)),
    ("UITraitCollection", false, "verticalSizeClass", |e| size_class(e, true)),
    ("UITraitCollection", false, "preferredContentSizeCategory", |e| {
        // Same value pomme gives the UIContentSizeCategoryLarge constant (see constants.rs).
        let s = foundation::new_nsstring(e, "UIContentSizeCategoryLarge")?;
        e.ret(s);
        Ok(())
    }),
    ("UITraitCollection", false, "layoutDirection", ret_zero),
];

// ---- inspection ----

/// The window hierarchy as indented text: class, frame, colour, text.
/// The system blue: #007AFF, and #0088FF in the iOS 26 design (measured on its simulator).

pub fn describe_tree(emu: &mut Emu) -> Result<String> {
    fn rgba_hex(emu: &Emu, c: u64) -> Option<String> {
        let [r, g, b, a] = *emu.ui.views.colors.get(&c)?;
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some(format!("#{:02x}{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b), byte(a)))
    }
    fn text_of(emu: &mut Emu, v: u64) -> Result<Option<String>> {
        for sel in ["attributedText", "text"] {
            let s = objc::intern(emu, sel)?;
            if !objc::responds(emu, v, s)? {
                continue;
            }
            let t = send(emu, v, sel, &[])?;
            if t == 0 {
                continue;
            }
            let t = if sel == "attributedText" { send(emu, t, "string", &[])? } else { t };
            if t != 0 {
                return Ok(Some(foundation::nsstring(emu, t)?));
            }
        }
        Ok(None)
    }
    fn walk(emu: &mut Emu, v: u64, depth: usize, out: &mut String) -> Result<()> {
        let vw = view(emu, v);
        let f = frame_of(emu, v);
        let cls = emu.read_u64(v).map(|isa| emu.objc.class_name(isa)).unwrap_or_default();
        let mut line = format!("{}{cls} ({:.1}, {:.1}, {:.1}×{:.1})", "  ".repeat(depth), f[0], f[1], f[2], f[3]);
        if let Some(c) = rgba_hex(emu, vw.background) {
            line += &format!(" bg {c}");
        }
        if vw.hidden {
            line += " hidden";
        }
        if vw.tag != 0 {
            line += &format!(" tag {}", vw.tag);
        }
        if vw.alpha < 1.0 {
            line += &format!(" alpha {:.2}", vw.alpha);
        }
        if vw.layer != 0 {
            let l = crate::calayer::layer(emu, vw.layer);
            if l.corner_radius > 0.0 {
                line += &format!(" radius {}", l.corner_radius);
            }
            if l.border_width > 0.0 {
                line += &format!(" border {}{}", l.border_width, rgba_hex(emu, l.border_color).map(|c| format!(" {c}")).unwrap_or_default());
            }
        }
        // A scroll view moved off its origin: where its content is.
        if crate::scrollview::is_scroll_view(emu, v) && (vw.bounds[0] != 0.0 || vw.bounds[1] != 0.0) {
            line += &format!(" offset ({:.1}, {:.1})", vw.bounds[0], vw.bounds[1]);
        }
        if let Some(t) = text_of(emu, v)? {
            line += &format!(" {t:?}");
        }
        // A value control's value.
        let (switch, slider) = (objc::host_class(emu, "UISwitch")?, objc::host_class(emu, "UISlider")?);
        if objc::is_kind_of(emu, v, switch)? {
            line += if objc::send(emu, v, "isOn", &[])? & 0xff != 0 { " on" } else { " off" };
        } else if objc::is_kind_of(emu, v, slider)? {
            line += &format!(" value {:.2}", crate::valuecontrols::slider_values(emu, v).0);
        }
        out.push_str(&line);
        out.push('\n');
        for s in vw.subviews {
            walk(emu, s, depth + 1, out)?;
        }
        Ok(())
    }
    let mut out = String::new();
    for w in emu.ui.windows.clone() {
        walk(emu, w, 0, &mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoresizing_shares_the_change() {
        // Flexible width: all of it.
        assert_eq!(autoresize_axis(0.0, 0.0, 0.0, 390.0, [false, true, false]), (0.0, 390.0));
        // Flexible margins around a fixed size: it stays centered.
        assert_eq!(autoresize_axis(50.0, 100.0, 200.0, 100.0, [true, false, true]), (100.0, 100.0));
        // Fixed: nothing moves.
        assert_eq!(autoresize_axis(10.0, 20.0, 100.0, 50.0, [false; 3]), (10.0, 20.0));
    }
}
