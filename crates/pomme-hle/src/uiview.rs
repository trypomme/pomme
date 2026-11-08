//! UIView, UIWindow, UIViewController, UIColor, CALayer.
//!
//! View state is kept host-side, keyed by object address, not in the object:
//! apps subclass UIView (React Native's component views do) with ivars laid
//! out after the real UIView's, so pomme must not use the object's memory.
//!
//! Geometry follows the ABI: CGRect/CGPoint/CGSize are homogeneous float
//! aggregates, passed and returned in d0–d3.

use crate::objc::{self, host_class, send};
use crate::uikit::{SCREEN_H, SCREEN_W, ret_f64, ret_rect};
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

fn view_mut(emu: &mut Emu, v: u64) -> &mut View {
    emu.ui.views.views.entry(v).or_insert_with(|| View { alpha: 1.0, user_interaction: true, ..Default::default() })
}

fn view(emu: &mut Emu, v: u64) -> View {
    view_mut(emu, v).clone()
}

fn set_frame_of(emu: &mut Emu, v: u64, r: [f64; 4]) {
    let vw = view_mut(emu, v);
    vw.bounds[2] = r[2];
    vw.bounds[3] = r[3];
    vw.center = [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
}

fn frame_of(emu: &mut Emu, v: u64) -> [f64; 4] {
    let vw = view(emu, v);
    [vw.center[0] - vw.bounds[2] / 2.0, vw.center[1] - vw.bounds[3] / 2.0, vw.bounds[2], vw.bounds[3]]
}

// ---- UIView ----

fn view_init_with_frame(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), rect_arg(emu));
    set_frame_of(emu, v, r);
    Ok(()) // returns self
}

fn view_init(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    view_mut(emu, v);
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
    view_mut(emu, v).bounds = r;
    Ok(())
}

fn view_center(emu: &mut Emu) -> Result<()> {
    let c = view(emu, emu.arg(0)).center;
    emu.cpu.q[0] = c[0].to_bits() as u128;
    emu.cpu.q[1] = c[1].to_bits() as u128;
    Ok(())
}

fn view_set_center(emu: &mut Emu) -> Result<()> {
    let (v, x, y) = (emu.arg(0), f(emu, 0), f(emu, 1));
    view_mut(emu, v).center = [x, y];
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

fn insert_subview(emu: &mut Emu, parent: u64, child: u64, index: Option<usize>) -> Result<()> {
    if child == 0 || child == parent {
        return Ok(());
    }
    detach(emu, child);
    let p = view_mut(emu, parent);
    let i = index.unwrap_or(p.subviews.len()).min(p.subviews.len());
    p.subviews.insert(i, child);
    view_mut(emu, child).superview = parent;
    let w = view(emu, parent).window;
    set_window_recursive(emu, child, w);
    for (sel, arg) in [("didMoveToSuperview", None), ("didMoveToWindow", None), ("didAddSubview:", Some(child))] {
        let target = if arg.is_some() { parent } else { child };
        let s = objc::intern(emu, sel)?;
        if objc::responds(emu, target, s)? {
            send(emu, target, sel, &arg.into_iter().collect::<Vec<_>>())?;
        }
    }
    Ok(())
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
    detach(emu, v);
    set_window_recursive(emu, v, 0);
    Ok(())
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
    view_mut(emu, v).background = c;
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
    view_mut(emu, v).alpha = a;
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

fn set_content_mode(emu: &mut Emu) -> Result<()> {
    let (v, m) = (emu.arg(0), emu.arg(2));
    view_mut(emu, v).content_mode = m;
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
        let cls = host_class(emu, "CALayer")?;
        l = objc::create_instance(emu, cls, 0)?;
        view_mut(emu, v).layer = l;
        emu.ui.views.layer_view.insert(l, v);
    }
    emu.ret(l);
    Ok(())
}

/// `-layoutIfNeeded`: run `layoutSubviews` on this view and its subviews.
fn layout_if_needed(emu: &mut Emu) -> Result<()> {
    fn layout(emu: &mut Emu, v: u64) -> Result<()> {
        send(emu, v, "layoutSubviews", &[])?;
        for s in view(emu, v).subviews {
            layout(emu, s)?;
        }
        Ok(())
    }
    layout(emu, emu.arg(0))
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
    let cls = host_class(emu, "UITraitCollection")?;
    let t = objc::create_instance(emu, cls, 0)?;
    emu.ret(t);
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
    set_frame_of(emu, w, [0.0, 0.0, SCREEN_W, SCREEN_H]);
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
    emu.cpu.q[..4].copy_from_slice(&[0, 0, SCREEN_W.to_bits() as u128, SCREEN_H.to_bits() as u128]);
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

fn new_color(emu: &mut Emu, rgba: [f64; 4]) -> Result<u64> {
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

fn color_cg(_: &mut Emu) -> Result<()> {
    Ok(()) // the UIColor stands in for its CGColor
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

// ---- CALayer (state only, no compositing yet) ----

fn layer_delegate(emu: &mut Emu) -> Result<()> {
    let v = emu.ui.views.layer_view.get(&emu.arg(0)).copied().unwrap_or(0);
    emu.ret(v);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIView", false, "init", view_init),
    ("UIView", false, "initWithFrame:", view_init_with_frame),
    ("UIView", false, "initWithCoder:", view_init),
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
    ("UIView", false, "setNeedsLayout", noop),
    ("UIView", false, "setNeedsDisplay", noop),
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
    ("UIView", false, "addGestureRecognizer:", noop),
    ("UIView", false, "removeGestureRecognizer:", noop),
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
    ("UIViewController", false, "init", |_| Ok(())),
    ("UIViewController", false, "initWithNibName:bundle:", |_| Ok(())),
    ("UIViewController", false, "view", vc_view),
    ("UIViewController", false, "setView:", vc_set_view),
    ("UIViewController", false, "loadView", vc_load_view),
    ("UIViewController", false, "viewIfLoaded", vc_view_if_loaded),
    ("UIViewController", false, "isViewLoaded", vc_is_view_loaded),
    ("UIViewController", false, "viewDidLoad", noop),
    ("UIViewController", false, "viewWillAppear:", noop),
    ("UIViewController", false, "viewDidAppear:", noop),
    ("UIViewController", false, "viewWillLayoutSubviews", noop),
    ("UIViewController", false, "viewDidLayoutSubviews", noop),
    ("UIViewController", false, "setModalPresentationStyle:", noop),
    ("UIViewController", false, "presentedViewController", ret_zero),
    ("UIViewController", false, "presentingViewController", ret_zero),
    ("UIViewController", false, "parentViewController", ret_zero),
    ("UIViewController", false, "childViewControllers", |e| {
        let a = foundation::new_array_of(e, &[])?;
        e.ret(a);
        Ok(())
    }),
    ("UIViewController", false, "traitCollection", trait_collection),
    ("UIColor", true, "colorWithRed:green:blue:alpha:", color_rgba),
    ("UIColor", false, "initWithRed:green:blue:alpha:", color_rgba),
    ("UIColor", true, "colorWithWhite:alpha:", color_white),
    ("UIColor", true, "whiteColor", |e| ret_color(e, [1.0, 1.0, 1.0, 1.0])),
    ("UIColor", true, "blackColor", |e| ret_color(e, [0.0, 0.0, 0.0, 1.0])),
    ("UIColor", true, "clearColor", |e| ret_color(e, [0.0, 0.0, 0.0, 0.0])),
    ("UIColor", true, "systemBackgroundColor", |e| ret_color(e, [1.0, 1.0, 1.0, 1.0])),
    ("UIColor", true, "labelColor", |e| ret_color(e, [0.0, 0.0, 0.0, 1.0])),
    ("UIColor", true, "grayColor", |e| ret_color(e, [0.5, 0.5, 0.5, 1.0])),
    ("UIColor", true, "redColor", |e| ret_color(e, [1.0, 0.0, 0.0, 1.0])),
    ("UIColor", false, "CGColor", color_cg),
    ("UIColor", false, "colorWithAlphaComponent:", color_with_alpha),
    ("UIColor", false, "getRed:green:blue:alpha:", color_get_rgba),
    ("CALayer", false, "delegate", layer_delegate),
    ("CALayer", false, "setNeedsDisplay", noop),
    ("CALayer", false, "setNeedsLayout", noop),
    ("UITraitCollection", false, "userInterfaceStyle", ret_one), // light
    ("UITraitCollection", false, "displayScale", |e| {
        ret_f64(e, crate::uikit::SCREEN_SCALE);
        Ok(())
    }),
    ("UITraitCollection", false, "horizontalSizeClass", ret_one), // compact
    ("UITraitCollection", false, "verticalSizeClass", |e| {
        e.ret(2); // regular
        Ok(())
    }),
    ("UITraitCollection", false, "preferredContentSizeCategory", |e| {
        let s = foundation::new_nsstring(e, "UICTContentSizeCategoryL")?;
        e.ret(s);
        Ok(())
    }),
    ("UITraitCollection", false, "layoutDirection", ret_zero),
];
