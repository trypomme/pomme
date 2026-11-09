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
use crate::uiview::{set_frame_of, view, view_mut};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::HashMap;

/// The navigation bar's height (below the status bar).
pub const BAR_HEIGHT: f64 = 44.0;
pub const COMPACT_BAR_HEIGHT: f64 = 32.0;

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
    pub(crate) children: HashMap<u64, Vec<u64>>,
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
    /// Navigation controller → its push or pop that's animating.
    transitions: HashMap<u64, Transition>,
    /// Modal presentations, in the order they were made (the last is on top).
    presentations: Vec<Presentation>,
}

/// A presented view controller and what shows it.
#[derive(Clone, Debug)]
struct Presentation {
    /// The controller it's presented from (the root of the one that asked).
    presenter: u64,
    presented: u64,
    /// Over the window: holds the dimming view and the sheet, or the presented view.
    container: u64,
    dim: u64,
    /// The sheet holding the presented view (0 for full screen).
    sheet: u64,
    /// Full screen, the screen behind leaves the hierarchy once covered (not for `overFullScreen`).
    covers: bool,
    /// Presenting (true) or dismissing (false) right now, with the completion block to run.
    moving: Option<bool>,
    completion: u64,
}

// iOS 26's modal presentation on iPhone, measured in the Simulator
// (scripts/measure/transitions): sheets and full-screen controllers slide up
// from the bottom on a critically damped spring and back down on another; a
// page sheet runs from the safe area's top to the bottom, its top corners
// continuous with a 37.6 pt radius, over 20% black dimming; the screen
// behind keeps its size.
const PRESENT_SPRING: (f64, f64, f64) = (18.0, 0.99, 2.5);
const DISMISS_SPRING: (f64, f64, f64) = (16.8, 0.94, 0.0);
const SHEET_RADIUS: f64 = 37.6;
const SHEET_DIMMING: [f64; 4] = [0.0, 0.0, 0.0, 0.2];

/// A change of the top controller: what's left to do when its transition settles.
#[derive(Clone)]
struct Transition {
    nc: u64,
    old_top: Option<u64>,
    new_top: Option<u64>,
    removed: Vec<u64>,
    added: Vec<u64>,
    animated: bool,
    /// Views the transition animates (their transition animations end with it), and its dimming view.
    moving: Vec<u64>,
    dim: u64,
}

// UIKit's push and pop on iPhone, measured in the iOS 26 Simulator
// (scripts/measure/transitions): the incoming view slides across the whole
// width, the outgoing one moves 30% of it the other way under a 10% black
// dimming view, on springs (natural frequency rad/s, damping ratio, initial
// velocity: a pop starts already moving).
const PUSH_SPRING: (f64, f64, f64) = (26.0, 1.11, 0.0);
const POP_SPRING: (f64, f64, f64) = (20.4, 0.99, 9.0);
const PARALLAX: f64 = 0.3;
const DIMMING: [f64; 4] = [0.0, 0.0, 0.0, 0.1];

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

/// An appearance callback (`viewWillAppear:` …) on `vc` and, as UIKit forwards
/// them, its visible children: a navigation controller's top, every child of
/// other containers.
pub fn appearance(emu: &mut Emu, vc: u64, sel: &str, animated: bool) -> Result<()> {
    send(emu, vc, sel, &[animated as u64])?;
    let children = emu.nav.children.get(&vc).cloned().unwrap_or_default();
    let visible: Vec<u64> = if emu.nav.navs.contains_key(&vc) { emu.nav.navs[&vc].stack.last().copied().into_iter().collect() } else { children };
    for c in visible {
        appearance(emu, c, sel, animated)?;
    }
    Ok(())
}

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
    let (w, h) = crate::orientation::screen(emu);
    set_frame_of(emu, container, [0.0, 0.0, w, h]);
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
    let safe_top = crate::orientation::safe_area(emu)[0];
    let bar_h = bar_height(emu);
    let bar_bottom = safe_top + bar_h;
    set_frame_of(emu, n.bar, [0.0, safe_top, w, bar_h]);
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

/// Switch to `stack`, running UIKit's containment and appearance sequence:
/// the `will` half now; then, when the transition (if animated) settles, the
/// `did` half ([`finish`]).
fn set_stack(emu: &mut Emu, nc: u64, stack: Vec<u64>, animated: bool) -> Result<()> {
    // A transition still running finishes first.
    if let Some(t) = emu.nav.transitions.remove(&nc) {
        finish(emu, t)?;
    }
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
    // A push brings in a controller that wasn't on the stack; a pop goes back to one that was.
    let forward = new_top.is_some_and(|t| !old.contains(&t));
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
            // Pushed, it comes in on top; popped back to, it's underneath the one leaving.
            let below = match old_top {
                Some(ot) if !forward => send(emu, ot, "view", &[])?,
                _ => nav(emu, nc).bar,
            };
            send(emu, container, "insertSubview:belowSubview:", &[v, below])?;
        }
    }
    layout_container(emu, container)?;
    let mut t = Transition { nc, old_top, new_top, removed, added, animated, moving: Vec::new(), dim: 0 };
    view_mut(emu, container).needs_layout = true;
    match (old_top, new_top) {
        (Some(ot), Some(nt)) if animated && ot != nt => {
            let duration = slide(emu, container, ot, nt, forward, &mut t)?;
            emu.nav.transitions.insert(nc, t);
            crate::uikit::after_on_main(emu, duration, finish_fire, nc)
        }
        _ => finish(emu, t),
    }
}

/// Start the push or pop slide; returns how long it takes to settle.
fn slide(emu: &mut Emu, container: u64, ot: u64, nt: u64, forward: bool, t: &mut Transition) -> Result<f64> {
    use crate::animation::{Prop, Val, transition, transition_spring};
    let (ov, nv) = (send(emu, ot, "view", &[])?, send(emu, nt, "view", &[])?);
    let w = view(emu, container).bounds[2];
    let (timing, duration) = transition_spring(if forward { PUSH_SPRING } else { POP_SPRING });
    // The view going underneath is dimmed: the old one on a push, the new one on a pop.
    let under = if forward { ov } else { nv };
    let dim = {
        let cls = crate::objc::host_class(emu, "UIView")?;
        let d = send(emu, cls, "alloc", &[])?;
        let frame = crate::uiview::frame_of(emu, under);
        for (i, f) in frame.iter().enumerate() {
            emu.cpu.q[i] = f.to_bits() as u128;
        }
        let d = send(emu, d, "initWithFrame:", &[])?;
        let black = crate::uiview::new_color(emu, DIMMING)?;
        send(emu, d, "setBackgroundColor:", &[black])?;
        view_mut(emu, d).user_interaction = false;
        send(emu, container, "insertSubview:aboveSubview:", &[d, under])?;
        d
    };
    let (from_dim, to_dim) = if forward { (0.0, 1.0) } else { (1.0, 0.0) };
    view_mut(emu, dim).alpha = to_dim;
    let at = |emu: &mut Emu, v: u64| crate::uiview::view(emu, v).center;
    let (oc, nc) = (at(emu, ov), at(emu, nv));
    let (ol, nl, dl) = (send(emu, ov, "layer", &[])?, send(emu, nv, "layer", &[])?, send(emu, dim, "layer", &[])?);
    if forward {
        transition(emu, nl, Prop::Position, Val::P([nc[0] + w, nc[1]]), Val::P(nc), timing, duration);
        transition(emu, ol, Prop::Position, Val::P(oc), Val::P([oc[0] - PARALLAX * w, oc[1]]), timing, duration);
    } else {
        transition(emu, ol, Prop::Position, Val::P(oc), Val::P([oc[0] + w, oc[1]]), timing, duration);
        transition(emu, nl, Prop::Position, Val::P([nc[0] - PARALLAX * w, nc[1]]), Val::P(nc), timing, duration);
    }
    transition(emu, dl, Prop::Opacity, Val::N(from_dim), Val::N(to_dim), timing, duration);
    t.moving = vec![ol, nl, dl];
    t.dim = dim;
    Ok(duration)
}

/// The run-loop callout when a push or pop settles.
fn finish_fire(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(1);
    match emu.nav.transitions.remove(&nc) {
        Some(t) => finish(emu, t),
        None => Ok(()),
    }
}

/// The `did` half of a change of controllers.
fn finish(emu: &mut Emu, t: Transition) -> Result<()> {
    let Transition { nc, old_top, new_top, removed, added, animated, moving, dim } = t;
    for l in moving {
        crate::animation::end_transition(emu, l);
    }
    if dim != 0 {
        send(emu, dim, "removeFromSuperview", &[])?;
        send(emu, dim, "release", &[])?;
    }
    let delegate = nav(emu, nc).delegate;
    let a = animated as u64;
    // Popped controllers are detached before `viewDidDisappear:` (screens checks
    // `parentViewController == nil` there to tell JS the screen was dismissed).
    for &vc in &removed {
        remove_from_parent(emu, vc)?;
    }
    if old_top != new_top {
        if let Some(ot) = old_top {
            // Still on the stack (pushed over), or not: either way its view leaves the container.
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

// ---- UINavigationBar: what's on it, how it looks, its back button ----

/// What to draw for a navigation bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarLook {
    pub title: String,
    pub back: Option<Back>,
    /// The iOS 26 design (glass back button, no bar background) or the one before.
    pub glass: bool,
    /// Fill behind the bar, extended up under the status bar (pre-26).
    pub background: Option<[f64; 4]>,
    /// The hairline along the bar's bottom edge (pre-26).
    pub shadow: Option<[f64; 4]>,
    /// Where the bar's leading content starts: its left safe-area inset
    /// (the sensor housing's side in landscape).
    pub leading: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Back {
    /// Text next to the chevron (pre-26 only; None: chevron alone).
    pub label: Option<String>,
    /// Tap area in bar coordinates (x, width).
    pub hit: (f64, f64),
}

/// Font of bar titles and back labels.
pub const TITLE_SIZE: f64 = 17.0;
pub const TITLE_WEIGHT: f64 = 0.3; // semibold
/// Pre-26 back button: chevron at x 8, label 26 pt further.
pub const BACK_LABEL_X: f64 = 26.0;
/// UIKit's default bar hairline: black at 30%.
const DEFAULT_SHADOW: [f64; 4] = [0.0, 0.0, 0.0, 0.3];
/// The default (blurred) bar material, flattened: pomme doesn't blur yet.
const CHROME_MATERIAL: [f64; 4] = [0.976, 0.976, 0.976, 0.94];

// UIBarAppearance background configurations.
const DEFAULT_BACKGROUND: u64 = 0;
const OPAQUE_BACKGROUND: u64 = 1;
const TRANSPARENT_BACKGROUND: u64 = 2;

/// `-configureWith…Background` (and a fresh appearance, which is the default
/// configuration): UIKit's values for the background, blur and hairline.
fn configure_appearance(emu: &mut Emu, a: u64, style: u64) -> Result<()> {
    emu.nav.props.insert((a, "background"), style);
    let bg = match style {
        OPAQUE_BACKGROUND => crate::uiview::new_color(emu, [1.0, 1.0, 1.0, 1.0])?, // systemBackground
        _ => 0,
    };
    let effect = if style == DEFAULT_BACKGROUND { new_object(emu, "UIBlurEffect")? } else { 0 };
    let shadow = if style == TRANSPARENT_BACKGROUND { 0 } else { crate::uiview::new_color(emu, DEFAULT_SHADOW)? };
    objc::bag_set(emu, a, "backgroundColor", bg);
    objc::bag_set(emu, a, "backgroundEffect", effect);
    objc::bag_set(emu, a, "shadowColor", shadow);
    objc::bag_set(emu, a, "shadowImage", 0);
    Ok(())
}

fn copy_appearance(emu: &mut Emu, from: u64, to: u64) {
    objc::bag_copy(emu, from, to);
    if let Some(&style) = emu.nav.props.get(&(from, "background")) {
        emu.nav.props.insert((to, "background"), style);
    }
}

fn color_of(emu: &Emu, c: u64) -> Option<[f64; 4]> {
    emu.ui.views.colors.get(&c).copied().filter(|c| c[3] > 0.0)
}

/// Background and hairline from the top item's appearance (pre-26). Content
/// that doesn't scroll sits at the scroll edge, so `scrollEdgeAppearance`
/// applies; without one, UIKit uses the standard one made transparent.
fn bar_background(emu: &mut Emu, item: u64) -> (Option<[f64; 4]>, Option<[f64; 4]>) {
    let edge = objc::bag_get(emu, item, "scrollEdgeAppearance").unwrap_or(0);
    if edge == 0 {
        return (None, None);
    }
    let effect = objc::bag_get(emu, edge, "backgroundEffect").unwrap_or(0);
    let bg = objc::bag_get(emu, edge, "backgroundColor").and_then(|c| color_of(emu, c));
    let background = bg.or((effect != 0).then_some(CHROME_MATERIAL));
    let shadow = objc::bag_get(emu, edge, "shadowColor").and_then(|c| color_of(emu, c));
    (background, shadow)
}

fn text_width(emu: &mut Emu, text: &str, weight: f64) -> f64 {
    crate::textkit::line_glyphs(emu, text, TITLE_SIZE, weight, [0.0; 4], 0.0, 0.0).1
}

/// How a navigation bar looks now, if it belongs to a navigation controller.
pub fn bar_look(emu: &mut Emu, bar: u64) -> Result<Option<BarLook>> {
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(None) };
    let stack = nav(emu, nc).stack;
    let Some(&top) = stack.last() else { return Ok(None) };
    let glass = emu.system.liquid_glass();
    let title = vc_title(emu, top)?;
    let top_item = item_of(emu, top)?;
    let leading = safe_area(emu, bar)[1];
    let mut back = None;
    if stack.len() >= 2 && !item_state(emu, top_item).hides_back {
        back = Some(if glass {
            // A 44-pt glass circle 16 pt in (with a little slop).
            Back { label: None, hit: (leading + 8.0, 60.0) }
        } else {
            let prev = stack[stack.len() - 2];
            let prev_item = item_of(emu, prev)?;
            let p = item_state(emu, prev_item);
            // UINavigationItemBackButtonDisplayMode: default, generic ("Back"), minimal.
            let mode = objc::bag_get(emu, prev_item, "backButtonDisplayMode").unwrap_or(0);
            let own = if p.back_title != 0 { foundation::nsstring(emu, p.back_title)? } else { vc_title(emu, prev)? };
            // The label gets what the centered title leaves; UIKit falls back
            // from the previous title to "Back" to the chevron alone.
            let room = (bar_width(emu, bar) - text_width(emu, &title, TITLE_WEIGHT)) / 2.0 - leading - BACK_LABEL_X - 8.0;
            let candidates: Vec<String> = match mode {
                2 => vec![],
                1 => vec!["Back".into()],
                _ => [own, "Back".into()].into_iter().filter(|t| !t.is_empty()).collect(),
            };
            let label = candidates.into_iter().find(|t| text_width(emu, t, 0.0) <= room);
            let w = BACK_LABEL_X + label.as_ref().map_or(0.0, |l| text_width(emu, l, 0.0)) + 8.0;
            Back { label, hit: (leading, w.max(44.0)) }
        });
    }
    let (background, shadow) = if glass { (None, None) } else { bar_background(emu, top_item) };
    Ok(Some(BarLook { title, back, glass, background, shadow, leading }))
}

fn bar_width(emu: &mut Emu, bar: u64) -> f64 {
    view(emu, bar).bounds[2]
}

/// A finger lifted on the bar: on the back button, pop (as UIKit's own button does).
fn bar_touches_ended(emu: &mut Emu) -> Result<()> {
    let bar = emu.arg(0);
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(()) };
    let Some(p) = crate::touch::active_location(emu) else { return Ok(()) };
    let o = crate::uiview::window_origin(emu, bar);
    let (x, y) = (p[0] - o[0], p[1] - o[1]);
    let Some(Back { hit: (bx, bw), .. }) = bar_look(emu, bar)?.and_then(|l| l.back) else { return Ok(()) };
    let h = view(emu, bar).bounds[3];
    if x >= bx && x < bx + bw && (0.0..h).contains(&y) {
        send(emu, nc, "popViewControllerAnimated:", &[1])?;
    }
    Ok(())
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

/// The safe region of the screen for a view, in window coordinates
/// `[left, top, right, bottom]`: inside the status bar (and a visible
/// navigation bar the view sits under), the sensor housing's side in
/// landscape, and the home indicator.
pub fn safe_rect(emu: &mut Emu, v: u64) -> [f64; 4] {
    let (w, h) = crate::orientation::screen(emu);
    let [mut top, left, bottom, right] = crate::orientation::safe_area(emu);
    // Inside a navigation controller with a visible bar, the bar covers more.
    let mut cur = v;
    while cur != 0 {
        if let Some(&nc) = emu.nav.containers.get(&cur) {
            if !nav(emu, nc).bar_hidden {
                top += bar_height(emu);
            }
            break;
        }
        cur = view(emu, cur).superview;
    }
    [left, top, w - right, h - bottom]
}

/// A navigation bar's height: 44 pt, or UIKit's compact 32 pt where the
/// vertical size class is compact (phones in landscape).
pub fn bar_height(emu: &Emu) -> f64 {
    if crate::orientation::size_classes(emu).1 == crate::device::COMPACT { COMPACT_BAR_HEIGHT } else { BAR_HEIGHT }
}

/// A view's safe-area insets (top, left, bottom, right): the device's, plus a
/// visible navigation bar's, as they overlap the view.
pub fn safe_area(emu: &mut Emu, v: u64) -> [f64; 4] {
    let o = crate::uiview::window_origin(emu, v);
    let b = view(emu, v).bounds;
    let [l, t, r, btm] = safe_rect(emu, v);
    [
        (t - o[1]).clamp(0.0, b[3]),
        (l - o[0]).clamp(0.0, b[2]),
        (o[1] + b[3] - btm).clamp(0.0, b[3]),
        (o[0] + b[2] - r).clamp(0.0, b[2]),
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

// ---- modal presentation ----

/// The controller at the top of `vc`'s containment (its outermost parent).
fn root_of(emu: &Emu, mut vc: u64) -> u64 {
    while let Some(&p) = emu.nav.parent.get(&vc) {
        if p == 0 {
            break;
        }
        vc = p;
    }
    vc
}

/// A plain UIView with this frame.
fn new_view(emu: &mut Emu, frame: [f64; 4]) -> Result<u64> {
    let cls = crate::objc::host_class(emu, "UIView")?;
    let v = send(emu, cls, "alloc", &[])?;
    for (i, f) in frame.iter().enumerate() {
        emu.cpu.q[i] = f.to_bits() as u128;
    }
    send(emu, v, "initWithFrame:", &[])
}

/// `-presentViewController:animated:completion:`: UIKit's sequence, the view
/// shown as the controller's `modalPresentationStyle` says (page sheets by
/// default on iPhone; full screen covers the screen behind).
fn present(emu: &mut Emu) -> Result<()> {
    let (me, vc, animated, completion) = (emu.arg(0), emu.arg(2), emu.arg(3) & 0xff != 0, emu.arg(4));
    if vc == 0 {
        return Ok(());
    }
    let presenter = root_of(emu, me);
    if emu.nav.presentations.iter().any(|p| p.presenter == presenter || p.presented == vc) {
        // As UIKit does: a warning, and nothing presented.
        eprintln!("Attempt to present a view controller on one which is already presenting");
        return Ok(());
    }
    // UIModalPresentationStyle: 0 full screen, 1 page sheet, 2 form sheet, 3 current context,
    // 4 custom, 5 over full screen, 6 over current context, 7 popover, -2 automatic.
    let style = emu.nav.props.get(&(vc, "modalPresentationStyle")).copied().unwrap_or(1) as i64;
    let sheet_style = matches!(style, 1 | 2 | 7 | -2);
    let covers = matches!(style, 0 | 3);
    let pv = send(emu, presenter, "view", &[])?;
    let window = match crate::uiview::view(emu, pv).window {
        0 => emu.ui.windows.first().copied().unwrap_or(0),
        w => w,
    };
    let wb = crate::uiview::view(emu, window).bounds;
    let (w, h) = (wb[2], wb[3]);
    send(emu, vc, "retain", &[])?;
    let container = new_view(emu, [0.0, 0.0, w, h])?;
    send(emu, window, "addSubview:", &[container])?;
    let vview = send(emu, vc, "view", &[])?;
    let (dim, sheet) = if sheet_style {
        let dim = new_view(emu, [0.0, 0.0, w, h])?;
        let black = crate::uiview::new_color(emu, SHEET_DIMMING)?;
        send(emu, dim, "setBackgroundColor:", &[black])?;
        send(emu, container, "addSubview:", &[dim])?;
        let top = crate::orientation::safe_area(emu)[0];
        let sheet = new_view(emu, [0.0, top, w, h - top])?;
        view_mut(emu, sheet).clips = true;
        let l = send(emu, sheet, "layer", &[])?;
        {
            let ly = crate::calayer::layer_mut(emu, l);
            ly.corner_radius = SHEET_RADIUS;
            ly.continuous_corners = true;
            ly.masked_corners = 3; // the top two
        }
        send(emu, container, "addSubview:", &[sheet])?;
        set_frame_of(emu, vview, [0.0, 0.0, w, h - top]);
        send(emu, sheet, "addSubview:", &[vview])?;
        (dim, sheet)
    } else {
        set_frame_of(emu, vview, [0.0, 0.0, w, h]);
        send(emu, container, "addSubview:", &[vview])?;
        (0, 0)
    };
    let completion = if completion != 0 { crate::libc::copy_block(emu, completion)? } else { 0 };
    let p = Presentation { presenter, presented: vc, container, dim, sheet, covers, moving: Some(true), completion };
    emu.nav.presentations.push(p.clone());
    let a = animated as u64;
    send(emu, vc, "viewWillAppear:", &[a])?;
    if covers {
        send(emu, presenter, "viewWillDisappear:", &[a])?;
    }
    if animated {
        let duration = slide_modal(emu, &p, true, h)?;
        crate::uikit::after_on_main(emu, duration, present_done_fire, vc)
    } else {
        present_done(emu, vc)
    }
}

/// Slide a presentation's view (the sheet, or the presented view) up or down
/// the screen of height `h`, and fade its dimming; returns how long it takes.
fn slide_modal(emu: &mut Emu, p: &Presentation, up: bool, h: f64) -> Result<f64> {
    use crate::animation::{Prop, Val, transition, transition_spring};
    let moving = if p.sheet != 0 { p.sheet } else { send(emu, p.presented, "view", &[])? };
    let (timing, duration) = transition_spring(if up { PRESENT_SPRING } else { DISMISS_SPRING });
    let c = view(emu, moving).center;
    let top = crate::uiview::frame_of(emu, moving)[1];
    let down = [c[0], c[1] + (h - top)];
    let l = send(emu, moving, "layer", &[])?;
    let (from, to) = if up { (down, c) } else { (c, down) };
    transition(emu, l, Prop::Position, Val::P(from), Val::P(to), timing, duration);
    if p.dim != 0 {
        let dl = send(emu, p.dim, "layer", &[])?;
        let (a, b) = if up { (0.0, 1.0) } else { (1.0, 0.0) };
        view_mut(emu, p.dim).alpha = b;
        transition(emu, dl, Prop::Opacity, Val::N(a), Val::N(b), timing, duration);
    }
    Ok(duration)
}

fn present_done_fire(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(1);
    present_done(emu, vc)
}

/// The presentation is on screen: `did` callbacks, the completion.
fn present_done(emu: &mut Emu, vc: u64) -> Result<()> {
    let Some(i) = emu.nav.presentations.iter().position(|p| p.presented == vc && p.moving == Some(true)) else { return Ok(()) };
    let p = emu.nav.presentations[i].clone();
    emu.nav.presentations[i].moving = None;
    emu.nav.presentations[i].completion = 0;
    let moving = if p.sheet != 0 { p.sheet } else { send(emu, vc, "view", &[])? };
    for v in [moving, p.dim] {
        if v != 0 {
            let l = send(emu, v, "layer", &[])?;
            crate::animation::end_transition(emu, l);
        }
    }
    send(emu, vc, "viewDidAppear:", &[1])?;
    if p.covers {
        // Covered, the screen behind leaves the window (it comes back to be dismissed to).
        let pv = send(emu, p.presenter, "view", &[])?;
        send(emu, pv, "removeFromSuperview", &[])?;
        send(emu, p.presenter, "viewDidDisappear:", &[1])?;
    }
    if p.completion != 0 {
        crate::libsystem::call_block(emu, p.completion, &[])?;
    }
    Ok(())
}

/// `-dismissViewControllerAnimated:completion:`: on the presenting controller
/// (or one inside it), or on the presented one itself.
fn dismiss(emu: &mut Emu) -> Result<()> {
    let (me, animated, completion) = (emu.arg(0), emu.arg(2) & 0xff != 0, emu.arg(3));
    let root = root_of(emu, me);
    let Some(i) = emu.nav.presentations.iter().position(|p| p.presenter == root).or_else(|| emu.nav.presentations.iter().position(|p| p.presented == root)) else {
        return Ok(());
    };
    // Anything presented on top of it goes too, at once.
    while emu.nav.presentations.len() > i + 1 {
        let top = emu.nav.presentations.last().unwrap().presented;
        dismiss_now(emu, top, false)?;
    }
    let mut p = emu.nav.presentations[i].clone();
    if p.moving == Some(false) {
        return Ok(());
    }
    if p.moving == Some(true) {
        present_done(emu, p.presented)?;
        p = emu.nav.presentations[i].clone();
    }
    let completion = if completion != 0 { crate::libc::copy_block(emu, completion)? } else { 0 };
    emu.nav.presentations[i].moving = Some(false);
    emu.nav.presentations[i].completion = completion;
    let a = animated as u64;
    send(emu, p.presented, "viewWillDisappear:", &[a])?;
    if p.covers {
        // The screen behind comes back underneath.
        let pv = send(emu, p.presenter, "view", &[])?;
        let window = view(emu, p.container).superview;
        send(emu, window, "insertSubview:belowSubview:", &[pv, p.container])?;
        send(emu, p.presenter, "viewWillAppear:", &[a])?;
    }
    if animated {
        let h = view(emu, p.container).bounds[3];
        let duration = slide_modal(emu, &p, false, h)?;
        crate::uikit::after_on_main(emu, duration, dismiss_done_fire, p.presented)
    } else {
        dismiss_now(emu, p.presented, true)
    }
}

fn dismiss_done_fire(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(1);
    dismiss_now(emu, vc, true)
}

/// Take a presentation away: `did` callbacks, the links, the completion.
fn dismiss_now(emu: &mut Emu, vc: u64, notify: bool) -> Result<()> {
    let Some(i) = emu.nav.presentations.iter().position(|p| p.presented == vc) else { return Ok(()) };
    let p = emu.nav.presentations.remove(i);
    let moving = if p.sheet != 0 { p.sheet } else { send(emu, vc, "view", &[])? };
    for v in [moving, p.dim] {
        if v != 0 {
            let l = send(emu, v, "layer", &[])?;
            crate::animation::end_transition(emu, l);
        }
    }
    let vview = send(emu, vc, "view", &[])?;
    send(emu, vview, "removeFromSuperview", &[])?;
    send(emu, p.container, "removeFromSuperview", &[])?;
    for v in [p.dim, p.sheet, p.container] {
        if v != 0 {
            send(emu, v, "release", &[])?;
        }
    }
    if notify {
        send(emu, vc, "viewDidDisappear:", &[1])?;
        if p.covers {
            send(emu, p.presenter, "viewDidAppear:", &[1])?;
        }
    }
    if p.completion != 0 {
        crate::libsystem::call_block(emu, p.completion, &[])?;
    }
    send(emu, vc, "release", &[])?;
    Ok(())
}

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
    (VC, false, "isBeingPresented", |e| {
        let vc = e.arg(0);
        let r = e.nav.presentations.iter().any(|p| p.presented == vc && p.moving == Some(true));
        ret(e, r as u64)
    }),
    (VC, false, "isBeingDismissed", |e| {
        let vc = e.arg(0);
        let r = e.nav.presentations.iter().any(|p| p.presented == vc && p.moving == Some(false));
        ret(e, r as u64)
    }),
    (VC, false, "presentViewController:animated:completion:", present),
    (VC, false, "dismissViewControllerAnimated:completion:", dismiss),
    (VC, false, "presentedViewController", |e| {
        let root = root_of(e, e.arg(0));
        let p = e.nav.presentations.iter().find(|p| p.presenter == root).map_or(0, |p| p.presented);
        ret(e, p)
    }),
    (VC, false, "presentingViewController", |e| {
        let root = root_of(e, e.arg(0));
        let p = e.nav.presentations.iter().find(|p| p.presented == root).map_or(0, |p| p.presenter);
        ret(e, p)
    }),
    (VC, false, "tabBarController", |e| ret(e, 0)),
    (VC, false, "splitViewController", |e| ret(e, 0)),
    (VC, false, "beginAppearanceTransition:animated:", |_| Ok(())),
    (VC, false, "nibName", |e| ret(e, 0)),
    (VC, false, "nibBundle", |e| ret(e, 0)),
    (VC, false, "presentationController", |e| ret(e, 0)),
    (VC, false, "sheetPresentationController", |e| ret(e, 0)),
    (VC, false, "popoverPresentationController", |e| ret(e, 0)),
    (VC, false, "setAdditionalSafeAreaInsets:", |_| Ok(())),
    (VC, false, "setPreferredContentSize:", |_| Ok(())),
    (VC, false, "preferredStatusBarUpdateAnimation", |e| ret(e, 1)), // fade
    (VC, false, "childViewControllerForHomeIndicatorAutoHidden", |e| ret(e, 0)),
    (VC, false, "prefersHomeIndicatorAutoHidden", |e| ret(e, 0)),
    (VC, false, "setNeedsUpdateOfHomeIndicatorAutoHidden", |_| Ok(())),
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
    ("UIBarAppearance", false, "init", |e| {
        let a = e.arg(0);
        configure_appearance(e, a, DEFAULT_BACKGROUND)?;
        ret(e, a)
    }),
    ("UIBarAppearance", false, "initWithBarAppearance:", |e| {
        let (a, from) = (e.arg(0), e.arg(2));
        copy_appearance(e, from, a);
        ret(e, a)
    }),
    ("UIBarAppearance", false, "copy", |e| {
        let from = e.arg(0);
        let cls = objc::class_of(e, from)?;
        let a = objc::create_instance(e, cls, 0)?;
        copy_appearance(e, from, a);
        ret(e, a)
    }),
    ("UIBarAppearance", false, "configureWithDefaultBackground", |e| {
        let a = e.arg(0);
        configure_appearance(e, a, DEFAULT_BACKGROUND)
    }),
    ("UIBarAppearance", false, "configureWithOpaqueBackground", |e| {
        let a = e.arg(0);
        configure_appearance(e, a, OPAQUE_BACKGROUND)
    }),
    ("UIBarAppearance", false, "configureWithTransparentBackground", |e| {
        let a = e.arg(0);
        configure_appearance(e, a, TRANSPARENT_BACKGROUND)
    }),
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
