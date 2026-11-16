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

/// The navigation bar's height (below the status bar): 44 pt, 54 under
/// Liquid Glass (iOS 26, measured on its simulator with the largetitles probe).
pub const BAR_HEIGHT: f64 = 44.0;
pub const GLASS_BAR_HEIGHT: f64 = 54.0;
pub const COMPACT_BAR_HEIGHT: f64 = 32.0;
/// A large title: 34-pt bold, 16 pt in, under the bar's usual row, which it
/// makes 52 pt taller; its baseline 16.33 above the bar's bottom under Liquid
/// Glass (measured), 8.67 before.
pub const LARGE_TITLE_SIZE: f64 = 34.0;
pub const LARGE_TITLE_WEIGHT: f64 = 0.4; // bold
pub const LARGE_TITLE_X: f64 = 16.0;
pub const LARGE_TITLE_HEIGHT: f64 = 52.0;

/// The space a large title adds to the bar, and its baseline above the bar's bottom.
pub fn large_title_metrics(emu: &Emu) -> (f64, f64) {
    if emu.system.liquid_glass() { (LARGE_TITLE_HEIGHT, 16.33) } else { (LARGE_TITLE_HEIGHT, 8.67) }
}

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
    /// UINavigationItemLargeTitleDisplayMode: automatic (0), always (1), never (2).
    large_title_mode: u64,
}

#[derive(Clone, Default)]
pub struct State {
    pub(crate) parent: HashMap<u64, u64>,
    pub(crate) children: HashMap<u64, Vec<u64>>,
    navs: HashMap<u64, Nav>,
    /// Container view → its navigation controller.
    containers: HashMap<u64, u64>,
    /// Navigation bar → its navigation controller.
    pub(crate) bars: HashMap<u64, u64>,
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
    /// A finger that may be swiping back from the left edge.
    swipe: Option<Swipe>,
    /// A finger that may be dragging a sheet.
    sheet_drag: Option<SheetDrag>,
    /// View controller → how far its large title has collapsed as its content
    /// scrolled (up to the title's height), or stretched (negative) as it was
    /// pulled down.
    large_title_offsets: HashMap<u64, f64>,
}

/// The back swipe: a touch that began at the left edge over a stack that can go back.
#[derive(Clone, Copy, Debug)]
struct Swipe {
    nc: u64,
    /// Right to left: it starts at the right edge and goes left (fixtures/rtlnav).
    rtl: bool,
    start: [f64; 2],
    /// It became the swipe (the pop is following the finger).
    tracking: bool,
    /// The last two positions and times, for the release velocity.
    last: (f64, f64),
    prev: (f64, f64),
}

/// How far in from the left edge a back swipe can start, and how far the
/// finger moves before it's a swipe (UIKit's edge pan gesture).
const SWIPE_EDGE: f64 = 30.0;
const SWIPE_SLOP: f64 = 10.0;
/// Released, a swipe finishes the pop when it would end up past halfway
/// (where it is, plus where its speed carries it in this long).
const SWIPE_PROJECTION: f64 = 0.2;

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
    /// `modalTransitionStyle` cross-dissolve: it fades in and out where it is, rather than sliding.
    dissolve: bool,
    /// Presenting (true) or dismissing (false) right now, with the completion block to run.
    moving: Option<bool>,
    completion: u64,
    /// A sheet's detents, smallest first: (identifier, its frame), and the one it's at.
    detents: Vec<(String, [f64; 4])>,
    detent: usize,
    /// Dismissed by a drag: the sheet's delegate hears presentationControllerDidDismiss: after.
    dragged_away: bool,
}

/// A finger on a sheet that may drag it.
#[derive(Clone, Copy, Debug)]
struct SheetDrag {
    presented: u64,
    hit: u64,
    start: [f64; 2],
    tracking: bool,
    last: (f64, f64),
    prev: (f64, f64),
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
// Sheet detents, measured on the iPhone 17 Pro and 16e: resolvers get the
// height between the safe area's top and bottom; the medium detent is 0.553
// of it; a sheet smaller than the largest floats 8 pt in from the sides and
// bottom, 20.5 pt taller than its detent, its bottom corners the display's
// less the inset; its grabber is a 34.6 × 4.8 pt pill 4.8 pt from its top.
const MEDIUM_DETENT: f64 = 0.553;
const FLOAT_INSET: f64 = 8.0;
const FLOAT_EXTRA: f64 = 20.5;
const GRABBER: [f64; 2] = [34.6, 4.8];
const GRABBER_TOP: f64 = 4.8;
const GRABBER_COLOR: [f64; 4] = [0.0, 0.0, 0.0, 0.23];
const SHEET_DIMMING: [f64; 4] = [0.0, 0.0, 0.0, 0.2];

/// A change of the top controller: what's left to do when its transition settles.
#[derive(Clone)]
struct Transition {
    nc: u64,
    /// The stack before the change (its top's bar shows while the old screen leaves).
    old_stack: Vec<u64>,
    /// A push (true) or pop.
    forward: bool,
    /// The two screens' views while they move (cards with the display's corners).
    screens: Vec<u64>,
    old_top: Option<u64>,
    new_top: Option<u64>,
    removed: Vec<u64>,
    added: Vec<u64>,
    animated: bool,
    /// Views the transition animates (their transition animations end with it), and its dimming view.
    moving: Vec<u64>,
    dim: u64,
    /// The shadow cards under the moving screens.
    cards: Vec<u64>,
    /// What moves, placed by the transition's progress: (layer, its model
    /// centre, offset at progress 0 and at 1, in widths).
    pieces: Vec<(u64, [f64; 2], f64, f64)>,
    /// The dimming view's layer and its opacity at progress 0 and 1.
    dim_fade: (u64, f64, f64),
    width: f64,
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
/// Each screen moves on a card casting a faint shadow: (opacity, radius, offset y).
const CARD_SHADOW: (f32, f64, f64) = (0.04, 12.0, -3.0);

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

/// A navigation controller's stack, if it's one.
pub fn stack_of(emu: &Emu, nc: u64) -> Option<Vec<u64>> {
    emu.nav.navs.get(&nc).map(|n| n.stack.clone())
}

fn nav(emu: &mut Emu, nc: u64) -> Nav {
    emu.nav.navs.entry(nc).or_insert_with(|| Nav { translucent: true, ..Default::default() }).clone()
}

fn nav_mut(emu: &mut Emu, nc: u64) -> &mut Nav {
    emu.nav.navs.entry(nc).or_insert_with(|| Nav { translucent: true, ..Default::default() })
}

// ---- containment ----

/// An appearance callback (`viewWillAppear:` …) on `vc` and, as UIKit forwards
/// them, its visible children: a navigation controller's top, a tab bar
/// controller's selected tab, every child of other containers.
pub fn appearance(emu: &mut Emu, vc: u64, sel: &str, animated: bool) -> Result<()> {
    send(emu, vc, sel, &[animated as u64])?;
    let children = emu.nav.children.get(&vc).cloned().unwrap_or_default();
    let visible: Vec<u64> = if emu.nav.navs.contains_key(&vc) {
        emu.nav.navs[&vc].stack.last().copied().into_iter().collect()
    } else if let Some(selected) = crate::tabbar::selected_child(emu, vc) {
        selected.into_iter().collect()
    } else {
        children
    };
    for c in visible {
        appearance(emu, c, sel, animated)?;
    }
    Ok(())
}

pub(crate) fn add_child(emu: &mut Emu, parent: u64, child: u64) -> Result<()> {
    if child == 0 || emu.nav.parent.get(&child) == Some(&parent) {
        return Ok(());
    }
    remove_from_parent(emu, child)?;
    send(emu, child, "willMoveToParentViewController:", &[parent])?;
    emu.nav.parent.insert(child, parent);
    emu.nav.children.entry(parent).or_default().push(child);
    Ok(())
}

/// Take a child out of its parent's children.
pub(crate) fn remove_child(emu: &mut Emu, child: u64) -> Result<()> {
    remove_from_parent(emu, child)
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
    let bar_h = nav_bar_height(emu, nc)?;
    let bar_bottom = safe_top + bar_h;
    // (Pulled down, the bar stretches past the space it takes.)
    let stretch = match n.stack.last() {
        Some(&top) if shows_large_title(emu, nc)? => (-large_title_offset(emu, top)).max(0.0),
        _ => 0.0,
    };
    set_frame_of(emu, n.bar, [0.0, safe_top, w, bar_h + stretch]);
    view_mut(emu, n.bar).hidden = n.bar_hidden;
    crate::baritems::layout_toolbar(emu, nc, container, n.stack.last().copied(), w, h)?;
    // (The bar's custom item views go into it as it's laid out, not only when drawn.)
    if !n.bar_hidden {
        bar_look(emu, n.bar)?;
    }
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
    change_stack(emu, nc, stack, animated, false)
}

/// As [`set_stack`]; `interactive`: the pop's pieces wait for a finger to move them.
fn change_stack(emu: &mut Emu, nc: u64, stack: Vec<u64>, animated: bool, interactive: bool) -> Result<()> {
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
    // Appearance callbacks only while it's on screen (in a window); otherwise
    // the window's own appearance pass tells the top controller later.
    let shown = view(emu, container).window != 0;
    if old_top != new_top {
        if let Some(nt) = new_top {
            delegate_call(emu, delegate, "navigationController:willShowViewController:animated:", &[nc, nt, a])?;
            if shown {
                send(emu, nt, "viewWillAppear:", &[a])?;
            }
        }
        if let Some(ot) = old_top
            && shown
        {
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
    // In a tab, a pushed screen may hide the tab bar (or bring it back).
    crate::tabbar::navigation_changed(emu, nc)?;
    let mut t = Transition {
        nc,
        old_stack: old.clone(),
        forward,
        screens: Vec::new(),
        old_top,
        new_top,
        removed,
        added,
        animated,
        moving: Vec::new(),
        dim: 0,
        cards: Vec::new(),
        pieces: Vec::new(),
        dim_fade: (0, 0.0, 0.0),
        width: 0.0,
    };
    view_mut(emu, container).needs_layout = true;
    match (old_top, new_top) {
        (Some(ot), Some(nt)) if interactive && ot != nt => {
            chrome(emu, container, ot, nt, forward, &mut t)?;
            place(emu, &t, 0.0, 0.0, crate::animation::Timing::Linear, 1e-6);
            emu.nav.transitions.insert(nc, t);
            Ok(())
        }
        (Some(ot), Some(nt)) if animated && ot != nt => {
            chrome(emu, container, ot, nt, forward, &mut t)?;
            let (timing, duration) = crate::animation::transition_spring(if forward { PUSH_SPRING } else { POP_SPRING });
            place(emu, &t, 0.0, 1.0, timing, duration);
            emu.nav.transitions.insert(nc, t);
            crate::animation::after_commit(emu, duration, finish_fire, nc)
        }
        _ => finish(emu, t),
    }
}

/// Put a transition's pieces where progress `from` puts them, moving to
/// progress `to` (the same for a finger holding them still).
fn place(emu: &mut Emu, t: &Transition, from: f64, to: f64, timing: crate::animation::Timing, duration: f64) {
    use crate::animation::{Prop, Val, transition};
    let w = t.width;
    for &(l, c, at0, at1) in &t.pieces {
        let x = |p: f64| c[0] + w * (at0 + (at1 - at0) * p);
        transition(emu, l, Prop::Position, Val::P([x(from), c[1]]), Val::P([x(to), c[1]]), timing, duration);
    }
    let (dl, d0, d1) = t.dim_fade;
    if dl != 0 {
        let o = |p: f64| d0 + (d1 - d0) * p;
        transition(emu, dl, Prop::Opacity, Val::N(o(from)), Val::N(o(to)), timing, duration);
    }
}

/// Make a push or pop's chrome: the dimming view over the screen going
/// underneath, and a card casting the faint shadow under each moving screen;
/// and record where each piece goes as the transition progresses.
fn chrome(emu: &mut Emu, container: u64, ot: u64, nt: u64, forward: bool, t: &mut Transition) -> Result<()> {
    let (ov, nv) = (send(emu, ot, "view", &[])?, send(emu, nt, "view", &[])?);
    let w = view(emu, container).bounds[2];
    t.width = w;
    // The view going underneath is dimmed: the old one on a push, the new one on a pop.
    let under = if forward { ov } else { nv };
    let frame = crate::uiview::frame_of(emu, under);
    let dim = new_view(emu, frame)?;
    let black = crate::uiview::new_color(emu, DIMMING)?;
    send(emu, dim, "setBackgroundColor:", &[black])?;
    view_mut(emu, dim).user_interaction = false;
    send(emu, container, "insertSubview:aboveSubview:", &[dim, under])?;
    let (d0, d1) = if forward { (0.0, 1.0) } else { (1.0, 0.0) };
    view_mut(emu, dim).alpha = d1;
    let dl = send(emu, dim, "layer", &[])?;
    t.dim_fade = (dl, d0, d1);
    t.moving.push(dl);
    t.dim = dim;
    // Each screen's offset (in widths) at the start and end: from the
    // trailing side, the left one in right to left (fixtures/rtlnav).
    let moves = if forward { [(nv, 1.0, 0.0), (ov, 0.0, -PARALLAX)] } else { [(ov, 0.0, 1.0), (nv, -PARALLAX, 0.0)] };
    let sign = if crate::direction::view_rtl(emu, container)? { -1.0 } else { 1.0 };
    for (v, at0, at1) in moves {
        let (at0, at1) = (sign * at0, sign * at1);
        // Its card: the screen's shape, casting the shadow, just under it.
        let frame = crate::uiview::frame_of(emu, v);
        let card = new_view(emu, frame)?;
        view_mut(emu, card).user_interaction = false;
        let cl = send(emu, card, "layer", &[])?;
        let path = crate::render::new_card_path(emu, [frame[2], frame[3]], emu.device.corner_radius)?;
        {
            let ly = crate::calayer::layer_mut(emu, cl);
            ly.shadow_opacity = CARD_SHADOW.0;
            ly.shadow_radius = CARD_SHADOW.1;
            ly.shadow_offset = [0.0, CARD_SHADOW.2];
            ly.shadow_path = path;
        }
        send(emu, container, "insertSubview:belowSubview:", &[card, v])?;
        let l = send(emu, v, "layer", &[])?;
        let c = view(emu, v).center;
        t.pieces.push((l, c, at0, at1));
        t.pieces.push((cl, c, at0, at1));
        t.moving.extend([l, cl]);
        t.cards.push(card);
        t.screens.push(v);
    }
    Ok(())
}

// ---- the back swipe ----

/// A finger went down at `p` (window coordinates): it may become a back swipe.
pub(crate) fn swipe_began(emu: &mut Emu, p: [f64; 2], now: f64) -> Result<()> {
    emu.nav.swipe = None;
    let rtl = crate::direction::app_rtl(emu)?;
    // The navigation controllers under the finger that can go back; the innermost wins.
    let mut best: Option<(u64, usize)> = None;
    let navs: Vec<(u64, Nav)> = emu.nav.navs.iter().map(|(&k, v)| (k, v.clone())).collect();
    for (nc, n) in navs {
        if n.stack.len() < 2 || n.container == 0 || emu.nav.transitions.contains_key(&nc) {
            continue;
        }
        let cv = view(emu, n.container);
        if cv.window == 0 || cv.hidden {
            continue;
        }
        // Covered by a presentation it isn't inside.
        if let Some(top) = emu.nav.presentations.last().map(|p| p.container)
            && !is_inside(emu, n.container, top)
        {
            continue;
        }
        if n.pop_gesture != 0 && objc::bag_get(emu, n.pop_gesture, "enabled").is_some_and(|e| e & 0xff == 0) {
            continue;
        }
        let o = crate::uiview::window_origin(emu, n.container);
        let inside = p[0] >= o[0] && p[1] >= o[1] && p[0] < o[0] + cv.bounds[2] && p[1] < o[1] + cv.bounds[3];
        // From the leading edge: the left one, the right one in right to left.
        let from_edge = if rtl { o[0] + cv.bounds[2] - p[0] } else { p[0] - o[0] };
        let inside = inside && from_edge <= SWIPE_EDGE;
        let depth = depth_of(emu, n.container);
        if inside && best.is_none_or(|(_, d)| depth > d) {
            best = Some((nc, depth));
        }
    }
    if let Some((nc, _)) = best {
        emu.nav.swipe = Some(Swipe { nc, rtl, start: p, tracking: false, last: (p[0], now), prev: (p[0], now) });
    }
    Ok(())
}

fn is_inside(emu: &mut Emu, mut v: u64, ancestor: u64) -> bool {
    while v != 0 {
        if v == ancestor {
            return true;
        }
        v = view(emu, v).superview;
    }
    false
}

fn depth_of(emu: &mut Emu, mut v: u64) -> usize {
    let mut d = 0;
    while v != 0 {
        d += 1;
        v = view(emu, v).superview;
    }
    d
}

/// Whether a back swipe has the touch (the app's views don't get it).
pub(crate) fn swiping(emu: &Emu) -> bool {
    emu.nav.swipe.is_some_and(|s| s.tracking)
}

/// The finger moved. Ok(true): it just became the swipe (the view under it loses the touch).
pub(crate) fn swipe_moved(emu: &mut Emu, p: [f64; 2], now: f64) -> Result<bool> {
    let Some(mut sw) = emu.nav.swipe else { return Ok(false) };
    sw.prev = sw.last;
    sw.last = (p[0], now);
    let (dx, dy) = (p[0] - sw.start[0], p[1] - sw.start[1]);
    let dx = if sw.rtl { -dx } else { dx };
    let mut began = false;
    if !sw.tracking {
        if dx.abs() < SWIPE_SLOP && dy.abs() < SWIPE_SLOP {
            emu.nav.swipe = Some(sw);
            return Ok(false);
        }
        if dx < SWIPE_SLOP || dy.abs() > dx {
            // Not a swipe back: the touch is the app's.
            emu.nav.swipe = None;
            return Ok(false);
        }
        // It's a swipe: start the pop, following the finger.
        let stack = nav(emu, sw.nc).stack;
        change_stack(emu, sw.nc, stack[..stack.len() - 1].to_vec(), true, true)?;
        sw.tracking = true;
        began = true;
    }
    emu.nav.swipe = Some(sw);
    if let Some(t) = emu.nav.transitions.get(&sw.nc).cloned() {
        let progress = (dx / t.width.max(1.0)).clamp(0.0, 1.0);
        place(emu, &t, progress, progress, crate::animation::Timing::Linear, 1e-6);
    }
    Ok(began)
}

/// The finger lifted (or the touch was cancelled): finish the pop or put it back.
pub(crate) fn swipe_ended(emu: &mut Emu, p: [f64; 2], now: f64, cancelled: bool) -> Result<()> {
    let Some(sw) = emu.nav.swipe.take() else { return Ok(()) };
    if !sw.tracking {
        return Ok(());
    }
    let Some(t) = emu.nav.transitions.get(&sw.nc).cloned() else { return Ok(()) };
    let w = t.width.max(1.0);
    let sign = if sw.rtl { -1.0 } else { 1.0 };
    let progress = (sign * (p[0] - sw.start[0]) / w).clamp(0.0, 1.0);
    let (x0, t0) = if (now - sw.last.1).abs() > 1e-6 { sw.last } else { sw.prev };
    let velocity = if now > t0 { sign * (p[0] - x0) / (now - t0) } else { 0.0 };
    let finish = !cancelled && progress + velocity / w * SWIPE_PROJECTION > 0.5;
    let (omega, zeta, _) = POP_SPRING;
    // The spring starts at the finger's speed, as a fraction of the distance left per second.
    let (to, left) = if finish { (1.0, w * (1.0 - progress)) } else { (0.0, w * progress) };
    let v = if left > 0.5 { velocity * if finish { 1.0 } else { -1.0 } / left } else { 0.0 };
    let (timing, duration) = crate::animation::transition_spring((omega, zeta, v.max(0.0)));
    place(emu, &t, progress, to, timing, duration);
    if finish {
        crate::animation::after_commit(emu, duration, finish_fire, sw.nc)
    } else {
        crate::animation::after_commit(emu, duration, cancel_fire, sw.nc)
    }
}

fn cancel_fire(emu: &mut Emu) -> Result<()> {
    let nc = emu.arg(1);
    let Some(t) = emu.nav.transitions.remove(&nc) else { return Ok(()) };
    cancel(emu, t)
}

/// A swipe back let go short: everything back as it was, with UIKit's
/// callbacks for a cancelled transition.
fn cancel(emu: &mut Emu, t: Transition) -> Result<()> {
    let Transition { nc, old_stack, old_top, new_top, removed, added, moving, dim, cards, .. } = t;
    for l in moving {
        crate::animation::end_transition(emu, l);
    }
    for v in cards.into_iter().chain([dim]) {
        if v != 0 {
            send(emu, v, "removeFromSuperview", &[])?;
            send(emu, v, "release", &[])?;
        }
    }
    nav_mut(emu, nc).stack = old_stack;
    // The screen it was going back to leaves again; the one it was leaving stays.
    if let Some(nt) = new_top {
        send(emu, nt, "viewWillDisappear:", &[1])?;
        let v = send(emu, nt, "view", &[])?;
        send(emu, v, "removeFromSuperview", &[])?;
        send(emu, nt, "viewDidDisappear:", &[1])?;
    }
    if let Some(ot) = old_top {
        send(emu, ot, "viewWillAppear:", &[1])?;
        send(emu, ot, "viewDidAppear:", &[1])?;
    }
    for &vc in &removed {
        emu.nav.moving_from.remove(&vc);
        send(emu, vc, "didMoveToParentViewController:", &[nc])?;
    }
    for &vc in &added {
        emu.nav.moving_to.remove(&vc);
    }
    let container = nav(emu, nc).container;
    layout_container(emu, container)?;
    Ok(())
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
    let Transition { nc, old_top, new_top, removed, added, animated, moving, dim, cards, .. } = t;
    for l in moving {
        crate::animation::end_transition(emu, l);
    }
    for card in cards {
        send(emu, card, "removeFromSuperview", &[])?;
        send(emu, card, "release", &[])?;
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
    let container = nav(emu, nc).container;
    let shown = container != 0 && view(emu, container).window != 0;
    if old_top != new_top {
        if let Some(ot) = old_top {
            // Still on the stack (pushed over), or not: either way its view leaves the container.
            let v = send(emu, ot, "view", &[])?;
            send(emu, v, "removeFromSuperview", &[])?;
            if shown {
                send(emu, ot, "viewDidDisappear:", &[a])?;
            }
        }
        if let Some(nt) = new_top {
            if shown {
                send(emu, nt, "viewDidAppear:", &[a])?;
            }
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
    /// The title shown large, under the bar's usual row.
    pub large: bool,
    /// The height of the bar's usual row (the back button, a small title).
    pub row: f64,
    /// Right to left: the back button on the right, its chevron pointing right.
    pub rtl: bool,
    /// The top screen's bar button items (baritems.rs), in the bar's coordinates.
    pub items: Vec<crate::baritems::BarItem>,
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

pub(crate) fn text_width(emu: &mut Emu, text: &str, weight: f64) -> f64 {
    crate::textkit::line_glyphs(emu, text, TITLE_SIZE, weight, [0.0; 4], 0.0, 0.0).1
}

/// How a navigation bar looks now, if it belongs to a navigation controller.
/// A screen moving in a push or pop: the corner radius its card has (the display's).
pub fn card_corner(emu: &Emu, v: u64) -> Option<f64> {
    emu.nav.transitions.values().any(|t| t.screens.contains(&v)).then_some(emu.device.corner_radius)
}

/// One screen's part of a navigation bar: its look, how far its title has
/// moved, how visible the back button is (None: the title alone), and where
/// the screen above it covers it from (x in the bar), if it's underneath.
pub type BarPart = (BarLook, f64, Option<f64>, Option<f64>);

/// What a navigation bar shows. During a push or pop (iOS 26) each screen's
/// title rides with its screen, the lower screen's under the upper one's
/// card, and the glass back button fades with the transition's progress.
pub fn bar_parts(emu: &mut Emu, bar: u64) -> Result<Vec<BarPart>> {
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(vec![]) };
    let stack = nav(emu, nc).stack;
    let Some(now) = look_for(emu, bar, &stack)? else { return Ok(vec![]) };
    let Some(t) = emu.nav.transitions.get(&nc).cloned() else { return Ok(vec![(now, 0.0, Some(1.0), None)]) };
    let (Some(ot), Some(nt)) = (t.old_top, t.new_top) else { return Ok(vec![(now, 0.0, Some(1.0), None)]) };
    let Some(before) = look_for(emu, bar, &t.old_stack)? else { return Ok(vec![(now, 0.0, Some(1.0), None)]) };
    let w = view(emu, bar).bounds[2].max(1.0);
    let offset = |emu: &mut Emu, vc: u64| -> Result<f64> {
        let v = send(emu, vc, "view", &[])?;
        Ok(crate::animation::presented_center(emu, v)[0] - view(emu, v).center[0])
    };
    let (dx_old, dx_new) = (offset(emu, ot)?, offset(emu, nt)?);
    // How far along it is: the incoming card's travel on a push, the leaving one's on a pop.
    let progress = if t.forward { 1.0 - dx_new.abs() / w } else { dx_old.abs() / w }.clamp(0.0, 1.0);
    let (b0, b1) = (before.back.is_some() as u8 as f64, now.back.is_some() as u8 as f64);
    let back = b0 + (b1 - b0) * progress;
    // The upper screen's card edge: the incoming one on a push, the leaving one on a pop.
    // The back button is drawn with the upper screen's part (it has one: it's not the root).
    Ok(if t.forward {
        vec![(before, dx_old, None, Some(dx_new)), (now, dx_new, Some(back), None)]
    } else {
        vec![(now, dx_new, None, Some(dx_old)), (before, dx_old, Some(back), None)]
    })
}

/// The navigation bars there are (for tests and tools).
pub fn bars(emu: &Emu) -> Vec<u64> {
    let mut b: Vec<u64> = emu.nav.bars.keys().copied().collect();
    b.sort();
    b
}

pub fn bar_look(emu: &mut Emu, bar: u64) -> Result<Option<BarLook>> {
    let Some(&nc) = emu.nav.bars.get(&bar) else { return Ok(None) };
    let stack = nav(emu, nc).stack;
    look_for(emu, bar, &stack)
}

/// The bar as it looks for this stack.
fn look_for(emu: &mut Emu, bar: u64, stack: &[u64]) -> Result<Option<BarLook>> {
    let stack = stack.to_vec();
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
    // The large title until it has collapsed; then the bar's own.
    let large = match emu.nav.bars.get(&bar).copied() {
        Some(nc) if nav(emu, nc).stack.last() == Some(&top) => shows_large_title(emu, nc)? && large_title_offset(emu, top) < LARGE_TITLE_HEIGHT,
        _ => false,
    };
    // (Under Liquid Glass the bar is 10 pt taller below its row, which is
    // still 44: measured, the small title's baseline 28 below the bar's top.)
    let row = if compact_bar(emu) { COMPACT_BAR_HEIGHT } else { BAR_HEIGHT };
    let rtl = crate::direction::view_rtl(emu, bar)?;
    if rtl && let Some(b) = &mut back {
        // (Mirrored: fixtures/rtlnav, the glass button at 330 on a 390-pt bar.)
        b.hit.0 = bar_width(emu, bar) - b.hit.0 - b.hit.1;
    }
    let width = bar_width(emu, bar);
    let mut items = crate::baritems::layout(emu, top_item, width, leading, back.is_some(), glass)?;
    if rtl {
        crate::baritems::mirror(&mut items, width);
    }
    // (The custom views of the screen the bar shows now are in it.)
    let nc = emu.nav.bars.get(&bar).copied();
    if nc.is_some_and(|nc| nav(emu, nc).stack.last() == Some(&top)) {
        crate::baritems::place_custom(emu, bar, &items, 0.0)?;
    }
    Ok(Some(BarLook { title, back, glass, background, shadow, leading, large, row, rtl, items }))
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
    let look = bar_look(emu, bar)?;
    if let Some(l) = &look
        && crate::baritems::tap(emu, &l.items, x, y)?
    {
        return Ok(());
    }
    let Some(Back { hit: (bx, bw), .. }) = look.and_then(|l| l.back) else { return Ok(()) };
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
    // Inside a tab bar controller (not its bar), a visible tab bar (and its accessory) covers the bottom.
    let mut bottom = bottom;
    let (mut cur, mut below) = (v, 0);
    while cur != 0 {
        if let Some(cover) = crate::tabbar::bottom_cover(emu, cur, below) {
            bottom += cover;
            break;
        }
        below = cur;
        cur = view(emu, cur).superview;
    }
    // Inside a navigation controller with a visible bar, the bar covers more
    // (its screens; not its own view, nor the bar).
    let mut cur = v;
    while cur != 0 {
        if let Some(&nc) = emu.nav.containers.get(&cur) {
            let n = nav(emu, nc);
            if cur != v && !n.bar_hidden && !is_inside(emu, v, n.bar) {
                top += nav_bar_height(emu, nc).unwrap_or(BAR_HEIGHT);
            }
            // And a visible toolbar the bottom (not its own content).
            let toolbar = crate::baritems::toolbar_of(emu, nc);
            if cur != v && toolbar.is_none_or(|t| !is_inside(emu, v, t)) {
                bottom += crate::baritems::toolbar_covers(emu, nc, n.stack.last().copied());
            }
            break;
        }
        cur = view(emu, cur).superview;
    }
    [left, top, w - right, h - bottom]
}

/// A navigation bar's height: 44 pt (54 under Liquid Glass), or UIKit's
/// compact 32 pt where the vertical size class is compact (phones in landscape).
pub fn bar_height(emu: &Emu) -> f64 {
    if compact_bar(emu) {
        COMPACT_BAR_HEIGHT
    } else if emu.system.liquid_glass() {
        GLASS_BAR_HEIGHT
    } else {
        BAR_HEIGHT
    }
}

fn compact_bar(emu: &Emu) -> bool {
    crate::orientation::size_classes(emu).1 == crate::device::COMPACT
}

/// Whether the navigation controller's bar shows its top item's title large:
/// the bar prefers large titles, and the item says always, or automatic with
/// the item below it large (the root's automatic is large), as UIKit decides.
pub fn shows_large_title(emu: &mut Emu, nc: u64) -> Result<bool> {
    let n = nav(emu, nc);
    if compact_bar(emu) || n.bar == 0 || objc::bag_get(emu, n.bar, "prefersLargeTitles").is_none_or(|v| v & 0xff == 0) {
        return Ok(false);
    }
    let mut large = true;
    for vc in n.stack {
        let item = item_of(emu, vc)?;
        match item_state(emu, item).large_title_mode {
            1 => large = true,
            2 => large = false,
            _ => {}
        }
    }
    Ok(large)
}

/// The space the bar takes over the navigation controller's top screen:
/// taller with a large title, less so as it collapses.
pub fn nav_bar_height(emu: &mut Emu, nc: u64) -> Result<f64> {
    let base = bar_height(emu);
    let Some(&top) = nav(emu, nc).stack.last() else { return Ok(base) };
    if !shows_large_title(emu, nc)? {
        return Ok(base);
    }
    Ok(base + LARGE_TITLE_HEIGHT - large_title_offset(emu, top).clamp(0.0, LARGE_TITLE_HEIGHT))
}

fn large_title_offset(emu: &Emu, vc: u64) -> f64 {
    emu.nav.large_title_offsets.get(&vc).copied().unwrap_or(0.0)
}

// ---- the status bar ----

/// The controller that decides the status bar for the window whose root is
/// `root`: the last one presented full screen (or one that captures the status
/// bar's appearance), else the root.
pub(crate) fn status_bar_root(emu: &mut Emu, root: u64) -> u64 {
    let presented: Vec<(u64, bool)> = emu.nav.presentations.iter().map(|p| (p.presented, p.sheet == 0)).collect();
    for (vc, full_screen) in presented.into_iter().rev() {
        let captures = emu.nav.props.get(&(vc, "modalPresentationCapturesStatusBarAppearance")).is_some_and(|&v| v & 0xff != 0);
        if full_screen || captures {
            return vc;
        }
    }
    root
}

/// The child a navigation controller defers the status bar to (None: `vc`
/// isn't one): its top controller for whether it's hidden, and for its style
/// while the navigation bar is hidden (else the bar's own, the default).
pub(crate) fn status_bar_child(emu: &mut Emu, vc: u64, for_style: bool) -> Option<u64> {
    let n = emu.nav.navs.get(&vc)?.clone();
    let top = n.stack.last().copied().unwrap_or(0);
    Some(if for_style && !n.bar_hidden { 0 } else { top })
}

// ---- large titles and the content under them ----
//
// A large title belongs to its screen's content (measured on the iOS 26
// simulator, the largetitles probe). The bar follows the content's top as the
// user scrolls: up, the title collapses point for point until the bar is its
// usual height; pulled down, the bar stretches. The screen's safe area (and
// so its scroll view's adjusted inset) follows the collapse, so the content
// stays where the finger has it. Let go half collapsed, the content settles
// where the title is whole or gone, whichever is nearer. Moved by the app,
// the content doesn't drag the title: it collapses once the content's top
// passes the usual bar, opens once it's back below it, and the content keeps
// its distance from where it rests.

/// The scroll view a navigation controller's bar follows: its top
/// controller's view, or the first scroll view down its first subviews.
fn content_scroll_view(emu: &mut Emu, nc: u64) -> u64 {
    let Some(&top) = nav(emu, nc).stack.last() else { return 0 };
    let mut v = emu.ui.views.vc_view.get(&top).copied().unwrap_or(0);
    while v != 0 {
        if crate::scrollview::is_scroll_view(emu, v) {
            return v;
        }
        v = view(emu, v).subviews.first().copied().unwrap_or(0);
    }
    0
}

/// The navigation controller (and its top controller) whose bar follows `v`.
fn observer_of(emu: &mut Emu, v: u64) -> Option<(u64, u64)> {
    let mut cur = view(emu, v).superview;
    while cur != 0 {
        if let Some(&nc) = emu.nav.containers.get(&cur) {
            let top = *nav(emu, nc).stack.last()?;
            return (content_scroll_view(emu, nc) == v).then_some((nc, top));
        }
        cur = view(emu, cur).superview;
    }
    None
}

/// Whether `v` is the scroll view a navigation controller's bar follows.
pub(crate) fn is_content_scroll_view(emu: &mut Emu, v: u64) -> bool {
    observer_of(emu, v).is_some()
}

/// How far `v`'s content is past where it rests under the whole large title,
/// at vertical offset `y`.
fn past_full_rest(emu: &mut Emu, v: u64, y: f64) -> f64 {
    let full_bottom = crate::orientation::safe_area(emu)[0] + bar_height(emu) + LARGE_TITLE_HEIGHT;
    let top = crate::uiview::window_origin(emu, v)[1];
    let own = crate::scrollview::content_inset(emu, v)[0];
    y + own + full_bottom - top
}

/// Where `v`'s content rests under its whole large title, if a large title
/// follows it: it can be scrolled up to there without stretching, the title
/// opening as it comes.
pub(crate) fn large_title_rest(emu: &mut Emu, v: u64) -> Result<Option<f64>> {
    let Some((nc, _)) = observer_of(emu, v) else { return Ok(None) };
    if !shows_large_title(emu, nc)? {
        return Ok(None);
    }
    Ok(Some(-past_full_rest(emu, v, 0.0)))
}

/// `v`'s content moved: the large title over it follows.
pub(crate) fn content_scrolled(emu: &mut Emu, v: u64) -> Result<()> {
    let Some((nc, vc)) = observer_of(emu, v) else { return Ok(()) };
    if !shows_large_title(emu, nc)? {
        return Ok(());
    }
    let before = large_title_offset(emu, vc);
    let y = view(emu, v).bounds[1];
    let past = past_full_rest(emu, v, y);
    let user = crate::scrollview::user_scrolling(emu, v);
    let after = if user {
        past.min(LARGE_TITLE_HEIGHT)
    } else if past >= LARGE_TITLE_HEIGHT {
        LARGE_TITLE_HEIGHT
    } else if before >= LARGE_TITLE_HEIGHT {
        0.0
    } else {
        before.max(0.0)
    };
    if after == before {
        return Ok(());
    }
    emu.nav.large_title_offsets.insert(vc, after);
    let c = nav(emu, nc).container;
    if c != 0 {
        layout_container(emu, c)?;
    }
    if user {
        // The content stays under the finger: its insets moved with it.
        crate::scrollview::keep_insets(emu, v)?;
    }
    Ok(())
}

/// Where content let go to rest at vertical offset `target` settles instead:
/// not with its large title half collapsed.
pub(crate) fn settle_large_title(emu: &mut Emu, v: u64, target: f64) -> Result<f64> {
    let Some((nc, _)) = observer_of(emu, v) else { return Ok(target) };
    if !shows_large_title(emu, nc)? {
        return Ok(target);
    }
    let past = past_full_rest(emu, v, target);
    Ok(if past <= 0.0 || past >= LARGE_TITLE_HEIGHT {
        target
    } else if past < LARGE_TITLE_HEIGHT / 2.0 {
        target - past
    } else {
        target + LARGE_TITLE_HEIGHT - past
    })
}

/// Lay out the controller whose bar this is (its height may have changed).
fn relayout_bar(emu: &mut Emu, bar: u64) -> Result<()> {
    if let Some(&nc) = emu.nav.bars.get(&bar) {
        let c = nav(emu, nc).container;
        if c != 0 {
            layout_container(emu, c)?;
        }
    }
    Ok(())
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

/// How a sheet shows: its height when it floats (None: the large detent,
/// attached), whether the screen behind stays undimmed, and the grabber.
struct SheetDetent {
    height: Option<f64>,
    undimmed: bool,
    grabber: bool,
    /// Every detent's sheet height, smallest first (None: large, attached), and which is shown.
    all: Vec<(String, Option<f64>)>,
    shown: usize,
}

/// What `UISheetPresentationController` says for `vc`, its detents resolved
/// against `max` (the height between the safe area's top and bottom).
fn sheet_detent(emu: &mut Emu, vc: u64, max: f64) -> Result<SheetDetent> {
    let sc = emu.nav.props.get(&(vc, "pomme.sheet")).copied().unwrap_or(0);
    if sc == 0 {
        return Ok(SheetDetent { height: None, undimmed: false, grabber: false, all: vec![(String::new(), None)], shown: 0 });
    }
    let grabber = crate::objc::bag_get(emu, sc, "prefersGrabberVisible").is_some_and(|g| g & 0xff != 0);
    let list = crate::objc::bag_get(emu, sc, "detents").unwrap_or(0);
    let n = if list != 0 { send(emu, list, "count", &[])? } else { 0 };
    let mut detents = Vec::new();
    for i in 0..n {
        let d = send(emu, list, "objectAtIndex:", &[i])?;
        let id = crate::objc::bag_get(emu, d, "identifier").map(|s| foundation::nsstring(emu, s).unwrap_or_default()).unwrap_or_default();
        detents.push((id, detent_value(emu, d, max)?));
    }
    if detents.is_empty() {
        return Ok(SheetDetent { height: None, undimmed: false, grabber, all: vec![(String::new(), None)], shown: 0 });
    }
    let id_of = |emu: &mut Emu, key: &str| crate::objc::bag_get(emu, sc, key).filter(|&s| s != 0).map(|s| foundation::nsstring(emu, s).unwrap_or_default());
    // It shows at the selected detent, else the smallest.
    let selected = id_of(emu, "selectedDetentIdentifier");
    let (_, value) = selected
        .and_then(|s| detents.iter().find(|d| d.0 == s).cloned())
        .unwrap_or_else(|| detents.iter().cloned().fold((String::new(), f64::MAX), |a, b| if b.1 < a.1 { b } else { a }));
    let undimmed = match id_of(emu, "largestUndimmedDetentIdentifier") {
        Some(u) => detents.iter().find(|d| d.0 == u).is_some_and(|d| value <= d.1 + 0.5),
        None => false,
    };
    let height_of = |v: f64| (v < max - 0.5).then_some(v + FLOAT_EXTRA);
    let height = height_of(value);
    let mut all: Vec<(String, f64)> = detents.clone();
    all.sort_by(|a, b| a.1.total_cmp(&b.1));
    let shown = all.iter().position(|d| (d.1 - value).abs() < 1e-6).unwrap_or(0);
    let all = all.into_iter().map(|(id, v)| (id, height_of(v))).collect();
    Ok(SheetDetent { height, undimmed, grabber, all, shown })
}

/// A sheet's frame at a detent of `height` (None: large, attached) in a `w` × `h` window.
fn detent_frame(height: Option<f64>, w: f64, h: f64, top: f64) -> [f64; 4] {
    match height {
        Some(sh) => [FLOAT_INSET, h - FLOAT_INSET - sh, w - 2.0 * FLOAT_INSET, sh],
        None => [0.0, top, w, h - top],
    }
}

/// A detent's height: medium, large, or what its resolver returns.
fn detent_value(emu: &mut Emu, d: u64, max: f64) -> Result<f64> {
    match crate::objc::bag_get(emu, d, "pomme.kind").unwrap_or(2) {
        1 => Ok(MEDIUM_DETENT * max),
        3 => {
            let resolver = crate::objc::bag_get(emu, d, "pomme.resolver").unwrap_or(0);
            if resolver == 0 {
                return Ok(max);
            }
            let cls = crate::objc::host_class(emu, "_PommeDetentResolutionContext")?;
            let ctx = crate::objc::create_instance(emu, cls, 0)?;
            emu.nav.props.insert((ctx, "maximumDetentValue"), max.to_bits());
            crate::libsystem::call_block(emu, resolver, &[ctx])?;
            let v = f64::from_bits(emu.last_q0 as u64);
            Ok(if v.is_finite() && v > 0.0 { v.min(max) } else { max })
        }
        _ => Ok(max),
    }
}

/// A detent object of `kind` (1 medium, 2 large, 3 custom) with this identifier.
fn new_detent(emu: &mut Emu, kind: u64, id: &str, resolver: u64) -> Result<u64> {
    let cls = crate::objc::host_class(emu, "UISheetPresentationControllerDetent")?;
    let d = crate::objc::create_instance(emu, cls, 0)?;
    let s = foundation::new_nsstring(emu, id)?;
    crate::objc::bag_set(emu, d, "identifier", s);
    crate::objc::bag_set(emu, d, "pomme.kind", kind);
    if resolver != 0 {
        let r = crate::libc::copy_block(emu, resolver)?;
        crate::objc::bag_set(emu, d, "pomme.resolver", r);
    }
    Ok(d)
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
    let mut sheet_detents: (Vec<(String, [f64; 4])>, usize) = (Vec::new(), 0);
    let (dim, sheet) = if sheet_style {
        let dim = new_view(emu, [0.0, 0.0, w, h])?;
        let black = crate::uiview::new_color(emu, SHEET_DIMMING)?;
        send(emu, dim, "setBackgroundColor:", &[black])?;
        send(emu, container, "addSubview:", &[dim])?;
        let safe = crate::orientation::safe_area(emu);
        let (top, bottom) = (safe[0], safe[2]);
        let detent = sheet_detent(emu, vc, h - top - bottom)?;
        // The large detent attaches at the safe area's top, full width; smaller
        // ones float inset from the sides and bottom (iOS 26).
        let frame = detent_frame(detent.height, w, h, top);
        sheet_detents = (detent.all.iter().map(|(id, sh)| (id.clone(), detent_frame(*sh, w, h, top))).collect(), detent.shown);
        let sheet = new_view(emu, frame)?;
        view_mut(emu, sheet).clips = true;
        let l = send(emu, sheet, "layer", &[])?;
        {
            let bottom_r = emu.device.corner_radius - FLOAT_INSET;
            let ly = crate::calayer::layer_mut(emu, l);
            ly.corner_radius = SHEET_RADIUS;
            ly.continuous_corners = true;
            ly.masked_corners = 3; // the top two
            if detent.height.is_some() {
                // Floating, its bottom corners follow the display's.
                ly.corner_radii = Some([SHEET_RADIUS, SHEET_RADIUS, bottom_r, bottom_r]);
            }
        }
        if detent.undimmed {
            view_mut(emu, dim).hidden = true;
        }
        send(emu, container, "addSubview:", &[sheet])?;
        set_frame_of(emu, vview, [0.0, 0.0, frame[2], frame[3]]);
        send(emu, sheet, "addSubview:", &[vview])?;
        if detent.grabber {
            let g = new_view(emu, [(frame[2] - GRABBER[0]) / 2.0, GRABBER_TOP, GRABBER[0], GRABBER[1]])?;
            let c = crate::uiview::new_color(emu, GRABBER_COLOR)?;
            send(emu, g, "setBackgroundColor:", &[c])?;
            let gl = send(emu, g, "layer", &[])?;
            crate::calayer::layer_mut(emu, gl).corner_radius = GRABBER[1] / 2.0;
            view_mut(emu, g).user_interaction = false;
            send(emu, sheet, "addSubview:", &[g])?;
        }
        (dim, sheet)
    } else {
        set_frame_of(emu, vview, [0.0, 0.0, w, h]);
        send(emu, container, "addSubview:", &[vview])?;
        (0, 0)
    };
    let completion = if completion != 0 { crate::libc::copy_block(emu, completion)? } else { 0 };
    // UIModalTransitionStyle: 0 cover vertical, 1 flip horizontal, 2 cross dissolve, 3 partial curl.
    let dissolve = emu.nav.props.get(&(vc, "modalTransitionStyle")).copied().unwrap_or(0) == 2;
    let p = Presentation {
        presenter,
        presented: vc,
        container,
        dim,
        sheet,
        covers,
        dissolve,
        moving: Some(true),
        completion,
        detents: sheet_detents.0,
        detent: sheet_detents.1,
        dragged_away: false,
    };
    emu.nav.presentations.push(p.clone());
    let a = animated as u64;
    send(emu, vc, "viewWillAppear:", &[a])?;
    if covers {
        send(emu, presenter, "viewWillDisappear:", &[a])?;
    }
    if animated {
        let duration = slide_modal(emu, &p, true, h)?;
        crate::animation::after_commit(emu, duration, present_done_fire, vc)
    } else {
        present_done(emu, vc)
    }
}

/// Slide a presentation's view (the sheet, or the presented view) up or down
/// the screen of height `h`, and fade its dimming; returns how long it takes.
fn slide_modal(emu: &mut Emu, p: &Presentation, up: bool, h: f64) -> Result<f64> {
    slide_modal_from(emu, p, up, h, None, 0.0)
}

/// As [`slide_modal`], down from where a drag left it (`from`, its presented
/// centre) at `velocity` (pt/s down).
fn slide_modal_from(emu: &mut Emu, p: &Presentation, up: bool, h: f64, from_drag: Option<[f64; 2]>, velocity: f64) -> Result<f64> {
    use crate::animation::{Prop, Val, transition, transition_spring};
    let moving = if p.sheet != 0 { p.sheet } else { send(emu, p.presented, "view", &[])? };
    let c = view(emu, moving).center;
    let top = crate::uiview::frame_of(emu, moving)[1];
    let down = [c[0], c[1] + (h - top)];
    let (o, z, v0) = if up { PRESENT_SPRING } else { DISMISS_SPRING };
    let start = from_drag.unwrap_or(c);
    let left = (down[1] - start[1]).abs().max(1.0);
    let v = if from_drag.is_some() { (velocity / left).max(0.0) } else { v0 };
    let (timing, duration) = transition_spring((o, z, v));
    let c = if up { c } else { start };
    let l = send(emu, moving, "layer", &[])?;
    if p.dissolve {
        // Cross-dissolve: the same spring, on its opacity (measured in the iOS 26 Simulator).
        let (from, to) = if up { (0.0, 1.0) } else { (1.0, 0.0) };
        transition(emu, l, Prop::Opacity, Val::N(from), Val::N(to), timing, duration);
    } else {
        let (from, to) = if up { (down, c) } else { (c, down) };
        transition(emu, l, Prop::Position, Val::P(from), Val::P(to), timing, duration);
    }
    if p.dim != 0 {
        let dl = send(emu, p.dim, "layer", &[])?;
        // From a drag, the dimming fades from where the drag left it.
        let from_dim = match from_drag {
            Some(f) => (1.0 - (f[1] - view(emu, moving).center[1]) / view(emu, moving).bounds[3].max(1.0)).clamp(0.0, 1.0),
            None => 1.0,
        };
        let (a, b) = if up { (0.0, 1.0) } else { (from_dim, 0.0) };
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
        crate::animation::after_commit(emu, duration, dismiss_done_fire, p.presented)
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
    if p.dragged_away {
        sheet_delegate(emu, vc, "presentationControllerDidDismiss:")?;
    }
    send(emu, vc, "release", &[])?;
    Ok(())
}

/// Tell the sheet's delegate (`presentationController.delegate`) `sel`, if it listens.
fn sheet_delegate(emu: &mut Emu, vc: u64, sel: &str) -> Result<u64> {
    let sc = emu.nav.props.get(&(vc, "pomme.sheet")).copied().unwrap_or(0);
    let d = if sc != 0 { objc::bag_get(emu, sc, "delegate").unwrap_or(0) } else { 0 };
    if d == 0 {
        return Ok(1);
    }
    let s = objc::intern(emu, sel)?;
    if !objc::responds(emu, d, s)? {
        return Ok(1);
    }
    send(emu, d, sel, &[sc])
}

// ---- dragging a sheet ----

/// How far the finger moves before it's a drag; how far ahead the release
/// projects; how much a drag past the largest detent gives.
const SHEET_SLOP: f64 = 10.0;
const SHEET_PROJECTION: f64 = 0.2;
const SHEET_RUBBER: f64 = 0.55;

/// A finger went down at `p`: on the top sheet, it may drag it.
pub(crate) fn sheet_began(emu: &mut Emu, hit: u64, p: [f64; 2], now: f64) -> Result<()> {
    emu.nav.sheet_drag = None;
    let Some(top) = emu.nav.presentations.last().cloned() else { return Ok(()) };
    if top.sheet == 0 || top.moving.is_some() {
        return Ok(());
    }
    let f = crate::uiview::frame_of(emu, top.sheet);
    let o = crate::uiview::window_origin(emu, top.sheet);
    if p[0] < o[0] || p[1] < o[1] || p[0] >= o[0] + f[2] || p[1] >= o[1] + f[3] {
        return Ok(());
    }
    emu.nav.sheet_drag = Some(SheetDrag { presented: top.presented, hit, start: p, tracking: false, last: (p[1], now), prev: (p[1], now) });
    Ok(())
}

pub(crate) fn sheet_dragging(emu: &Emu) -> bool {
    emu.nav.sheet_drag.is_some_and(|d| d.tracking)
}

/// The finger moved. Ok(true): it just became the sheet's drag (the view under it loses the touch).
pub(crate) fn sheet_moved(emu: &mut Emu, p: [f64; 2], now: f64) -> Result<bool> {
    let Some(mut d) = emu.nav.sheet_drag else { return Ok(false) };
    d.prev = d.last;
    d.last = (p[1], now);
    let (dx, dy) = (p[0] - d.start[0], p[1] - d.start[1]);
    let mut began = false;
    if !d.tracking {
        if dx.abs() < SHEET_SLOP && dy.abs() < SHEET_SLOP {
            emu.nav.sheet_drag = Some(d);
            return Ok(false);
        }
        let Some(i) = emu.nav.presentations.iter().position(|q| q.presented == d.presented) else { return Ok(false) };
        let sheet = emu.nav.presentations[i].sheet;
        if dx.abs() > dy.abs() || !crate::scrollview::sheet_may_drag(emu, d.hit, sheet, dy > 0.0)? {
            emu.nav.sheet_drag = None;
            return Ok(false);
        }
        crate::scrollview::abandon(emu);
        d.tracking = true;
        began = true;
    }
    emu.nav.sheet_drag = Some(d);
    let Some(q) = emu.nav.presentations.iter().find(|q| q.presented == d.presented).cloned() else { return Ok(began) };
    let rest = view(emu, q.sheet).center;
    let frame = crate::uiview::frame_of(emu, q.sheet);
    // Up to the largest detent's top it follows the finger; past it, it gives a little.
    let highest = q.detents.iter().map(|x| x.1[1]).fold(frame[1], f64::min);
    let room = frame[1] - highest;
    let offset = if dy >= -room { dy } else { -room - rubber(-dy - room, frame[3]) };
    move_sheet(emu, &q, [rest[0], rest[1] + offset]);
    Ok(began)
}

fn rubber(x: f64, size: f64) -> f64 {
    (1.0 - 1.0 / (x * SHEET_RUBBER / size.max(1.0) + 1.0)) * size
}

/// Hold the sheet (and its dimming) where a finger has it.
fn move_sheet(emu: &mut Emu, q: &Presentation, centre: [f64; 2]) {
    use crate::animation::{Prop, Timing, Val, transition};
    let rest = view(emu, q.sheet).center;
    let Ok(l) = send(emu, q.sheet, "layer", &[]) else { return };
    transition(emu, l, Prop::Position, Val::P(centre), Val::P(centre), Timing::Linear, 1e-6);
    if q.dim != 0
        && let Ok(dl) = send(emu, q.dim, "layer", &[])
    {
        let h = view(emu, q.sheet).bounds[3].max(1.0);
        let o = (1.0 - (centre[1] - rest[1]) / h).clamp(0.0, 1.0);
        transition(emu, dl, Prop::Opacity, Val::N(o), Val::N(o), Timing::Linear, 1e-6);
    }
}

/// The finger lifted: dismiss, settle on a detent, or spring back.
pub(crate) fn sheet_ended(emu: &mut Emu, p: [f64; 2], now: f64, cancelled: bool) -> Result<()> {
    let Some(d) = emu.nav.sheet_drag.take() else { return Ok(()) };
    if !d.tracking {
        return Ok(());
    }
    let Some(i) = emu.nav.presentations.iter().position(|q| q.presented == d.presented) else { return Ok(()) };
    let q = emu.nav.presentations[i].clone();
    let shown = crate::animation::presented_center(emu, q.sheet);
    let rest = view(emu, q.sheet).center;
    let frame = crate::uiview::frame_of(emu, q.sheet);
    let (y0, t0) = if (now - d.last.1).abs() > 1e-6 { d.last } else { d.prev };
    let velocity = if now > t0 { (p[1] - y0) / (now - t0) } else { 0.0 };
    let top_now = frame[1] + (shown[1] - rest[1]);
    let projected = top_now + velocity * SHEET_PROJECTION;
    // Dragged down past half of the smallest detent: away it goes.
    let smallest = q.detents.first().map_or(frame, |x| x.1);
    let away = !cancelled && projected > smallest[1] + smallest[3] / 2.0;
    if away {
        let modal = emu.nav.props.get(&(q.presented, "isModalInPresentation")).copied().unwrap_or(0) & 0xff != 0;
        let allowed = !modal && sheet_delegate(emu, q.presented, "presentationControllerShouldDismiss:")? & 0xff != 0;
        if allowed {
            sheet_delegate(emu, q.presented, "presentationControllerWillDismiss:")?;
            emu.nav.presentations[i].moving = Some(false);
            emu.nav.presentations[i].dragged_away = true;
            send(emu, q.presented, "viewWillDisappear:", &[1])?;
            if q.covers {
                let pv = send(emu, q.presenter, "view", &[])?;
                let window = view(emu, q.container).superview;
                send(emu, window, "insertSubview:belowSubview:", &[pv, q.container])?;
                send(emu, q.presenter, "viewWillAppear:", &[1])?;
            }
            let h = view(emu, q.container).bounds[3];
            let duration = slide_modal_from(emu, &q, false, h, Some(shown), velocity.max(0.0))?;
            return crate::animation::after_commit(emu, duration, dismiss_done_fire, q.presented);
        }
        sheet_delegate(emu, q.presented, "presentationControllerDidAttemptToDismiss:")?;
    }
    // Settle on the detent nearest where it was heading.
    let target = if away || q.detents.is_empty() {
        q.detent
    } else {
        (0..q.detents.len()).min_by(|&a, &b| (q.detents[a].1[1] - projected).abs().total_cmp(&(q.detents[b].1[1] - projected).abs())).unwrap_or(q.detent)
    };
    settle(emu, i, target, shown, velocity)
}

/// Put the sheet at detent `target`, springing there from where it's shown.
fn settle(emu: &mut Emu, i: usize, target: usize, shown: [f64; 2], velocity: f64) -> Result<()> {
    use crate::animation::{Prop, Val, transition, transition_spring};
    let q = emu.nav.presentations[i].clone();
    if let Some((id, f)) = q.detents.get(target).cloned()
        && target != q.detent
    {
        // A new detent: its frame and corners (floating or attached).
        set_frame_of(emu, q.sheet, f);
        let vview = send(emu, q.presented, "view", &[])?;
        set_frame_of(emu, vview, [0.0, 0.0, f[2], f[3]]);
        let floating = f[0] > 0.0;
        let l = send(emu, q.sheet, "layer", &[])?;
        let bottom_r = emu.device.corner_radius - FLOAT_INSET;
        crate::calayer::layer_mut(emu, l).corner_radii = floating.then_some([SHEET_RADIUS, SHEET_RADIUS, bottom_r, bottom_r]);
        emu.nav.presentations[i].detent = target;
        let sc = emu.nav.props.get(&(q.presented, "pomme.sheet")).copied().unwrap_or(0);
        if sc != 0 {
            let s = foundation::new_nsstring(emu, &id)?;
            objc::bag_set(emu, sc, "selectedDetentIdentifier", s);
            let d = objc::bag_get(emu, sc, "delegate").unwrap_or(0);
            let sel = objc::intern(emu, "sheetPresentationControllerDidChangeSelectedDetentIdentifier:")?;
            if d != 0 && objc::responds(emu, d, sel)? {
                send(emu, d, "sheetPresentationControllerDidChangeSelectedDetentIdentifier:", &[sc])?;
            }
        }
    }
    let rest = view(emu, q.sheet).center;
    let left = (rest[1] - shown[1]).abs().max(1.0);
    // Speed toward the detent, as a fraction of the distance left per second.
    let toward = if rest[1] > shown[1] { velocity } else { -velocity };
    let (o, z, _) = PRESENT_SPRING;
    let (timing, duration) = transition_spring((o, z, (toward / left).max(0.0)));
    let l = send(emu, q.sheet, "layer", &[])?;
    transition(emu, l, Prop::Position, Val::P(shown), Val::P(rest), timing, duration);
    if q.dim != 0 {
        let dl = send(emu, q.dim, "layer", &[])?;
        let h = view(emu, q.sheet).bounds[3].max(1.0);
        let from = (1.0 - (shown[1] - rest[1]) / h).clamp(0.0, 1.0);
        transition(emu, dl, Prop::Opacity, Val::N(from), Val::N(1.0), timing, duration);
    }
    let info = q.presented;
    crate::animation::after_commit(emu, duration, settle_done_fire, info)
}

fn settle_done_fire(emu: &mut Emu) -> Result<()> {
    let vc = emu.arg(1);
    if let Some(q) = emu.nav.presentations.iter().find(|q| q.presented == vc).cloned() {
        for v in [q.sheet, q.dim] {
            if v != 0 {
                let l = send(emu, v, "layer", &[])?;
                crate::animation::end_transition(emu, l);
            }
        }
    }
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
    (VC, false, "splitViewController", |e| ret(e, 0)),
    (VC, false, "beginAppearanceTransition:animated:", |_| Ok(())),
    (VC, false, "nibName", |e| ret(e, 0)),
    (VC, false, "nibBundle", |e| ret(e, 0)),
    (VC, false, "presentationController", |e| {
        let vc = e.arg(0);
        let sc = send(e, vc, "sheetPresentationController", &[])?;
        ret(e, sc)
    }),
    // A page or form sheet's presentation controller: made on first use, read when presented.
    (VC, false, "sheetPresentationController", |e| {
        let vc = e.arg(0);
        let style = e.nav.props.get(&(vc, "modalPresentationStyle")).copied().unwrap_or(1) as i64;
        if !matches!(style, 1 | 2 | -2) {
            return ret(e, 0);
        }
        if let Some(&sc) = e.nav.props.get(&(vc, "pomme.sheet")) {
            return ret(e, sc);
        }
        let cls = crate::objc::host_class(e, "UISheetPresentationController")?;
        let sc = crate::objc::create_instance(e, cls, 0)?;
        e.nav.props.insert((vc, "pomme.sheet"), sc);
        ret(e, sc)
    }),
    ("UISheetPresentationControllerDetent", true, "mediumDetent", |e| {
        let d = new_detent(e, 1, "com.apple.UIKit.medium", 0)?;
        ret(e, d)
    }),
    ("UISheetPresentationControllerDetent", true, "largeDetent", |e| {
        let d = new_detent(e, 2, "com.apple.UIKit.large", 0)?;
        ret(e, d)
    }),
    ("UISheetPresentationControllerDetent", true, "customDetentWithIdentifier:resolver:", |e| {
        let (id, resolver) = (e.arg(2), e.arg(3));
        let id = if id != 0 { foundation::nsstring(e, id)? } else { String::new() };
        let d = new_detent(e, 3, &id, resolver)?;
        ret(e, d)
    }),
    ("_PommeDetentResolutionContext", false, "maximumDetentValue", |e| {
        let v = e.nav.props.get(&(e.arg(0), "maximumDetentValue")).copied().unwrap_or(0);
        e.cpu.q[0] = v as u128;
        Ok(())
    }),
    ("_PommeDetentResolutionContext", false, "containerTraitCollection", |e| ret(e, 0)),
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
    (NB, false, "prefersLargeTitles", |e| {
        let v = objc::bag_get(e, e.arg(0), "prefersLargeTitles").unwrap_or(0) & 0xff;
        ret(e, v)
    }),
    (NB, false, "setPrefersLargeTitles:", |e| {
        let (bar, v) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, bar, "prefersLargeTitles", v);
        relayout_bar(e, bar)
    }),
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
    (NI, false, "largeTitleDisplayMode", |e| {
        let m = item_state(e, e.arg(0)).large_title_mode;
        ret(e, m)
    }),
    (NI, false, "setLargeTitleDisplayMode:", |e| {
        let (i, m) = (e.arg(0), e.arg(2));
        e.nav.item_state.entry(i).or_default().large_title_mode = m;
        // The bar of a controller showing this item changes height.
        let bars: Vec<u64> = e.nav.bars.keys().copied().collect();
        for bar in bars {
            relayout_bar(e, bar)?;
        }
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
