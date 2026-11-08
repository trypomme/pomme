//! View-controller containment, UINavigationController and UINavigationItem:
//! what react-native-screens (and so Expo Router / React Navigation's native
//! stack) builds on.
//!
//! A navigation controller's view is a container holding the top view
//! controller's view and the navigation bar. Pushes and pops are instant (no
//! slide yet), but they run the same sequence UIKit does: containment
//! (`willMove`/`didMove`), appearance callbacks (`viewWillAppear:` …
//! `viewDidDisappear:`) and the delegate's `willShow`/`didShow`, which is
//! what the screens library turns into React Navigation's events.

use crate::objc::{self, host_class, send};
use crate::uikit::{SCREEN_H, SCREEN_W};
use crate::uiview::{SAFE_AREA, set_frame_of, view, view_mut};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::HashMap;

/// The navigation bar's height (below the status bar).
pub const BAR_HEIGHT: f64 = 44.0;

#[derive(Clone, Default)]
struct Nav {
    stack: Vec<u64>,
    bar: u64,
    container: u64,
    delegate: u64,
    bar_hidden: bool,
    /// Reported back to the app; layout follows the top controller's edgesForExtendedLayout.
    translucent: bool,
    pop_gesture: u64,
}

#[derive(Clone, Default)]
struct Item {
    title: u64,
    back_title: u64,
    title_view: u64,
    hides_back: bool,
}

#[derive(Default)]
pub struct State {
    parent: HashMap<u64, u64>,
    children: HashMap<u64, Vec<u64>>,
    navs: HashMap<u64, Nav>,
    /// Container view → its navigation controller.
    containers: HashMap<u64, u64>,
    /// Navigation bar → its navigation controller.
    bars: HashMap<u64, u64>,
    items: HashMap<u64, u64>,
    item_state: HashMap<u64, Item>,
    titles: HashMap<u64, u64>,
    /// Controllers being pushed / popped right now (`isMovingTo/FromParentViewController`).
    moving_to: std::collections::HashSet<u64>,
    moving_from: std::collections::HashSet<u64>,
    /// Plain stored properties (object, property) → value.
    props: HashMap<(u64, &'static str), u64>,
}

fn get_prop(emu: &mut Emu, key: &'static str, default: u64) -> Result<()> {
    let v = emu.nav.props.get(&(emu.arg(0), key)).copied().unwrap_or(default);
    ret(emu, v)
}

fn set_prop(emu: &mut Emu, key: &'static str) -> Result<()> {
    let (o, v) = (emu.arg(0), emu.arg(2));
    emu.nav.props.insert((o, key), v);
    Ok(())
}

/// A stored property: getter and setter entries for `class`.
macro_rules! prop {
    ($class:expr, $get:literal, $set:literal, $default:expr) => {
        [($class, false, $get, (|e| get_prop(e, $get, $default)) as HostFn), ($class, false, $set, (|e| set_prop(e, $get)) as HostFn)]
    };
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn nav(emu: &mut Emu, nc: u64) -> Nav {
    emu.nav.navs.entry(nc).or_insert_with(|| Nav { translucent: true, ..Default::default() }).clone()
}

fn nav_mut(emu: &mut Emu, nc: u64) -> &mut Nav {
    emu.nav.navs.entry(nc).or_insert_with(|| Nav { translucent: true, ..Default::default() })
}

// ---- containment ----

fn add_child(emu: &mut Emu, parent: u64, child: u64) -> Result<()> {
    if child == 0 || emu.nav.parent.get(&child) == Some(&parent) {
        return Ok(());
    }
    remove_from_parent(emu, child)?;
    send(emu, child, "willMoveToParentViewController:", &[parent])?;
    emu.nav.parent.insert(child, parent);
    emu.nav.children.entry(parent).or_default().push(child);
    Ok(())
}

fn remove_from_parent(emu: &mut Emu, child: u64) -> Result<()> {
    if let Some(p) = emu.nav.parent.remove(&child)
        && let Some(list) = emu.nav.children.get_mut(&p)
    {
        list.retain(|&c| c != child);
    }
    Ok(())
}

/// The navigation controller a view controller is in (itself included).
fn navigation_controller_of(emu: &Emu, mut vc: u64) -> u64 {
    while vc != 0 {
        if emu.nav.navs.contains_key(&vc) {
            return vc;
        }
        vc = emu.nav.parent.get(&vc).copied().unwrap_or(0);
    }
    0
}

// ---- UINavigationController ----

/// The container view and bar, made on first use.
fn ensure_view(emu: &mut Emu, nc: u64) -> Result<u64> {
    let n = nav(emu, nc);
    if n.container != 0 {
        return Ok(n.container);
    }
    let container = new_object(emu, "UIView")?;
    send(emu, container, "init", &[])?;
    set_frame_of(emu, container, [0.0, 0.0, SCREEN_W, SCREEN_H]);
    let bar = if n.bar != 0 { n.bar } else { new_object(emu, "UINavigationBar")? };
    send(emu, bar, "init", &[])?;
    {
        let s = nav_mut(emu, nc);
        s.container = container;
        s.bar = bar;
    }
    emu.nav.containers.insert(container, nc);
    emu.nav.bars.insert(bar, nc);
    emu.ui.views.vc_view.insert(nc, container);
    send(emu, container, "addSubview:", &[bar])?;
    Ok(container)
}

/// Place the bar and the top view controller's view in the container.
pub fn layout_container(emu: &mut Emu, container: u64) -> Result<()> {
    let Some(&nc) = emu.nav.containers.get(&container) else { return Ok(()) };
    let n = nav(emu, nc);
    let b = view(emu, container).bounds;
    let (w, h) = (b[2], b[3]);
    let bar_bottom = SAFE_AREA[0] + BAR_HEIGHT;
    set_frame_of(emu, n.bar, [0.0, SAFE_AREA[0], w, BAR_HEIGHT]);
    view_mut(emu, n.bar).hidden = n.bar_hidden;
    if let Some(&top) = n.stack.last() {
        let v = send(emu, top, "view", &[])?;
        // The view runs under the bar unless the controller excludes the top edge.
        let edges = emu.nav.props.get(&(top, "edgesForExtendedLayout")).copied().unwrap_or(15);
        let under = n.bar_hidden || edges & 1 != 0;
        let frame = if under { [0.0, 0.0, w, h] } else { [0.0, bar_bottom, w, (h - bar_bottom).max(0.0)] };
        set_frame_of(emu, v, frame);
    }
    Ok(())
}

/// Switch to `stack`, running UIKit's containment and appearance sequence.
fn set_stack(emu: &mut Emu, nc: u64, stack: Vec<u64>, animated: bool) -> Result<()> {
    let container = ensure_view(emu, nc)?;
    let old = nav(emu, nc).stack;
    let (old_top, new_top) = (old.last().copied(), stack.last().copied());
    let removed: Vec<u64> = old.iter().copied().filter(|v| !stack.contains(v)).collect();
    let added: Vec<u64> = stack.iter().copied().filter(|v| !old.contains(v)).collect();
    // Leaving controllers hear willMove(nil) first; arriving ones are added as children.
    for &vc in &removed {
        emu.nav.moving_from.insert(vc);
        send(emu, vc, "willMoveToParentViewController:", &[0])?;
    }
    for &vc in &added {
        emu.nav.moving_to.insert(vc);
        send(emu, vc, "retain", &[])?;
        add_child(emu, nc, vc)?;
    }
    let delegate = nav(emu, nc).delegate;
    let a = animated as u64;
    // UIKit updates `viewControllers` before telling the delegate.
    nav_mut(emu, nc).stack = stack.clone();
    if old_top != new_top {
        if let Some(nt) = new_top {
            delegate_call(emu, delegate, "navigationController:willShowViewController:animated:", &[nc, nt, a])?;
            send(emu, nt, "viewWillAppear:", &[a])?;
        }
        if let Some(ot) = old_top {
            send(emu, ot, "viewWillDisappear:", &[a])?;
        }
        if let Some(nt) = new_top {
            let v = send(emu, nt, "view", &[])?;
            let bar = nav(emu, nc).bar;
            send(emu, container, "insertSubview:belowSubview:", &[v, bar])?;
        }
    }
    layout_container(emu, container)?;
    // Popped controllers are detached before `viewDidDisappear:` (screens checks
    // `parentViewController == nil` there to tell JS the screen was dismissed).
    for &vc in &removed {
        remove_from_parent(emu, vc)?;
    }
    if old_top != new_top {
        if let Some(ot) = old_top {
            let v = send(emu, ot, "view", &[])?;
            send(emu, v, "removeFromSuperview", &[])?;
            send(emu, ot, "viewDidDisappear:", &[a])?;
        }
        if let Some(nt) = new_top {
            send(emu, nt, "viewDidAppear:", &[a])?;
            delegate_call(emu, delegate, "navigationController:didShowViewController:animated:", &[nc, nt, a])?;
        }
    }
    for &vc in &removed {
        send(emu, vc, "didMoveToParentViewController:", &[0])?;
        emu.nav.moving_from.remove(&vc);
        send(emu, vc, "release", &[])?;
    }
    for &vc in &added {
        send(emu, vc, "didMoveToParentViewController:", &[nc])?;
        emu.nav.moving_to.remove(&vc);
    }
    view_mut(emu, container).needs_layout = true;
    Ok(())
}

fn delegate_call(emu: &mut Emu, delegate: u64, sel: &str, args: &[u64]) -> Result<()> {
    let s = objc::intern(emu, sel)?;
    if delegate != 0 && objc::responds(emu, delegate, s)? {
        send(emu, delegate, sel, args)?;
    }
    Ok(())
}

fn nc_init_with_root(emu: &mut Emu) -> Result<()> {
    let (nc, root) = (emu.arg(0), emu.arg(2));
    nav_mut(emu, nc);
    if root != 0 {
        set_stack(emu, nc, vec![root], false)?;
    }
    ret(emu, nc)
}

fn nc_init(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(0);
    nav_mut(emu, nc);
    ret(emu, nc)
}

fn nc_load_view(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(0);
    ensure_view(emu, nc)?;
    Ok(())
}

fn nc_view_controllers(emu: &mut Emu) -> Result<()> {
    let stack = nav(emu, emu.arg(0)).stack;
    let a = foundation::new_array_of(emu, &stack)?;
    ret(emu, a)
}

fn nc_set_view_controllers(emu: &mut Emu) -> Result<()> {
    let (nc, a, animated) = (emu.arg(0), emu.arg(2), emu.arg(3) & 0xff != 0);
    let stack = if a == 0 { Vec::new() } else { foundation::array_values(emu, a)? };
    set_stack(emu, nc, stack, animated)
}

fn nc_push(emu: &mut Emu) -> Result<()> {
    let (nc, vc, animated) = (emu.arg(0), emu.arg(2), emu.arg(3) & 0xff != 0);
    let mut stack = nav(emu, nc).stack;
    stack.push(vc);
    set_stack(emu, nc, stack, animated)
}

fn nc_pop(emu: &mut Emu) -> Result<()> {
    let (nc, animated) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    let mut stack = nav(emu, nc).stack;
    if stack.len() < 2 {
        return ret(emu, 0);
    }
    let popped = stack.pop().unwrap_or(0);
    send(emu, popped, "retain", &[])?;
    set_stack(emu, nc, stack, animated)?;
    send(emu, popped, "autorelease", &[])?;
    ret(emu, popped)
}

fn nc_pop_to(emu: &mut Emu, target: Option<u64>) -> Result<()> {
    let (nc, animated) = (emu.arg(0), emu.arg(if target.is_some() { 3 } else { 2 }) & 0xff != 0);
    let stack = nav(emu, nc).stack;
    let keep = match target {
        Some(t) => stack.iter().position(|&v| v == t).map_or(stack.len(), |i| i + 1),
        None => 1.min(stack.len()),
    };
    let popped = stack[keep..].to_vec();
    set_stack(emu, nc, stack[..keep].to_vec(), animated)?;
    let a = foundation::new_array_of(emu, &popped)?;
    ret(emu, a)
}

fn nc_top(emu: &mut Emu) -> Result<()> {
    let top = nav(emu, emu.arg(0)).stack.last().copied().unwrap_or(0);
    ret(emu, top)
}

fn nc_bar(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(0);
    ensure_view(emu, nc)?;
    let bar = nav(emu, nc).bar;
    ret(emu, bar)
}

fn nc_set_bar_hidden(emu: &mut Emu) -> Result<()> {
    let (nc, hidden) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    nav_mut(emu, nc).bar_hidden = hidden;
    let c = nav(emu, nc).container;
    if c != 0 {
        layout_container(emu, c)?;
    }
    Ok(())
}

fn nc_pop_gesture(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(0);
    let mut g = nav(emu, nc).pop_gesture;
    if g == 0 {
        g = new_object(emu, "UIScreenEdgePanGestureRecognizer")?;
        nav_mut(emu, nc).pop_gesture = g;
    }
    ret(emu, g)
}

// ---- UINavigationBar (what's on it, for drawing; its back button) ----

/// The back button's tap area in bar coordinates: the 44-pt circle 16 pt in, with slop.
pub const BACK_BUTTON: [f64; 4] = [8.0, 0.0, 60.0, BAR_HEIGHT];

/// A finger lifted on the bar: on the back button, pop (as UIKit's own button does).
fn bar_touches_ended(emu: &mut Emu) -> Result<()> {
    let bar = emu.arg(0);
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(()) };
    let Some(p) = crate::touch::active_location(emu) else { return Ok(()) };
    let o = crate::uiview::window_origin(emu, bar);
    let (x, y) = (p[0] - o[0], p[1] - o[1]);
    let [bx, by, bw, bh] = BACK_BUTTON;
    let n = nav(emu, nc);
    if n.stack.len() >= 2 && x >= bx && x < bx + bw && y >= by && y < by + bh {
        let top = n.stack[n.stack.len() - 1];
        if !vc_item(emu, top)?.hides_back {
            send(emu, nc, "popViewControllerAnimated:", &[1])?;
        }
    }
    Ok(())
}

/// The bar's title and back-button title, if it belongs to a navigation controller.
pub fn bar_titles(emu: &mut Emu, bar: u64) -> Result<Option<(String, Option<String>)>> {
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(None) };
    let stack = nav(emu, nc).stack;
    let Some(&top) = stack.last() else { return Ok(None) };
    let title = vc_title(emu, top)?;
    let back = if stack.len() >= 2 {
        let prev = stack[stack.len() - 2];
        let item = vc_item(emu, prev)?;
        let top_item = vc_item(emu, top)?;
        if top_item.hides_back {
            None
        } else if item.back_title != 0 {
            Some(foundation::nsstring(emu, item.back_title)?)
        } else {
            Some(vc_title(emu, prev)?).filter(|t| !t.is_empty()).or(Some("Back".into()))
        }
    } else {
        None
    };
    Ok(Some((title, back)))
}

fn vc_title(emu: &mut Emu, vc: u64) -> Result<String> {
    let item = vc_item(emu, vc)?;
    let t = if item.title != 0 { item.title } else { emu.nav.titles.get(&vc).copied().unwrap_or(0) };
    if t == 0 { Ok(String::new()) } else { foundation::nsstring(emu, t) }
}

// ---- UINavigationItem ----

fn item_of(emu: &mut Emu, vc: u64) -> Result<u64> {
    if let Some(&i) = emu.nav.items.get(&vc) {
        return Ok(i);
    }
    let i = new_object(emu, "UINavigationItem")?;
    emu.nav.items.insert(vc, i);
    Ok(i)
}

fn item_state(emu: &Emu, item: u64) -> Item {
    emu.nav.item_state.get(&item).cloned().unwrap_or_default()
}

fn vc_item(emu: &mut Emu, vc: u64) -> Result<Item> {
    let i = item_of(emu, vc)?;
    Ok(item_state(emu, i))
}

fn retained(emu: &mut Emu, v: u64) -> Result<u64> {
    if v != 0 {
        send(emu, v, "copy", &[])
    } else {
        Ok(0)
    }
}

// ---- the responder chain ----

/// A view's next responder: the controller it is the view of, else its superview.
fn view_next_responder(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let vc = emu.ui.views.vc_view.iter().find(|&(_, &view)| view == v).map(|(&vc, _)| vc);
    let next = match vc {
        Some(vc) => vc,
        None => view(emu, v).superview,
    };
    ret(emu, next)
}

/// A view controller's next responder: its view's superview, else its parent.
fn vc_next_responder(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(0);
    let sup = emu.ui.views.vc_view.get(&vc).map_or(0, |&v| emu.ui.views.views.get(&v).map_or(0, |w| w.superview));
    let next = if sup != 0 { sup } else { emu.nav.parent.get(&vc).copied().unwrap_or(0) };
    ret(emu, next)
}

// ---- safe areas ----

/// A view's safe-area insets (top, left, bottom, right): the device's, plus a
/// visible navigation bar's, as they overlap the view.
pub fn safe_area(emu: &mut Emu, v: u64) -> [f64; 4] {
    let o = crate::uiview::window_origin(emu, v);
    let b = view(emu, v).bounds;
    let (top, bottom) = (o[1], o[1] + b[3]);
    let mut safe_top = SAFE_AREA[0];
    // Inside a navigation controller with a visible bar, the bar covers more.
    let mut cur = v;
    while cur != 0 {
        if let Some(&nc) = emu.nav.containers.get(&cur) {
            let n = nav(emu, nc);
            if !n.bar_hidden {
                safe_top += BAR_HEIGHT;
            }
            break;
        }
        cur = view(emu, cur).superview;
    }
    let screen_h = SCREEN_H;
    [
        (safe_top - top).clamp(0.0, b[3]),
        0.0,
        (bottom - (screen_h - SAFE_AREA[2])).clamp(0.0, b[3]),
        0.0,
    ]
}

const VC: &str = "UIViewController";

/// Stored UIViewController properties (presentation and layout knobs pomme doesn't act on yet).
pub(crate) const PROPS: &[[(&str, bool, &str, HostFn); 2]] = &[
    prop!(VC, "isModalInPresentation", "setModalInPresentation:", 0),
    prop!(VC, "modalPresentationStyle", "setModalPresentationStyle:", 1), // page sheet
    prop!(VC, "modalTransitionStyle", "setModalTransitionStyle:", 0),
    prop!(VC, "transitioningDelegate", "setTransitioningDelegate:", 0),
    prop!(VC, "definesPresentationContext", "setDefinesPresentationContext:", 0),
    prop!(VC, "providesPresentationContextTransitionStyle", "setProvidesPresentationContextTransitionStyle:", 0),
    prop!(VC, "edgesForExtendedLayout", "setEdgesForExtendedLayout:", 15),
    prop!(VC, "extendedLayoutIncludesOpaqueBars", "setExtendedLayoutIncludesOpaqueBars:", 0),
    prop!(VC, "overrideUserInterfaceStyle", "setOverrideUserInterfaceStyle:", 0),
    prop!(VC, "restorationIdentifier", "setRestorationIdentifier:", 0),
    prop!(VC, "hidesBottomBarWhenPushed", "setHidesBottomBarWhenPushed:", 0),
    prop!(VC, "isEditing", "setEditing:", 0),
    prop!(VC, "modalPresentationCapturesStatusBarAppearance", "setModalPresentationCapturesStatusBarAppearance:", 0),
];
const NC: &str = "UINavigationController";
const NI: &str = "UINavigationItem";
const NB: &str = "UINavigationBar";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // iOS 26 layout regions: the safe area, with or without corner adaptation
    // (pomme's insets have no rounded-corner adjustments, so they are the same).
    ("UIViewLayoutRegion", true, "safeAreaLayoutRegionWithCornerAdaptation:", |e| {
        let r = new_object(e, "UIViewLayoutRegion")?;
        ret(e, r)
    }),
    ("UIViewLayoutRegion", true, "marginsLayoutRegionWithCornerAdaptation:", |e| {
        let r = new_object(e, "UIViewLayoutRegion")?;
        ret(e, r)
    }),
    ("UIView", false, "edgeInsetsForLayoutRegion:", crate::uiview::safe_area_insets),
    ("UIView", false, "nextResponder", view_next_responder),
    // Portrait only: pomme's screen doesn't rotate.
    ("UIApplication", false, "supportedInterfaceOrientationsForWindow:", |e| ret(e, 2)),
    (VC, false, "nextResponder", vc_next_responder),
    // iOS 26 scroll edge effects (the blur under bars): styles are tokens; no
    // effect is drawn, so scroll views report none (messages to nil are no-ops).
    ("UIScrollEdgeEffectStyle", true, "automaticStyle", |e| {
        let o = new_object(e, "UIScrollEdgeEffectStyle")?;
        ret(e, o)
    }),
    ("UIScrollEdgeEffectStyle", true, "hardStyle", |e| {
        let o = new_object(e, "UIScrollEdgeEffectStyle")?;
        ret(e, o)
    }),
    ("UIScrollEdgeEffectStyle", true, "softStyle", |e| {
        let o = new_object(e, "UIScrollEdgeEffectStyle")?;
        ret(e, o)
    }),
    (VC, false, "addChildViewController:", |e| {
        let (p, c) = (e.arg(0), e.arg(2));
        add_child(e, p, c)
    }),
    (VC, false, "removeFromParentViewController", |e| {
        let c = e.arg(0);
        remove_from_parent(e, c)
    }),
    (VC, false, "parentViewController", |e| {
        let p = e.nav.parent.get(&e.arg(0)).copied().unwrap_or(0);
        ret(e, p)
    }),
    (VC, false, "childViewControllers", |e| {
        let c = e.nav.children.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = foundation::new_array_of(e, &c)?;
        ret(e, a)
    }),
    (VC, false, "children", |e| {
        let c = e.nav.children.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = foundation::new_array_of(e, &c)?;
        ret(e, a)
    }),
    (VC, false, "navigationController", |e| {
        let p = e.nav.parent.get(&e.arg(0)).copied().unwrap_or(0);
        let n = navigation_controller_of(e, p);
        ret(e, n)
    }),
    (VC, false, "navigationItem", |e| {
        let i = item_of(e, e.arg(0))?;
        ret(e, i)
    }),
    (VC, false, "title", |e| {
        let t = e.nav.titles.get(&e.arg(0)).copied().unwrap_or(0);
        ret(e, t)
    }),
    (VC, false, "setTitle:", |e| {
        let (vc, t) = (e.arg(0), e.arg(2));
        let t = retained(e, t)?;
        e.nav.titles.insert(vc, t);
        Ok(())
    }),
    (VC, false, "willMoveToParentViewController:", |_| Ok(())),
    (VC, false, "didMoveToParentViewController:", |_| Ok(())),
    (VC, false, "viewWillDisappear:", |_| Ok(())),
    (VC, false, "viewDidDisappear:", |_| Ok(())),
    (VC, false, "transitionCoordinator", |e| ret(e, 0)),
    (VC, false, "isMovingToParentViewController", |e| {
        let m = e.nav.moving_to.contains(&e.arg(0));
        ret(e, m as u64)
    }),
    (VC, false, "isMovingFromParentViewController", |e| {
        let m = e.nav.moving_from.contains(&e.arg(0));
        ret(e, m as u64)
    }),
    (VC, false, "isBeingPresented", |e| ret(e, 0)),
    (VC, false, "isBeingDismissed", |e| ret(e, 0)),
    (VC, false, "tabBarController", |e| ret(e, 0)),
    (VC, false, "splitViewController", |e| ret(e, 0)),
    (VC, false, "beginAppearanceTransition:animated:", |_| Ok(())),
    (VC, false, "presentationController", |e| ret(e, 0)),
    (VC, false, "sheetPresentationController", |e| ret(e, 0)),
    (VC, false, "popoverPresentationController", |e| ret(e, 0)),
    (VC, false, "setAdditionalSafeAreaInsets:", |_| Ok(())),
    (VC, false, "setPreferredContentSize:", |_| Ok(())),
    (VC, false, "preferredStatusBarUpdateAnimation", |e| ret(e, 1)), // fade
    (VC, false, "supportedInterfaceOrientations", |e| ret(e, 2)), // portrait
    (VC, false, "childViewControllerForHomeIndicatorAutoHidden", |e| ret(e, 0)),
    (VC, false, "prefersHomeIndicatorAutoHidden", |e| ret(e, 0)),
    (VC, false, "setNeedsUpdateOfHomeIndicatorAutoHidden", |_| Ok(())),
    (VC, false, "setNeedsUpdateOfSupportedInterfaceOrientations", |_| Ok(())),
    (VC, false, "endAppearanceTransition", |_| Ok(())),
    (NC, false, "init", nc_init),
    (NC, false, "initWithNibName:bundle:", nc_init),
    (NC, false, "initWithRootViewController:", nc_init_with_root),
    (NC, false, "initWithNavigationBarClass:toolbarClass:", |e| {
        let (nc, bar_class) = (e.arg(0), e.arg(2));
        if bar_class != 0 {
            let bar = objc::create_instance(e, bar_class, 0)?;
            nav_mut(e, nc).bar = bar;
        } else {
            nav_mut(e, nc);
        }
        ret(e, nc)
    }),
    (NC, false, "loadView", nc_load_view),
    (NC, false, "viewControllers", nc_view_controllers),
    (NC, false, "setViewControllers:", |e| {
        e.cpu.x[3] = 0;
        nc_set_view_controllers(e)
    }),
    (NC, false, "setViewControllers:animated:", nc_set_view_controllers),
    (NC, false, "pushViewController:animated:", nc_push),
    (NC, false, "popViewControllerAnimated:", nc_pop),
    (NC, false, "popToRootViewControllerAnimated:", |e| nc_pop_to(e, None)),
    (NC, false, "popToViewController:animated:", |e| {
        let t = e.arg(2);
        nc_pop_to(e, Some(t))
    }),
    (NC, false, "topViewController", nc_top),
    (NC, false, "visibleViewController", nc_top),
    (NC, false, "navigationBar", nc_bar),
    (NC, false, "delegate", |e| {
        let d = nav(e, e.arg(0)).delegate;
        ret(e, d)
    }),
    (NC, false, "setDelegate:", |e| {
        let (nc, d) = (e.arg(0), e.arg(2));
        nav_mut(e, nc).delegate = d;
        Ok(())
    }),
    (NC, false, "isNavigationBarHidden", |e| {
        let h = nav(e, e.arg(0)).bar_hidden;
        ret(e, h as u64)
    }),
    (NC, false, "setNavigationBarHidden:", nc_set_bar_hidden),
    (NC, false, "setNavigationBarHidden:animated:", nc_set_bar_hidden),
    (NC, false, "interactivePopGestureRecognizer", nc_pop_gesture),
    // iOS 26: popping by swiping anywhere in the content (pomme has one recognizer for both).
    (NC, false, "interactiveContentPopGestureRecognizer", nc_pop_gesture),
    (NC, false, "toolbar", |e| ret(e, 0)),
    (NB, false, "touchesBegan:withEvent:", |_| Ok(())),
    (NB, false, "touchesMoved:withEvent:", |_| Ok(())),
    (NB, false, "touchesCancelled:withEvent:", |_| Ok(())),
    (NB, false, "touchesEnded:withEvent:", bar_touches_ended),
    (NB, false, "isTranslucent", |e| {
        let nc = e.nav.bars.get(&e.arg(0)).copied().unwrap_or(0);
        let t = nc == 0 || nav(e, nc).translucent;
        ret(e, t as u64)
    }),
    (NB, false, "setTranslucent:", |e| {
        let (bar, t) = (e.arg(0), e.arg(2) & 0xff != 0);
        if let Some(&nc) = e.nav.bars.get(&bar) {
            nav_mut(e, nc).translucent = t;
            let c = nav(e, nc).container;
            if c != 0 {
                layout_container(e, c)?;
            }
        }
        Ok(())
    }),
    (NI, false, "title", |e| {
        let t = item_state(e, e.arg(0)).title;
        ret(e, t)
    }),
    (NI, false, "setTitle:", |e| {
        let (i, t) = (e.arg(0), e.arg(2));
        let t = retained(e, t)?;
        e.nav.item_state.entry(i).or_default().title = t;
        Ok(())
    }),
    (NI, false, "backButtonTitle", |e| {
        let t = item_state(e, e.arg(0)).back_title;
        ret(e, t)
    }),
    (NI, false, "setBackButtonTitle:", |e| {
        let (i, t) = (e.arg(0), e.arg(2));
        let t = retained(e, t)?;
        e.nav.item_state.entry(i).or_default().back_title = t;
        Ok(())
    }),
    (NI, false, "titleView", |e| {
        let t = item_state(e, e.arg(0)).title_view;
        ret(e, t)
    }),
    (NI, false, "setTitleView:", |e| {
        let (i, t) = (e.arg(0), e.arg(2));
        e.nav.item_state.entry(i).or_default().title_view = t;
        Ok(())
    }),
    (NI, false, "hidesBackButton", |e| {
        let h = item_state(e, e.arg(0)).hides_back;
        ret(e, h as u64)
    }),
    (NI, false, "setHidesBackButton:", |e| {
        let (i, h) = (e.arg(0), e.arg(2) & 0xff != 0);
        e.nav.item_state.entry(i).or_default().hides_back = h;
        Ok(())
    }),
    (NI, false, "setHidesBackButton:animated:", |e| {
        let (i, h) = (e.arg(0), e.arg(2) & 0xff != 0);
        e.nav.item_state.entry(i).or_default().hides_back = h;
        Ok(())
    }),
];
