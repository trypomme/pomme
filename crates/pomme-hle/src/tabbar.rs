//! Tab bars: UITabBarController, UITabBar and UITabBarItem.
//!
//! A tab bar controller is a container: its view holds the selected tab's
//! view, full size (content runs under the bar; the bar adds to the
//! children's bottom safe area), and the tab bar along the bottom, 49 pt
//! above the home indicator's inset. The bar shows one button per item, and
//! a tap on one selects its tab, asking the controller's delegate first, as
//! UIKit does.
//!
//! pomme draws the bar itself (render.rs) from one description of it,
//! [`look`]: where each button, image, title and badge goes, in both designs.
//! Under Liquid Glass (iOS 26) the buttons sit on a floating capsule (the
//! platter) with a lens under the selected one; before, the bar spans the
//! screen's width and the selected tab is tinted. The numbers are measured on
//! the iOS 26 Simulator, in both designs, with 2 to 5 tabs
//! (tests/fixtures/tabbar). Accessibility sees each button as an element
//! (accessibility.rs), as VoiceOver does UIKit's.

use crate::objc::{self, bag_get, bag_set, host_class, send};
use crate::uiview::{set_frame_of, view};
use crate::{Emu, HostFn, Result, foundation};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct State {
    /// Tab bar controller → its tabs.
    tabs: HashMap<u64, Tabs>,
    /// Container view → its tab bar controller.
    containers: HashMap<u64, u64>,
    /// Tab bar → what's on it.
    bars: HashMap<u64, Bar>,
    /// A More list's view → its tab bar controller.
    more_lists: HashMap<u64, u64>,
}

#[derive(Default, Clone)]
struct Tabs {
    controllers: Vec<u64>,
    selected: Option<usize>,
    container: u64,
    bar: u64,
    delegate: u64,
    /// The controller whose view is in the container now.
    shown: u64,
    /// The More tab's navigation controller (`moreNavigationController`), its
    /// list, and the More item, made on first use.
    more: u64,
    more_list: u64,
    more_item: u64,
    /// The More tab is selected, showing its list (no tab of its own selected).
    more_selected: bool,
    /// The tabs as iOS 18's `UITab`s, when they were given that way (`tabs`),
    /// each standing for the controller at its index.
    uitabs: Vec<u64>,
    /// iOS 26's bottom accessory (a UITabAccessory), and the glass it's shown on.
    accessory: u64,
    accessory_view: u64,
}

/// A tab bar shows at most five buttons; with more tabs, four and More.
const MAX_BUTTONS: usize = 5;

impl Tabs {
    /// Whether some tabs are in the More list.
    fn overflows(&self) -> bool {
        self.controllers.len() > MAX_BUTTONS
    }

    /// The tabs with their own buttons.
    fn on_bar(&self) -> &[u64] {
        if self.overflows() { &self.controllers[..MAX_BUTTONS - 1] } else { &self.controllers }
    }

    /// Whether the More tab is what's selected (its list, or a tab in it).
    fn in_more(&self) -> bool {
        self.more_selected || (self.overflows() && self.selected.is_some_and(|i| i >= MAX_BUTTONS - 1))
    }
}

#[derive(Default, Clone)]
struct Bar {
    items: Vec<u64>,
    selected: u64,
    delegate: u64,
    /// The tab bar controller it belongs to, if any.
    controller: u64,
}

/// The bar's height above the bottom safe-area inset: 49 pt (both designs);
/// with the phone on its side (a compact height), 44 under Liquid Glass, 32
/// before (over the 20-pt inset there: 64 and 52 in all, as measured).
pub fn bar_height(emu: &Emu) -> f64 {
    if !compact(emu) {
        49.0
    } else if emu.system.liquid_glass() {
        44.0
    } else {
        32.0
    }
}

/// The phone is on its side: the bar's compact layout (image beside title).
fn compact(emu: &Emu) -> bool {
    crate::orientation::size_classes(emu).1 == crate::device::COMPACT
}
/// Titles: 10 pt, medium; selected under Liquid Glass, semibold.
const TITLE_SIZE: f64 = 10.0;
const TITLE_WEIGHT: f64 = 0.23;
const GLASS_SELECTED_WEIGHT: f64 = 0.3;
/// Tab images: SF Symbols at the large scale, 18.2 pt (fitted to the sizes
/// iOS gives house.fill, magnifyingglass and gear in its tab bars).
const SYMBOL_POINTS: f64 = 18.2;

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret_size(emu: &mut Emu, w: f64, h: f64) {
    emu.cpu.q[0] = w.to_bits() as u128;
    emu.cpu.q[1] = h.to_bits() as u128;
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn tabs(emu: &mut Emu, tbc: u64) -> Tabs {
    emu.tabs.tabs.entry(tbc).or_default().clone()
}

fn tabs_mut(emu: &mut Emu, tbc: u64) -> &mut Tabs {
    emu.tabs.tabs.entry(tbc).or_default()
}

fn bar(emu: &mut Emu, b: u64) -> Bar {
    emu.tabs.bars.entry(b).or_default().clone()
}

fn bar_mut(emu: &mut Emu, b: u64) -> &mut Bar {
    emu.tabs.bars.entry(b).or_default()
}

/// A tab bar controller's container view's bar.
pub fn container_bar(emu: &Emu, v: u64) -> Option<u64> {
    let tbc = emu.tabs.containers.get(&v)?;
    emu.tabs.tabs.get(tbc).map(|t| t.bar)
}

/// How much of a tab bar controller's container (`v`) the bar covers at the
/// bottom for what's in its subview `below`, with a bottom accessory too
/// (its 48 pt and 8 above the bar, measured); nothing for the bar and the
/// accessory themselves, or while the bar's hidden. None: not a container.
pub fn bottom_cover(emu: &Emu, v: u64, below: u64) -> Option<f64> {
    let t = emu.tabs.tabs.get(emu.tabs.containers.get(&v)?)?;
    let hidden = emu.ui.views.views.get(&t.bar).is_none_or(|b| b.hidden);
    if below == t.bar || (below != 0 && below == t.accessory_view) || hidden {
        return Some(0.0);
    }
    let accessory = if t.accessory_view != 0 && accessory_shown(emu) { ACCESSORY_HEIGHT + ACCESSORY_GAP } else { 0.0 };
    Some(bar_height(emu) + accessory)
}

/// The bottom accessory: a capsule of glass 48 pt tall, 8 pt above the bar
/// (both measured), as wide as the bar's capsule; only under Liquid Glass,
/// and upright (on its side it's inline in the bar, which pomme doesn't draw).
const ACCESSORY_HEIGHT: f64 = 48.0;
const ACCESSORY_GAP: f64 = 8.0;

fn accessory_shown(emu: &Emu) -> bool {
    emu.system.liquid_glass() && !compact(emu)
}

/// Set (or clear) the bottom accessory: its content view on a capsule of glass above the bar.
fn set_accessory(emu: &mut Emu, tbc: u64, accessory: u64) -> Result<()> {
    let old = tabs(emu, tbc);
    if old.accessory_view != 0 {
        send(emu, old.accessory_view, "removeFromSuperview", &[])?;
    }
    if accessory != 0 {
        send(emu, accessory, "retain", &[])?;
    }
    if old.accessory != 0 {
        objc::objc_release_obj(emu, old.accessory)?;
    }
    let mut glass = 0;
    let content = if accessory == 0 { 0 } else { bag_get(emu, accessory, "contentView").unwrap_or(0) };
    // (Before iOS 26's design there's none to show: the content isn't put anywhere, as measured.)
    if content != 0 && emu.system.liquid_glass() {
        let glass_class = objc::host_class(emu, "UIGlassEffect")?;
        let effect = send(emu, glass_class, "effectWithStyle:", &[crate::glass::REGULAR])?;
        glass = new_object(emu, "_UITabAccessoryContainer")?;
        glass = send(emu, glass, "initWithEffect:", &[effect])?;
        let corners = objc::host_class(emu, "UICornerConfiguration")?;
        let capsule = send(emu, corners, "capsuleConfiguration", &[])?;
        send(emu, glass, "setCornerConfiguration:", &[capsule])?;
        send(emu, content, "removeFromSuperview", &[])?;
        send(emu, glass, "addSubview:", &[content])?;
        send(emu, content, "setAutoresizingMask:", &[2 | 16])?;
        bag_set(emu, glass, "pommeAccessoryContent", content);
    }
    let tm = tabs_mut(emu, tbc);
    tm.accessory = accessory;
    tm.accessory_view = glass;
    let container = tm.container;
    if container != 0 {
        if glass != 0 {
            let bar = tabs(emu, tbc).bar;
            send(emu, container, "insertSubview:belowSubview:", &[glass, bar])?;
        }
        layout_container(emu, container)?;
        // (What's in the tabs is inset again.)
        let shown = tabs(emu, tbc).shown;
        if shown != 0 {
            let v = send(emu, shown, "view", &[])?;
            send(emu, v, "setNeedsLayout", &[])?;
        }
    }
    crate::host::changed(emu);
    Ok(())
}

/// The accessory environment of a view in a bottom accessory (UITabAccessoryEnvironment:
/// regular, 2, as measured), if it's in one.
pub fn accessory_environment(emu: &Emu, mut v: u64) -> Option<u64> {
    for _ in 0..64 {
        if v == 0 {
            return None;
        }
        if bag_get(emu, v, "pommeAccessoryContent").is_some() {
            return Some(2);
        }
        v = emu.ui.views.views.get(&v).map_or(0, |vw| vw.superview);
    }
    None
}

/// Every tab bar there is (for tests and tools).
pub fn bars(emu: &Emu) -> Vec<u64> {
    let mut v: Vec<u64> = emu.tabs.bars.keys().copied().collect();
    v.sort();
    v
}

/// Every More list there is (for tests and tools).
pub fn more_lists(emu: &Emu) -> Vec<u64> {
    let mut v: Vec<u64> = emu.tabs.more_lists.keys().copied().collect();
    v.sort();
    v
}

/// Whether a view is a tab bar (pomme draws it).
pub fn is_bar(emu: &Emu, v: u64) -> bool {
    emu.tabs.bars.contains_key(&v)
}

/// Store an object property (a retained copy for strings, retained otherwise), releasing the old.
fn set_object(emu: &mut Emu, o: u64, key: &'static str, v: u64, copy: bool) -> Result<()> {
    let v = if v == 0 {
        0
    } else if copy {
        send(emu, v, "copy", &[])?
    } else {
        send(emu, v, "retain", &[])?
    };
    if let Some(old) = bag_get(emu, o, key).filter(|&old| old != 0) {
        objc::objc_release_obj(emu, old)?;
    }
    bag_set(emu, o, key, v);
    crate::host::changed(emu);
    Ok(())
}

// ---- UITabBarItem ----

/// UITabBarSystemItem: its title and symbol.
fn system_item(kind: u64) -> (&'static str, &'static str) {
    match kind {
        0 => ("More", "ellipsis"),
        1 => ("Favorites", "star.fill"),
        2 => ("Featured", "star"),
        3 => ("Top Rated", "star"),
        4 => ("Recents", "clock.fill"),
        5 => ("Contacts", "person.crop.circle.fill"),
        6 => ("History", "clock.fill"),
        7 => ("Bookmarks", "book.fill"),
        8 => ("Search", "magnifyingglass"),
        9 => ("Downloads", "arrow.down.circle.fill"),
        10 => ("Most Recent", "clock.fill"),
        _ => ("Most Viewed", "list.number"),
    }
}

fn item_init(emu: &mut Emu, item: u64, title: u64, image: u64, selected: u64, tag: u64) -> Result<()> {
    set_object(emu, item, "title", title, true)?;
    set_object(emu, item, "image", image, false)?;
    set_object(emu, item, "selectedImage", selected, false)?;
    bag_set(emu, item, "tag", tag);
    ret(emu, item)
}

fn item_string(emu: &mut Emu, item: u64, key: &str) -> Option<String> {
    let s = bag_get(emu, item, key).filter(|&s| s != 0)?;
    foundation::nsstring(emu, s).ok()
}

/// The controller an item stands for (its title falls back to the controller's).
fn item_title(emu: &mut Emu, item: u64) -> Result<String> {
    if let Some(t) = item_string(emu, item, "title") {
        return Ok(t);
    }
    let vc = bag_get(emu, item, "pommeController").unwrap_or(0);
    if vc != 0 {
        let t = send(emu, vc, "title", &[])?;
        if t != 0 {
            return Ok(foundation::nsstring(emu, t).unwrap_or_default());
        }
    }
    Ok(String::new())
}

const ITEM: &str = "UITabBarItem";

macro_rules! object_property {
    ($get:literal, $set:literal, $copy:expr) => {
        [
            (ITEM, false, $get, (|e: &mut Emu| {
                let v = bag_get(e, e.arg(0), $get).unwrap_or(0);
                ret(e, v)
            }) as HostFn),
            (ITEM, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                set_object(e, o, $get, v, $copy)
            }) as HostFn),
        ]
    };
}

const ITEM_PROPS: &[[(&str, bool, &str, HostFn); 2]] = &[
    object_property!("title", "setTitle:", true),
    object_property!("image", "setImage:", false),
    object_property!("selectedImage", "setSelectedImage:", false),
    object_property!("badgeValue", "setBadgeValue:", true),
    object_property!("badgeColor", "setBadgeColor:", false),
    object_property!("standardAppearance", "setStandardAppearance:", false),
    object_property!("scrollEdgeAppearance", "setScrollEdgeAppearance:", false),
];

// ---- UIViewController's tab bar item ----

/// A controller's tab bar item: made on first use, with its title.
fn vc_item(emu: &mut Emu, vc: u64) -> Result<u64> {
    if let Some(i) = bag_get(emu, vc, "tabBarItem").filter(|&i| i != 0) {
        return Ok(i);
    }
    let item = new_object(emu, ITEM)?;
    send(emu, item, "init", &[])?;
    bag_set(emu, item, "pommeController", vc);
    bag_set(emu, vc, "tabBarItem", item);
    Ok(item)
}

/// The tab bar controller a controller is in, if any.
fn tab_bar_controller_of(emu: &Emu, mut vc: u64) -> u64 {
    for _ in 0..64 {
        vc = emu.nav.parent.get(&vc).copied().unwrap_or(0);
        if vc == 0 || emu.tabs.tabs.contains_key(&vc) {
            return vc;
        }
    }
    0
}

// ---- UITabBarController ----

/// The container view and the bar, made on first use.
fn ensure_view(emu: &mut Emu, tbc: u64) -> Result<u64> {
    let t = tabs(emu, tbc);
    if t.container != 0 {
        return Ok(t.container);
    }
    let container = new_object(emu, "UIView")?;
    send(emu, container, "init", &[])?;
    let (w, h) = crate::orientation::screen(emu);
    set_frame_of(emu, container, [0.0, 0.0, w, h]);
    let b = ensure_bar(emu, tbc)?;
    tabs_mut(emu, tbc).container = container;
    emu.tabs.containers.insert(container, tbc);
    emu.ui.views.vc_view.insert(tbc, container);
    send(emu, container, "addSubview:", &[b])?;
    show_selected(emu, tbc)?;
    Ok(container)
}

fn ensure_bar(emu: &mut Emu, tbc: u64) -> Result<u64> {
    let t = tabs(emu, tbc);
    if t.bar != 0 {
        return Ok(t.bar);
    }
    let b = new_object(emu, "UITabBar")?;
    send(emu, b, "init", &[])?;
    tabs_mut(emu, tbc).bar = b;
    bar_mut(emu, b).controller = tbc;
    Ok(b)
}

/// Place the bar along the bottom and the selected tab's view under it, full size.
pub fn layout_container(emu: &mut Emu, container: u64) -> Result<()> {
    let Some(&tbc) = emu.tabs.containers.get(&container) else { return Ok(()) };
    let t = tabs(emu, tbc);
    let b = view(emu, container).bounds;
    let (w, h) = (b[2], b[3]);
    let bar_h = bar_height(emu) + crate::orientation::safe_area(emu)[2];
    set_frame_of(emu, t.bar, [0.0, h - bar_h, w, bar_h]);
    // Hidden while a pushed screen asks (hidesBottomBarWhenPushed).
    let hidden = hidden_by_push(emu, t.shown);
    if view(emu, t.bar).hidden != hidden {
        crate::uiview::view_mut(emu, t.bar).hidden = hidden;
        crate::host::changed(emu);
    }
    if t.accessory_view != 0 {
        let shown = accessory_shown(emu) && !hidden;
        let frame = [21.0, h - bar_h - ACCESSORY_GAP - ACCESSORY_HEIGHT, (w - 42.0).max(0.0), ACCESSORY_HEIGHT];
        set_frame_of(emu, t.accessory_view, frame);
        let content = bag_get(emu, t.accessory_view, "pommeAccessoryContent").unwrap_or(0);
        if content != 0 {
            set_frame_of(emu, content, [0.0, 0.0, frame[2], frame[3]]);
        }
        if view(emu, t.accessory_view).hidden == shown {
            crate::uiview::view_mut(emu, t.accessory_view).hidden = !shown;
            crate::host::changed(emu);
        }
    }
    if t.shown != 0 {
        let v = send(emu, t.shown, "view", &[])?;
        set_frame_of(emu, v, [0.0, 0.0, w, h]);
    }
    Ok(())
}

/// Whether a tab's navigation controller shows a screen that hides the bar:
/// one pushed (not its root) with `hidesBottomBarWhenPushed`, or one above it.
fn hidden_by_push(emu: &Emu, shown: u64) -> bool {
    let Some(stack) = crate::navigation::stack_of(emu, shown) else { return false };
    stack.iter().skip(1).any(|&vc| bag_get(emu, vc, "hidesBottomBarWhenPushed").unwrap_or(0) != 0)
}

/// A navigation controller's stack changed: if it's a tab's, the bar may hide or come back.
pub(crate) fn navigation_changed(emu: &mut Emu, nc: u64) -> Result<()> {
    let tbc = tab_bar_controller_of(emu, nc);
    if tbc != 0 {
        let c = tabs(emu, tbc).container;
        if c != 0 {
            layout_container(emu, c)?;
        }
    }
    Ok(())
}

/// The More tab's item: UIKit's system More item.
fn more_item(emu: &mut Emu, tbc: u64) -> Result<u64> {
    let t = tabs(emu, tbc);
    if t.more_item != 0 {
        return Ok(t.more_item);
    }
    let item = new_object(emu, ITEM)?;
    send(emu, item, "initWithTabBarSystemItem:tag:", &[0, 0])?;
    tabs_mut(emu, tbc).more_item = item;
    Ok(item)
}

/// The More tab's navigation controller, its root the More list, made on first use.
fn more_navigation(emu: &mut Emu, tbc: u64) -> Result<u64> {
    let t = tabs(emu, tbc);
    if t.more != 0 {
        return Ok(t.more);
    }
    let list = new_object(emu, "UIMoreListController")?;
    send(emu, list, "init", &[])?;
    let title = foundation::new_nsstring(emu, "More")?;
    send(emu, list, "setTitle:", &[title])?;
    bag_set(emu, list, "pommeTabBarController", tbc);
    let nav = new_object(emu, "UIMoreNavigationController")?;
    send(emu, nav, "initWithRootViewController:", &[list])?;
    crate::navigation::add_child(emu, tbc, nav)?;
    let tm = tabs_mut(emu, tbc);
    tm.more = nav;
    tm.more_list = list;
    Ok(nav)
}

/// The bar's items: each tab's controller's item (with more than five tabs,
/// the first four's and More's).
fn sync_items(emu: &mut Emu, tbc: u64) -> Result<()> {
    let t = tabs(emu, tbc);
    let b = ensure_bar(emu, tbc)?;
    let mut items = Vec::new();
    for &vc in t.on_bar() {
        items.push(vc_item(emu, vc)?);
    }
    if t.overflows() {
        items.push(more_item(emu, tbc)?);
    }
    let selected = if t.in_more() { items.last().copied().unwrap_or(0) } else { t.selected.and_then(|i| items.get(i).copied()).unwrap_or(0) };
    let bs = bar_mut(emu, b);
    bs.items = items;
    bs.selected = selected;
    crate::host::changed(emu);
    Ok(())
}

/// Put the selected tab's view in the container (the one before goes),
/// with UIKit's appearance callbacks while it's on screen.
fn show_selected(emu: &mut Emu, tbc: u64) -> Result<()> {
    let t = tabs(emu, tbc);
    if t.container == 0 {
        return Ok(());
    }
    let new = if t.in_more() {
        // The More tab: its navigation controller, with the selected tab pushed on its list.
        let nav = more_navigation(emu, tbc)?;
        let list = tabs(emu, tbc).more_list;
        let mut stack = vec![list];
        if !t.more_selected
            && let Some(vc) = t.selected.and_then(|i| t.controllers.get(i).copied())
        {
            stack.push(vc);
        }
        let now = crate::navigation::stack_of(emu, nav);
        if now.as_deref() != Some(&stack[..]) {
            if t.shown == nav && stack.len() == 2 && now.as_deref() == Some(&stack[..1]) {
                // Chosen from the list: pushed, as UIKit pushes a row's screen.
                send(emu, nav, "pushViewController:animated:", &[stack[1], 1])?;
            } else {
                let a = foundation::new_array_of(emu, &stack)?;
                send(emu, nav, "setViewControllers:animated:", &[a, 0])?;
            }
        }
        nav
    } else {
        t.selected.and_then(|i| t.controllers.get(i).copied()).unwrap_or(0)
    };
    if new == t.shown {
        crate::host::changed(emu);
        return Ok(());
    }
    let on_screen = view(emu, t.container).window != 0;
    if on_screen && t.shown != 0 {
        crate::navigation::appearance(emu, t.shown, "viewWillDisappear:", false)?;
    }
    if on_screen && new != 0 {
        crate::navigation::appearance(emu, new, "viewWillAppear:", false)?;
    }
    if t.shown != 0 {
        let old = send(emu, t.shown, "view", &[])?;
        send(emu, old, "removeFromSuperview", &[])?;
    }
    tabs_mut(emu, tbc).shown = new;
    if new != 0 {
        // (Under the bar, and under its accessory if there's one.)
        let v = send(emu, new, "view", &[])?;
        let above = if t.accessory_view != 0 { t.accessory_view } else { t.bar };
        send(emu, t.container, "insertSubview:belowSubview:", &[v, above])?;
    }
    layout_container(emu, t.container)?;
    if on_screen && t.shown != 0 {
        crate::navigation::appearance(emu, t.shown, "viewDidDisappear:", false)?;
    }
    if on_screen && new != 0 {
        crate::navigation::appearance(emu, new, "viewDidAppear:", false)?;
    }
    crate::host::changed(emu);
    Ok(())
}

/// The tab bar controller's selected child: the one appearance callbacks reach.
pub fn selected_child(emu: &Emu, vc: u64) -> Option<Option<u64>> {
    let t = emu.tabs.tabs.get(&vc)?;
    Some(if t.in_more() { Some(t.more).filter(|&m| m != 0) } else { t.selected.and_then(|i| t.controllers.get(i).copied()) })
}

fn set_controllers(emu: &mut Emu, tbc: u64, list: Vec<u64>) -> Result<()> {
    let old = tabs(emu, tbc);
    let previous = old.selected.and_then(|i| old.controllers.get(i).copied());
    for &vc in &list {
        if !old.controllers.contains(&vc) {
            send(emu, vc, "retain", &[])?;
            crate::navigation::add_child(emu, tbc, vc)?;
            send(emu, vc, "didMoveToParentViewController:", &[tbc])?;
        }
    }
    for &vc in &old.controllers {
        if !list.contains(&vc) {
            send(emu, vc, "willMoveToParentViewController:", &[0])?;
            if old.shown == vc {
                let v = send(emu, vc, "view", &[])?;
                send(emu, v, "removeFromSuperview", &[])?;
                tabs_mut(emu, tbc).shown = 0;
            }
            crate::navigation::remove_child(emu, vc)?;
            send(emu, vc, "didMoveToParentViewController:", &[0])?;
            send(emu, vc, "release", &[])?;
        }
    }
    // The selection stays on the same controller if it's still there, else the first tab.
    let selected = previous.and_then(|p| list.iter().position(|&v| v == p)).or(if list.is_empty() { None } else { Some(0) });
    let t = tabs_mut(emu, tbc);
    t.controllers = list;
    t.selected = selected;
    sync_items(emu, tbc)?;
    show_selected(emu, tbc)
}

/// The More tab tapped: its list, the delegate asked first and told after (about the More navigation controller).
fn select_more(emu: &mut Emu, tbc: u64) -> Result<()> {
    let nav = more_navigation(emu, tbc)?;
    let delegate = tabs(emu, tbc).delegate;
    if delegate != 0 {
        let ask = objc::intern(emu, "tabBarController:shouldSelectViewController:")?;
        if objc::responds(emu, delegate, ask)? && send(emu, delegate, "tabBarController:shouldSelectViewController:", &[tbc, nav])? & 0xff == 0 {
            return Ok(());
        }
    }
    let tm = tabs_mut(emu, tbc);
    tm.more_selected = true;
    tm.selected = None;
    sync_items(emu, tbc)?;
    show_selected(emu, tbc)?;
    if delegate != 0 {
        let did = objc::intern(emu, "tabBarController:didSelectViewController:")?;
        if objc::responds(emu, delegate, did)? {
            send(emu, delegate, "tabBarController:didSelectViewController:", &[tbc, nav])?;
        }
    }
    Ok(())
}

/// Select a tab. From a tap (`by_user`), the delegate is asked first and told after.
/// With `UITab`s it's asked about and told of tabs instead, and told of any
/// change of tab, a program's too (as measured).
fn select(emu: &mut Emu, tbc: u64, index: usize, by_user: bool) -> Result<()> {
    let t = tabs(emu, tbc);
    let Some(&vc) = t.controllers.get(index) else { return Ok(()) };
    if let Some(&tab) = t.uitabs.get(index) {
        if by_user && t.delegate != 0 {
            let ask = objc::intern(emu, "tabBarController:shouldSelectTab:")?;
            if objc::responds(emu, t.delegate, ask)? && send(emu, t.delegate, "tabBarController:shouldSelectTab:", &[tbc, tab])? & 0xff == 0 {
                return Ok(());
            }
        }
        let previous = selected_tab(&t);
        let tm = tabs_mut(emu, tbc);
        tm.selected = Some(index);
        tm.more_selected = false;
        sync_items(emu, tbc)?;
        show_selected(emu, tbc)?;
        if previous != tab {
            did_select_tab(emu, tbc, tab, previous)?;
        }
        return Ok(());
    }
    if by_user && t.delegate != 0 {
        let ask = objc::intern(emu, "tabBarController:shouldSelectViewController:")?;
        if objc::responds(emu, t.delegate, ask)? && send(emu, t.delegate, "tabBarController:shouldSelectViewController:", &[tbc, vc])? & 0xff == 0 {
            return Ok(());
        }
    }
    let tm = tabs_mut(emu, tbc);
    tm.selected = Some(index);
    tm.more_selected = false;
    sync_items(emu, tbc)?;
    show_selected(emu, tbc)?;
    if by_user && t.delegate != 0 {
        let did = objc::intern(emu, "tabBarController:didSelectViewController:")?;
        if objc::responds(emu, t.delegate, did)? {
            send(emu, t.delegate, "tabBarController:didSelectViewController:", &[tbc, vc])?;
        }
    }
    Ok(())
}

// ---- iOS 18's UITab ----

const TAB: &str = "UITab";
const SEARCH_TAB: &str = "UISearchTab";

fn selected_tab(t: &Tabs) -> u64 {
    if t.more_selected { 0 } else { t.selected.and_then(|i| t.uitabs.get(i).copied()).unwrap_or(0) }
}

fn did_select_tab(emu: &mut Emu, tbc: u64, tab: u64, previous: u64) -> Result<()> {
    let delegate = tabs(emu, tbc).delegate;
    if delegate != 0 {
        let did = objc::intern(emu, "tabBarController:didSelectTab:previousTab:")?;
        if objc::responds(emu, delegate, did)? {
            send(emu, delegate, "tabBarController:didSelectTab:previousTab:", &[tbc, tab, previous])?;
        }
    }
    Ok(())
}

fn tab_init(emu: &mut Emu, tab: u64, title: u64, image: u64, identifier: u64, provider: u64) -> Result<()> {
    set_object(emu, tab, "title", title, true)?;
    set_object(emu, tab, "image", image, false)?;
    set_object(emu, tab, "identifier", identifier, true)?;
    // (The provider, a block, kept as a heap copy.)
    set_object(emu, tab, "pommeProvider", provider, true)?;
    ret(emu, tab)
}

/// A tab's view controller: its provider's, on first use.
fn tab_controller(emu: &mut Emu, tab: u64) -> Result<u64> {
    if let Some(vc) = bag_get(emu, tab, "viewController").filter(|&v| v != 0) {
        return Ok(vc);
    }
    let provider = bag_get(emu, tab, "pommeProvider").unwrap_or(0);
    if provider == 0 {
        return Ok(0);
    }
    let vc = crate::libsystem::call_block(emu, provider, &[tab])?;
    if vc == 0 {
        return Ok(0);
    }
    send(emu, vc, "retain", &[])?;
    bag_set(emu, tab, "viewController", vc);
    bag_set(emu, vc, "pommeTab", tab);
    let item = vc_item(emu, vc)?;
    let search = host_class(emu, SEARCH_TAB)?;
    if objc::is_kind_of(emu, tab, search)? {
        // (Liquid Glass sets the search tab apart.)
        bag_set(emu, item, "pommeSearchTab", 1);
    }
    for key in ["title", "image", "badgeValue"] {
        tab_to_item(emu, tab, key)?;
    }
    Ok(vc)
}

/// The bar shows a tab through its controller's item (as measured, the
/// tab's title, image and badge): what the tab has, the item gets.
fn tab_to_item(emu: &mut Emu, tab: u64, key: &'static str) -> Result<()> {
    let Some(vc) = bag_get(emu, tab, "viewController").filter(|&v| v != 0) else { return Ok(()) };
    let item = vc_item(emu, vc)?;
    let v = bag_get(emu, tab, key).unwrap_or(0);
    // (A tab without a title or image leaves the item's: a system item's, say.)
    if v != 0 || key == "badgeValue" {
        set_object(emu, item, key, v, key != "image")?;
    }
    let tbc = bag_get(emu, tab, "pommeTabBarController").unwrap_or(0);
    if tbc != 0 && emu.tabs.tabs.contains_key(&tbc) {
        sync_items(emu, tbc)?;
    }
    Ok(())
}

/// `tabs`: each tab's controller becomes a child, in order; the delegate
/// hears of a new selected tab (as measured).
fn set_tabs(emu: &mut Emu) -> Result<()> {
    let (tbc, a) = (emu.arg(0), emu.arg(2));
    let list = if a == 0 { Vec::new() } else { foundation::array_values(emu, a)? };
    let old = tabs(emu, tbc);
    let previous = selected_tab(&old);
    let mut controllers = Vec::with_capacity(list.len());
    for &tab in &list {
        if !old.uitabs.contains(&tab) {
            send(emu, tab, "retain", &[])?;
        }
        bag_set(emu, tab, "pommeTabBarController", tbc);
        let vc = tab_controller(emu, tab)?;
        if vc != 0 {
            controllers.push(vc);
        }
    }
    for &tab in &old.uitabs {
        if !list.contains(&tab) {
            bag_set(emu, tab, "pommeTabBarController", 0);
            objc::objc_release_obj(emu, tab)?;
        }
    }
    tabs_mut(emu, tbc).uitabs = list;
    set_controllers(emu, tbc, controllers)?;
    let now = selected_tab(&tabs(emu, tbc));
    if now != previous && now != 0 {
        did_select_tab(emu, tbc, now, previous)?;
    }
    Ok(())
}

fn set_view_controllers(emu: &mut Emu) -> Result<()> {
    let (tbc, a) = (emu.arg(0), emu.arg(2));
    let list = if a == 0 { Vec::new() } else { foundation::array_values(emu, a)? };
    for tab in std::mem::take(&mut tabs_mut(emu, tbc).uitabs) {
        bag_set(emu, tab, "pommeTabBarController", 0);
        objc::objc_release_obj(emu, tab)?;
    }
    set_controllers(emu, tbc, list)
}

macro_rules! tab_property {
    ($get:literal, $set:literal) => {
        [
            (TAB, false, $get, (|e: &mut Emu| {
                let v = bag_get(e, e.arg(0), $get).unwrap_or(0);
                ret(e, v)
            }) as HostFn),
            (TAB, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                set_object(e, o, $get, v, $get != "image")?;
                tab_to_item(e, o, $get)
            }) as HostFn),
        ]
    };
}

const TAB_PROPS: &[[(&str, bool, &str, HostFn); 2]] = &[
    tab_property!("title", "setTitle:"),
    tab_property!("image", "setImage:"),
    tab_property!("badgeValue", "setBadgeValue:"),
];

const TAB_METHODS: &[(&str, bool, &str, HostFn)] = &[
    (TAB, false, "initWithTitle:image:identifier:viewControllerProvider:", |e| {
        let (t, title, image, id, provider) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
        tab_init(e, t, title, image, id, provider)
    }),
    // The search tab: "Search", a magnifying glass, UIKit's own identifier (as measured).
    (SEARCH_TAB, false, "initWithViewControllerProvider:", |e| {
        let (t, provider) = (e.arg(0), e.arg(2));
        let title = foundation::new_nsstring(e, "Search")?;
        let image = crate::symbols::image(e, "magnifyingglass", Default::default())?;
        let id = foundation::new_nsstring(e, "com.apple.UIKit.Search")?;
        tab_init(e, t, title, image, id, provider)
    }),
    (TAB, false, "identifier", |e| {
        let v = bag_get(e, e.arg(0), "identifier").unwrap_or(0);
        ret(e, v)
    }),
    (TAB, false, "viewController", |e| {
        let vc = tab_controller(e, e.arg(0))?;
        ret(e, vc)
    }),
    (TAB, false, "tabBarController", |e| {
        let v = bag_get(e, e.arg(0), "pommeTabBarController").unwrap_or(0);
        ret(e, v)
    }),
    ("UIViewController", false, "tab", |e| {
        let v = bag_get(e, e.arg(0), "pommeTab").unwrap_or(0);
        ret(e, v)
    }),
    ("UITabAccessory", false, "initWithContentView:", |e| {
        let (a, c) = (e.arg(0), e.arg(2));
        if c != 0 {
            send(e, c, "retain", &[])?;
        }
        bag_set(e, a, "contentView", c);
        ret(e, a)
    }),
    ("UITabAccessory", false, "contentView", |e| {
        let c = bag_get(e, e.arg(0), "contentView").unwrap_or(0);
        ret(e, c)
    }),
    (TBC, false, "bottomAccessory", |e| {
        let a = tabs(e, e.arg(0)).accessory;
        ret(e, a)
    }),
    (TBC, false, "setBottomAccessory:", |e| {
        let (t, a) = (e.arg(0), e.arg(2));
        set_accessory(e, t, a)
    }),
    (TBC, false, "setBottomAccessory:animated:", |e| {
        let (t, a) = (e.arg(0), e.arg(2));
        set_accessory(e, t, a)
    }),
    (TBC, false, "tabs", |e| {
        let list = tabs(e, e.arg(0)).uitabs;
        let a = foundation::new_array_of(e, &list)?;
        ret(e, a)
    }),
    (TBC, false, "setTabs:", set_tabs),
    (TBC, false, "setTabs:animated:", set_tabs),
    (TBC, false, "selectedTab", |e| {
        let t = tabs(e, e.arg(0));
        ret(e, selected_tab(&t))
    }),
    (TBC, false, "setSelectedTab:", |e| {
        let (tbc, tab) = (e.arg(0), e.arg(2));
        match tabs(e, tbc).uitabs.iter().position(|&t| t == tab) {
            Some(i) => select(e, tbc, i, false),
            None => Ok(()),
        }
    }),
];

const TBC: &str = "UITabBarController";
const MORE_LIST: &str = "UIMoreListController";

// ---- The More list ----

/// The More tab's list as drawn: a row per tab beyond the bar's four (in its view's coordinates).
#[derive(Clone, Debug)]
pub struct MoreList {
    pub rows: Vec<MoreRow>,
    pub glass: bool,
    /// The image tint.
    pub tint: [f64; 4],
}

#[derive(Clone, Debug)]
pub struct MoreRow {
    /// Its tab's index in the controller's tabs.
    pub index: usize,
    pub title: String,
    /// The tab's image (a template, in the tint).
    pub image: u64,
    pub badge: Option<String>,
    pub frame: [f64; 4],
}

/// Rows of 52 pt under Liquid Glass (44 before), from under the navigation bar (measured).
fn more_row_height(emu: &Emu) -> f64 {
    if emu.system.liquid_glass() { 52.0 } else { 44.0 }
}

/// A More list's rows, if the view is one.
pub fn more_list(emu: &mut Emu, v: u64) -> Result<Option<MoreList>> {
    let Some(&tbc) = emu.tabs.more_lists.get(&v) else { return Ok(None) };
    let t = tabs(emu, tbc);
    let top = crate::navigation::safe_area(emu, v)[0];
    let w = view(emu, v).bounds[2];
    let h = more_row_height(emu);
    let mut rows = Vec::new();
    for (k, (index, &vc)) in t.controllers.iter().enumerate().skip(MAX_BUTTONS - 1).enumerate() {
        let item = vc_item(emu, vc)?;
        let title = item_title(emu, item)?;
        let image = bag_get(emu, item, "image").unwrap_or(0);
        let badge = item_string(emu, item, "badgeValue");
        rows.push(MoreRow { index, title, image, badge, frame: [0.0, top + k as f64 * h, w, h] });
    }
    let tint = default_tint(emu, v)?;
    Ok(Some(MoreList { rows, glass: emu.system.liquid_glass(), tint }))
}

/// A finger lifted on the More list: on a row, its tab (pushed on the More navigation controller).
fn more_list_touches_ended(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let Some(p) = crate::touch::active_location(emu) else { return Ok(()) };
    let o = crate::uiview::window_origin(emu, v);
    let (x, y) = (p[0] - o[0], p[1] - o[1]);
    let Some(list) = more_list(emu, v)? else { return Ok(()) };
    let Some(row) = list.rows.iter().find(|r| x >= r.frame[0] && x < r.frame[0] + r.frame[2] && y >= r.frame[1] && y < r.frame[1] + r.frame[3]) else {
        return Ok(());
    };
    let tbc = emu.tabs.more_lists[&v];
    select(emu, tbc, row.index, true)
}

/// Choose a More list's row (a screen reader's activation).
pub fn more_list_tap(emu: &mut Emu, v: u64, row: usize) -> Result<()> {
    let Some(list) = more_list(emu, v)? else { return Ok(()) };
    let Some(r) = list.rows.get(row) else { return Ok(()) };
    let tbc = emu.tabs.more_lists[&v];
    select(emu, tbc, r.index, true)
}

// ---- UITabBar ----

/// A finger lifted on the bar: on a button, that tab (as UIKit's buttons do).
fn bar_touches_ended(emu: &mut Emu) -> Result<()> {
    let b = emu.arg(0);
    let Some(p) = crate::touch::active_location(emu) else { return Ok(()) };
    let o = crate::uiview::window_origin(emu, b);
    let (x, y) = (p[0] - o[0], p[1] - o[1]);
    let Some(look) = look(emu, b)? else { return Ok(()) };
    let Some(i) = look.buttons.iter().position(|btn| {
        let [bx, by, bw, bh] = btn.frame;
        x >= bx && x < bx + bw && y >= by && y < by + bh
    }) else {
        return Ok(());
    };
    tap(emu, b, i)
}

/// Tap the bar's `i`th button: its controller's tab, else the bar's own selection and delegate.
pub fn tap(emu: &mut Emu, b: u64, i: usize) -> Result<()> {
    let bs = bar(emu, b);
    if !bs.items.get(i).is_some_and(|&it| bag_get(emu, it, "disabled").unwrap_or(0) == 0) {
        return Ok(());
    }
    if bs.controller != 0 {
        let t = tabs(emu, bs.controller);
        return if t.overflows() && i == MAX_BUTTONS - 1 { select_more(emu, bs.controller) } else { select(emu, bs.controller, i, true) };
    }
    let item = bs.items[i];
    bar_mut(emu, b).selected = item;
    crate::host::changed(emu);
    if bs.delegate != 0 {
        let sel = objc::intern(emu, "tabBar:didSelectItem:")?;
        if objc::responds(emu, bs.delegate, sel)? {
            send(emu, bs.delegate, "tabBar:didSelectItem:", &[b, item])?;
        }
    }
    Ok(())
}

const BAR: &str = "UITabBar";

// ---- How the bar looks ----

/// The bar as drawn: in its own coordinates.
#[derive(Clone, Debug)]
pub struct Look {
    pub glass: bool,
    pub dark: bool,
    /// Liquid Glass's capsule the buttons sit on.
    pub platter: Option<[f64; 4]>,
    /// And the search tab's circle, if it has one.
    pub search_platter: Option<[f64; 4]>,
    /// The bar's fill (before iOS 26), and its hairline along the top.
    pub background: Option<[f64; 4]>,
    pub hairline: Option<[f64; 4]>,
    pub buttons: Vec<Button>,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub item: u64,
    pub frame: [f64; 4],
    pub title: String,
    /// The title's colour (an appearance's title attributes may differ from the icon's).
    pub title_color: [f64; 4],
    pub title_frame: [f64; 4],
    pub title_weight: f64,
    pub title_size: f64,
    /// The image to draw (a UIImage, a template in `color`), and where.
    pub image: u64,
    pub image_frame: [f64; 4],
    pub color: [f64; 4],
    pub selected: bool,
    pub enabled: bool,
    pub badge: Option<Badge>,
    /// The search tab on its own circle (Liquid Glass): its image only.
    pub detached: bool,
}

#[derive(Clone, Debug)]
pub struct Badge {
    pub text: String,
    pub frame: [f64; 4],
    pub color: [f64; 4],
    pub size: f64,
    pub weight: f64,
}

/// The item's image as the bar shows it: a symbol at the tab bar's size
/// (smaller on its side: the medium scale at 17.5 pt, fitted as upright's is).
fn tab_image(emu: &mut Emu, item: u64, selected: bool, side: bool) -> Result<u64> {
    let key_img = if selected { "selectedImage" } else { "image" };
    let mut img = bag_get(emu, item, key_img).unwrap_or(0);
    if img == 0 {
        img = bag_get(emu, item, "image").unwrap_or(0);
    }
    let Some(symbol) = emu.render.images.get(&img).and_then(|i| i.look.symbol.clone()) else { return Ok(img) };
    // Kept per item until its image changes.
    let (cache, source) = match (selected, side) {
        (true, false) => ("pommeSelectedTabImage", "pommeSelectedTabSource"),
        (false, false) => ("pommeTabImage", "pommeTabSource"),
        (true, true) => ("pommeSelectedSideTabImage", "pommeSelectedSideTabSource"),
        (false, true) => ("pommeSideTabImage", "pommeSideTabSource"),
    };
    if bag_get(emu, item, source) == Some(img)
        && let Some(i) = bag_get(emu, item, cache)
    {
        return Ok(i);
    }
    let size = if side { crate::symbols::Config { point_size: 17.5, scale: 2, ..Default::default() } } else { crate::symbols::Config { point_size: SYMBOL_POINTS, scale: 3, ..Default::default() } };
    let config = symbol.config.applying(&size);
    let sized = crate::symbols::image(emu, &symbol.name, config)?;
    if sized != 0 {
        send(emu, sized, "retain", &[])?;
    }
    if let Some(old) = bag_get(emu, item, cache).filter(|&o| o != 0) {
        objc::objc_release_obj(emu, old)?;
    }
    bag_set(emu, item, cache, sized);
    bag_set(emu, item, source, img);
    Ok(sized)
}

/// A view's tint: one set on it or a superview, else the system blue as the appearance has it.
fn default_tint(emu: &mut Emu, v: u64) -> Result<[f64; 4]> {
    let mut cur = v;
    while cur != 0 {
        if let Some(c) = bag_get(emu, cur, "tintColor").filter(|&c| c != 0) {
            return Ok(color_of(emu, c).unwrap_or([0.0, 0.533, 1.0, 1.0]));
        }
        cur = view(emu, cur).superview;
    }
    Ok(crate::appearance::system(emu, "systemBlueColor"))
}

fn color_of(emu: &mut Emu, c: u64) -> Option<[f64; 4]> {
    (c != 0).then(|| emu.ui.views.colors.get(&c).copied()).flatten()
}

fn text_width(emu: &mut Emu, text: &str, size: f64, weight: f64) -> f64 {
    crate::textkit::line_glyphs(emu, text, size, weight, [0.0; 4], 0.0, 0.0).1
}

// ---- Appearances (UITabBarAppearance's item appearances) ----

/// A lazily made sub-appearance kept on `owner` under `key`.
fn sub_appearance(emu: &mut Emu, owner: u64, key: &'static str, class: &str) -> Result<u64> {
    if let Some(a) = bag_get(emu, owner, key).filter(|&a| a != 0) {
        return Ok(a);
    }
    let a = new_object(emu, class)?;
    send(emu, a, "init", &[])?;
    bag_set(emu, owner, key, a);
    Ok(a)
}

/// An item's colours in a state, from the appearance that applies: the
/// item's own, else the bar's (at the scroll edge: its scroll-edge
/// appearance if it has one); the stacked layout upright, compact inline on
/// its side. (icon, title, badge background), each if set.
fn state_colors(emu: &mut Emu, b: u64, item: u64, selected: bool, side: bool) -> Result<(Option<[f64; 4]>, Option<[f64; 4]>, Option<[f64; 4]>)> {
    let appearance = [bag_get(emu, item, "scrollEdgeAppearance"), bag_get(emu, item, "standardAppearance"), bag_get(emu, b, "scrollEdgeAppearance"), bag_get(emu, b, "standardAppearance")]
        .into_iter()
        .flatten()
        .find(|&a| a != 0);
    let Some(a) = appearance else { return Ok((None, None, None)) };
    let layout = bag_get(emu, a, if side { "compactInlineLayoutAppearance" } else { "stackedLayoutAppearance" }).unwrap_or(0);
    if layout == 0 {
        return Ok((None, None, None));
    }
    let state = bag_get(emu, layout, if selected { "selected" } else { "normal" }).unwrap_or(0);
    if state == 0 {
        return Ok((None, None, None));
    }
    let icon = bag_get(emu, state, "iconColor").and_then(|c| color_of(emu, c));
    let badge = bag_get(emu, state, "badgeBackgroundColor").and_then(|c| color_of(emu, c));
    let attrs = bag_get(emu, state, "titleTextAttributes").unwrap_or(0);
    let title = if attrs != 0 {
        let key = foundation::new_nsstring(emu, "NSForegroundColorAttributeName")?;
        let c = send(emu, attrs, "objectForKey:", &[key])?;
        color_of(emu, c)
    } else {
        None
    };
    Ok((icon, title, badge))
}

/// `copy` of an item or state appearance: a new one of its class with what it holds.
fn copy_appearance(emu: &mut Emu) -> Result<()> {
    let from = emu.arg(0);
    let cls = emu.read_u64(from)?;
    let to = objc::create_instance(emu, cls, 0)?;
    objc::bag_copy(emu, from, to);
    ret(emu, to)
}

const ITEM_APPEARANCE: &str = "UITabBarItemAppearance";
const STATE_APPEARANCE: &str = "UITabBarItemStateAppearance";

macro_rules! sub_appearance_property {
    ($class:expr, $get:literal, $set:literal, $sub:expr) => {
        [
            ($class, false, $get, (|e: &mut Emu| {
                let a = sub_appearance(e, e.arg(0), $get, $sub)?;
                ret(e, a)
            }) as HostFn),
            ($class, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                set_object(e, o, $get, v, true)
            }) as HostFn),
        ]
    };
}

macro_rules! retained_property {
    ($class:expr, $get:literal, $set:literal, $copy:expr) => {
        [
            ($class, false, $get, (|e: &mut Emu| {
                let v = bag_get(e, e.arg(0), $get).unwrap_or(0);
                ret(e, v)
            }) as HostFn),
            ($class, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                set_object(e, o, $get, v, $copy)
            }) as HostFn),
        ]
    };
}

const APPEARANCE_PROPS: &[[(&str, bool, &str, HostFn); 2]] = &[
    sub_appearance_property!("UITabBarAppearance", "stackedLayoutAppearance", "setStackedLayoutAppearance:", ITEM_APPEARANCE),
    sub_appearance_property!("UITabBarAppearance", "inlineLayoutAppearance", "setInlineLayoutAppearance:", ITEM_APPEARANCE),
    sub_appearance_property!("UITabBarAppearance", "compactInlineLayoutAppearance", "setCompactInlineLayoutAppearance:", ITEM_APPEARANCE),
    sub_appearance_property!(ITEM_APPEARANCE, "normal", "setNormal:", STATE_APPEARANCE),
    sub_appearance_property!(ITEM_APPEARANCE, "selected", "setSelected:", STATE_APPEARANCE),
    sub_appearance_property!(ITEM_APPEARANCE, "disabled", "setDisabled:", STATE_APPEARANCE),
    sub_appearance_property!(ITEM_APPEARANCE, "focused", "setFocused:", STATE_APPEARANCE),
    retained_property!(STATE_APPEARANCE, "iconColor", "setIconColor:", false),
    retained_property!(STATE_APPEARANCE, "badgeBackgroundColor", "setBadgeBackgroundColor:", false),
    retained_property!(STATE_APPEARANCE, "titleTextAttributes", "setTitleTextAttributes:", true),
    retained_property!(STATE_APPEARANCE, "badgeTextAttributes", "setBadgeTextAttributes:", true),
];

/// The bar's background before iOS 26: its scroll-edge appearance's (the
/// content rests at the bottom edge, so that's the one that shows), else
/// transparent, as UIKit's default; an opaque bar tint fills it.
fn background(emu: &mut Emu, b: u64) -> (Option<[f64; 4]>, Option<[f64; 4]>) {
    if let Some(c) = bag_get(emu, b, "barTintColor").and_then(|c| color_of(emu, c)) {
        return (Some(c), Some([0.0, 0.0, 0.0, 0.3]));
    }
    let edge = bag_get(emu, b, "scrollEdgeAppearance").unwrap_or(0);
    if edge == 0 {
        return (None, None);
    }
    let bg = bag_get(emu, edge, "backgroundColor").and_then(|c| color_of(emu, c));
    let effect = bag_get(emu, edge, "backgroundEffect").unwrap_or(0);
    // (The default material, flattened: pomme doesn't blur.)
    let bg = bg.or((effect != 0).then_some([0.976, 0.976, 0.976, 0.94]));
    let shadow = bag_get(emu, edge, "shadowColor").and_then(|c| color_of(emu, c));
    (bg, shadow)
}

/// How a tab bar looks now, if it's one. Measured on the iOS 26 Simulator
/// (tests/fixtures/tabbar), held upright and on its side, in both designs:
///
/// - Liquid Glass, upright: 94-pt buttons overlapping by 8 on a capsule 4 pt
///   around them, centred, while that fits 21 pt in from each side; else the
///   capsule takes that width and the buttons (95 pt for four, 74 for five,
///   wider for longer titles) overlap evenly across it. Image above title.
/// - Liquid Glass, on its side: a 44-pt capsule, centred, its buttons sized to
///   image and title side by side (6, image, 8, title, 10), 4 pt apart.
/// - Before, upright: equal buttons 4 pt apart, 2 pt in from the edges.
/// - Before, on its side: buttons sharing the width (at most 228 pt each),
///   their content centred, image and title side by side 6 pt apart.
pub fn look(emu: &mut Emu, b: u64) -> Result<Option<Look>> {
    let Some(bs) = emu.tabs.bars.get(&b).cloned() else { return Ok(None) };
    let glass = emu.system.liquid_glass();
    let dark = emu.appearance.dark;
    let side = compact(emu);
    let w = view(emu, b).bounds[2];
    let n = bs.items.len();
    let tint = default_tint(emu, b)?;
    let unselected = match bag_get(emu, b, "unselectedItemTintColor").and_then(|c| color_of(emu, c)) {
        Some(c) => c,
        // (Measured: under Liquid Glass the label's colour as drawn on the glass; before, grey.)
        None if glass => {
            if dark {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.098, 0.098, 0.098, 1.0]
            }
        }
        None => [0.5725, 0.5725, 0.5725, 1.0],
    };
    let title_size = if side { 12.0 } else { TITLE_SIZE };
    // Titles' weights: Liquid Glass's selected tab semibold; otherwise medium upright, regular on its side.
    let weight_of = |selected: bool| -> f64 {
        if glass && selected {
            GLASS_SELECTED_WEIGHT
        } else if side {
            0.0
        } else {
            TITLE_WEIGHT
        }
    };
    let mut titles = Vec::with_capacity(n);
    let mut images = Vec::with_capacity(n);
    for &it in &bs.items {
        titles.push(item_title(emu, it)?);
        let image = tab_image(emu, it, it == bs.selected, side)?;
        let size = emu.render.images.get(&image).map_or([0.0; 2], |im| im.size);
        images.push((image, size));
    }
    // Each button's content width on its side: image, gap, title.
    let gap = if glass { 8.0 } else { 6.0 };
    let mut side_content = Vec::with_capacity(n);
    for (i, t) in titles.iter().enumerate() {
        let tw = text_width(emu, t, title_size, if glass { GLASS_SELECTED_WEIGHT } else { 0.0 });
        side_content.push(images[i].1[0] + gap + tw);
    }
    // Where the buttons go.
    let mut frames = Vec::with_capacity(n);
    let mut platter = None;
    let mut search_platter = None;
    // A search tab (`UISearchTab`) last: under Liquid Glass, upright, set apart.
    let search_tab = glass && !side && bs.items.last().is_some_and(|&it| bag_get(emu, it, "pommeSearchTab").unwrap_or(0) == 1);
    match (glass, side) {
        (true, false) => {
            // The tabs on the capsule (but a search tab, on its own circle at
            // the right, 62 pt across and 8 from the capsule: measured).
            let m = if search_tab { n - 1 } else { n };
            let max_w = w - 42.0 - if search_tab { 70.0 } else { 0.0 };
            // 94-pt buttons, 8 pt overlapping, if that leaves 8 pt spare; else
            // narrower ones, fitted to the room (measured: 95 pt for four tabs
            // in 340 pt, 74 for four in 270 or five in 340; three in 270 are
            // 94.7, here 95).
            let inner = max_w - 8.0;
            let fits = m == 0 || m as f64 * 94.0 - 8.0 * (m as f64 - 1.0) <= inner - 8.0;
            let base: f64 = if fits {
                94.0
            } else if inner / m as f64 >= 80.0 {
                95.0
            } else {
                74.0
            };
            let mut widths = Vec::with_capacity(m);
            for t in &titles[..m] {
                widths.push(base.max(text_width(emu, t, TITLE_SIZE, GLASS_SELECTED_WEIGHT) + 32.0));
            }
            let sum: f64 = widths.iter().sum();
            let (px, pw, overlap) = if fits {
                let pw = 8.0 + sum - 8.0 * (m.max(1) as f64 - 1.0);
                // (Beside a search tab, from the left; else centred.)
                (if search_tab { 21.0 } else { (w - pw) / 2.0 }, pw, 8.0)
            } else {
                (21.0, max_w, (sum - inner) / (m as f64 - 1.0).max(1.0))
            };
            if m > 0 {
                platter = Some([px, 0.0, pw, 62.0]);
            }
            let mut x = px + 4.0;
            for bw in widths {
                frames.push([x, 4.0, bw, 54.0]);
                x += bw - overlap;
            }
            if search_tab {
                let circle = [w - 21.0 - 62.0, 0.0, 62.0, 62.0];
                search_platter = Some(circle);
                frames.push(circle);
            }
        }
        (true, true) => {
            let widths: Vec<f64> = side_content.iter().map(|c| 6.0 + c + 10.0).collect();
            let pw = 8.0 + widths.iter().sum::<f64>() + 4.0 * (n.max(1) as f64 - 1.0);
            let px = (w - pw) / 2.0;
            platter = Some([px, 0.0, pw, 44.0]);
            let mut x = px + 4.0;
            for bw in widths {
                frames.push([x, 4.0, bw, 36.0]);
                x += bw + 4.0;
            }
        }
        (false, false) => {
            let bw = (w - 4.0 - 4.0 * (n as f64 - 1.0).max(0.0)) / n.max(1) as f64;
            let xs: Vec<f64> = (0..=n).map(|i| if i == n { w - 2.0 + 4.0 } else { (2.0 + i as f64 * (bw + 4.0)).round() }).collect();
            for i in 0..n {
                frames.push([xs[i], 1.0, xs[i + 1] - 4.0 - xs[i], 48.0]);
            }
        }
        (false, true) => {
            let span = (w - 88.0).min(n as f64 * 228.0 - 4.0).max(0.0);
            let content: f64 = side_content.iter().sum();
            let pad = ((span - 4.0 * (n.max(1) as f64 - 1.0) - content) / n.max(1) as f64).max(0.0);
            let mut x = ((w - span) / 2.0).floor();
            for c in &side_content {
                frames.push([x, 1.0, c + pad, 31.0]);
                x += c + pad + 4.0;
            }
        }
    }
    let mut buttons = Vec::with_capacity(n);
    for (i, &item) in bs.items.iter().enumerate() {
        let selected = item == bs.selected;
        let enabled = bag_get(emu, item, "disabled").unwrap_or(0) == 0;
        let frame = frames[i];
        let (image, size) = images[i];
        let weight = weight_of(selected);
        let tw = text_width(emu, &titles[i], title_size, weight);
        let th = if side { 14.33 } else { 12.0 };
        let (image_frame, title_frame) = if side {
            // Image and title side by side, centred in the button (Liquid Glass: from 6 pt in).
            let x0 = if glass { frame[0] + 6.0 } else { frame[0] + (frame[2] - side_content[i]) / 2.0 };
            let mid = frame[1] + frame[3] / 2.0;
            let img = [x0, mid - size[1] / 2.0, size[0], size[1]];
            (img, [x0 + size[0] + gap, mid - th / 2.0, tw, th])
        } else {
            // Image centred 17.7 pt down the button (20 under Liquid Glass); title
            // below. (The search tab's circle: its image in the middle, no title.)
            let detached = search_tab && i == n - 1;
            let (image_mid, title_top) = if detached { (31.0, 62.0) } else if glass { (20.0, 35.0) } else { (17.7, 34.33) };
            let img = [frame[0] + (frame[2] - size[0]) / 2.0, frame[1] + image_mid - size[1] / 2.0, size[0], size[1]];
            (img, [frame[0] + (frame[2] - tw) / 2.0, frame[1] + title_top, tw, th])
        };
        // An appearance's colours override the tints.
        let (icon_c, title_c, badge_c) = state_colors(emu, b, item, selected, side)?;
        let mut color = icon_c.unwrap_or(if selected { tint } else { unselected });
        // (Measured: under Liquid Glass an unselected title keeps its colour
        // whatever the appearance says; the icon follows it.)
        let title_c = if glass && !selected { None } else { title_c };
        let mut title_color = title_c.unwrap_or(if glass && !selected { unselected } else { color });
        if !enabled {
            color[3] *= 0.35;
            title_color[3] *= 0.35;
        }
        // A badge: red, white text, at least round; at the image's top right
        // (on its side under Liquid Glass, the button's).
        let badge = match item_string(emu, item, "badgeValue") {
            Some(text) => {
                let (bsize, bweight, h, pad): (f64, f64, f64, f64) = match (glass, side) {
                    (true, _) => (12.0, 0.23, 18.33, 4.0),
                    (false, false) => (13.0, 0.0, 18.0, 5.0),
                    (false, true) => (10.0, 0.23, 14.0, 4.0),
                };
                let bw = h.max(text_width(emu, &text, bsize, bweight) + 2.0 * pad);
                let red = bag_get(emu, item, "badgeColor").and_then(|c| color_of(emu, c)).or(badge_c).unwrap_or_else(|| crate::appearance::system(emu, "systemRedColor"));
                let origin = match (glass, side) {
                    (true, true) => [frame[0] + frame[2] - 20.33, frame[1]],
                    (false, true) => [image_frame[0] + image_frame[2] - 9.0, frame[1] + 1.0],
                    _ => [image_frame[0] + image_frame[2] - 5.0, frame[1] + 2.0],
                };
                Some(Badge { text, frame: [origin[0], origin[1], bw, h], color: red, size: bsize, weight: bweight })
            }
            None => None,
        };
        let detached = search_tab && i == n - 1;
        buttons.push(Button { item, frame, title: titles[i].clone(), title_color, title_frame, title_weight: weight, title_size, image, image_frame, color, selected, enabled, badge, detached });
    }
    let (background, hairline) = if glass { (None, None) } else { background(emu, b) };
    let mut look = Look { glass, dark, platter, search_platter, background, hairline, buttons };
    if crate::direction::view_rtl(emu, b)? {
        mirror(&mut look, w);
    }
    Ok(Some(look))
}

/// The bar in a right-to-left layout: everything mirrored across it, the
/// tabs in reverse order (fixtures/tabbar/ios26-ar.txt). A badge sits at
/// its image's top left, a point further in than a mirror puts it.
fn mirror(look: &mut Look, w: f64) {
    let flip = |r: &mut [f64; 4]| r[0] = w - r[0] - r[2];
    for r in [&mut look.platter, &mut look.search_platter].into_iter().flatten() {
        flip(r);
    }
    for b in &mut look.buttons {
        flip(&mut b.frame);
        flip(&mut b.title_frame);
        flip(&mut b.image_frame);
        if let Some(badge) = &mut b.badge {
            flip(&mut badge.frame);
            badge.frame[0] += 1.0;
        }
    }
}

/// A tab bar's selected button and count, for accessibility.
pub fn buttons(emu: &mut Emu, b: u64) -> Result<Vec<Button>> {
    Ok(look(emu, b)?.map(|l| l.buttons).unwrap_or_default())
}

// ---- methods ----

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut m: Vec<_> = ITEM_PROPS.iter().chain(APPEARANCE_PROPS).chain(TAB_PROPS).flatten().copied().collect();
    m.extend_from_slice(METHODS);
    m.extend_from_slice(TAB_METHODS);
    m
}

const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // UITabBarItem
    (ITEM, false, "initWithTitle:image:tag:", |e| {
        let (i, t, img, tag) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        item_init(e, i, t, img, 0, tag)
    }),
    (ITEM, false, "initWithTitle:image:selectedImage:", |e| {
        let (i, t, img, sel) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        item_init(e, i, t, img, sel, 0)
    }),
    (ITEM, false, "initWithTabBarSystemItem:tag:", |e| {
        let (i, kind, tag) = (e.arg(0), e.arg(2), e.arg(3));
        let (title, symbol) = system_item(kind);
        let t = foundation::new_nsstring(e, title)?;
        let img = crate::symbols::image(e, symbol, Default::default())?;
        item_init(e, i, t, img, 0, tag)
    }),
    (ITEM, false, "tag", |e| {
        let v = bag_get(e, e.arg(0), "tag").unwrap_or(0);
        ret(e, v)
    }),
    (ITEM, false, "setTag:", |e| {
        let (i, v) = (e.arg(0), e.arg(2));
        bag_set(e, i, "tag", v);
        Ok(())
    }),
    (ITEM, false, "isEnabled", |e| {
        let v = bag_get(e, e.arg(0), "disabled").unwrap_or(0) == 0;
        ret(e, v as u64)
    }),
    (ITEM, false, "setEnabled:", |e| {
        let (i, v) = (e.arg(0), e.arg(2) & 0xff);
        bag_set(e, i, "disabled", (v == 0) as u64);
        crate::host::changed(e);
        Ok(())
    }),
    // UIViewController
    ("UIViewController", false, "tabBarItem", |e| {
        let i = vc_item(e, e.arg(0))?;
        ret(e, i)
    }),
    ("UIViewController", false, "setTabBarItem:", |e| {
        let (vc, item) = (e.arg(0), e.arg(2));
        if item != 0 {
            send(e, item, "retain", &[])?;
            bag_set(e, item, "pommeController", vc);
        }
        bag_set(e, vc, "tabBarItem", item);
        let tbc = tab_bar_controller_of(e, vc);
        if tbc != 0 {
            sync_items(e, tbc)?;
        }
        Ok(())
    }),
    ("UIViewController", false, "hidesBottomBarWhenPushed", |e| {
        let v = bag_get(e, e.arg(0), "hidesBottomBarWhenPushed").unwrap_or(0);
        ret(e, v)
    }),
    ("UIViewController", false, "setHidesBottomBarWhenPushed:", |e| {
        let (vc, v) = (e.arg(0), e.arg(2) & 0xff);
        bag_set(e, vc, "hidesBottomBarWhenPushed", v);
        Ok(())
    }),
    // The More tab's list: its view is drawn by pomme (render.rs), a row per tab in it.
    (MORE_LIST, false, "loadView", |e| {
        let list = e.arg(0);
        let v = new_object(e, "UIMoreListView")?;
        send(e, v, "init", &[])?;
        // (On the system background, as UIKit's table is.)
        let cls = host_class(e, "UIColor")?;
        let bg = send(e, cls, "systemBackgroundColor", &[])?;
        send(e, v, "setBackgroundColor:", &[bg])?;
        let tbc = bag_get(e, list, "pommeTabBarController").unwrap_or(0);
        e.tabs.more_lists.insert(v, tbc);
        send(e, list, "setView:", &[v])?;
        Ok(())
    }),
    ("UIMoreListView", false, "touchesBegan:withEvent:", |_| Ok(())),
    ("UIMoreListView", false, "touchesMoved:withEvent:", |_| Ok(())),
    ("UIMoreListView", false, "touchesCancelled:withEvent:", |_| Ok(())),
    ("UIMoreListView", false, "touchesEnded:withEvent:", more_list_touches_ended),
    ("UIViewController", false, "tabBarController", |e| {
        let t = tab_bar_controller_of(e, e.arg(0));
        ret(e, t)
    }),
    // UITabBarController
    (TBC, false, "loadView", |e| {
        let tbc = e.arg(0);
        ensure_view(e, tbc)?;
        Ok(())
    }),
    (TBC, false, "tabBar", |e| {
        let b = ensure_bar(e, e.arg(0))?;
        ret(e, b)
    }),
    (TBC, false, "viewControllers", |e| {
        let t = tabs(e, e.arg(0));
        // (Measured: none, when the tabs are `UITab`s.)
        let list = if t.uitabs.is_empty() { t.controllers } else { Vec::new() };
        let a = if list.is_empty() { 0 } else { foundation::new_array_of(e, &list)? };
        ret(e, a)
    }),
    (TBC, false, "setViewControllers:", set_view_controllers),
    (TBC, false, "setViewControllers:animated:", set_view_controllers),
    (TBC, false, "selectedIndex", |e| {
        // (NSNotFound with no tabs.)
        let i = tabs(e, e.arg(0)).selected.map_or(i64::MAX as u64, |i| i as u64);
        ret(e, i)
    }),
    (TBC, false, "setSelectedIndex:", |e| {
        let (tbc, i) = (e.arg(0), e.arg(2));
        select(e, tbc, i as usize, false)
    }),
    (TBC, false, "selectedViewController", |e| {
        let t = tabs(e, e.arg(0));
        let vc = if t.more_selected { t.more } else { t.selected.and_then(|i| t.controllers.get(i).copied()).unwrap_or(0) };
        ret(e, vc)
    }),
    (TBC, false, "moreNavigationController", |e| {
        let nav = more_navigation(e, e.arg(0))?;
        ret(e, nav)
    }),
    (TBC, false, "setSelectedViewController:", |e| {
        let (tbc, vc) = (e.arg(0), e.arg(2));
        let t = tabs(e, tbc);
        match t.controllers.iter().position(|&v| v == vc) {
            Some(i) => select(e, tbc, i, false),
            // The More tab's list.
            None if vc != 0 && vc == t.more => {
                let tm = tabs_mut(e, tbc);
                tm.more_selected = true;
                tm.selected = None;
                sync_items(e, tbc)?;
                show_selected(e, tbc)
            }
            None => Ok(()),
        }
    }),
    (TBC, false, "delegate", |e| {
        let d = tabs(e, e.arg(0)).delegate;
        ret(e, d)
    }),
    (TBC, false, "setDelegate:", |e| {
        let (tbc, d) = (e.arg(0), e.arg(2));
        tabs_mut(e, tbc).delegate = d;
        Ok(())
    }),
    // UITabBar
    (BAR, false, "items", |e| {
        let items = bar(e, e.arg(0)).items;
        let a = if items.is_empty() { 0 } else { foundation::new_array_of(e, &items)? };
        ret(e, a)
    }),
    (BAR, false, "setItems:", bar_set_items),
    (BAR, false, "setItems:animated:", bar_set_items),
    (BAR, false, "selectedItem", |e| {
        let s = bar(e, e.arg(0)).selected;
        ret(e, s)
    }),
    (BAR, false, "setSelectedItem:", |e| {
        let (b, item) = (e.arg(0), e.arg(2));
        bar_mut(e, b).selected = item;
        crate::host::changed(e);
        Ok(())
    }),
    (BAR, false, "delegate", |e| {
        let d = bar(e, e.arg(0)).delegate;
        ret(e, d)
    }),
    (BAR, false, "setDelegate:", |e| {
        let (b, d) = (e.arg(0), e.arg(2));
        bar_mut(e, b).delegate = d;
        Ok(())
    }),
    (BAR, false, "unselectedItemTintColor", |e| {
        let v = bag_get(e, e.arg(0), "unselectedItemTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (BAR, false, "setUnselectedItemTintColor:", |e| {
        let (b, c) = (e.arg(0), e.arg(2));
        set_object(e, b, "unselectedItemTintColor", c, false)
    }),
    (BAR, false, "barTintColor", |e| {
        let v = bag_get(e, e.arg(0), "barTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (BAR, false, "setBarTintColor:", |e| {
        let (b, c) = (e.arg(0), e.arg(2));
        set_object(e, b, "barTintColor", c, false)
    }),
    // Item appearances copy what they hold (as UIBarAppearance does).
    (ITEM_APPEARANCE, false, "copy", copy_appearance),
    (ITEM_APPEARANCE, false, "copyWithZone:", copy_appearance),
    (STATE_APPEARANCE, false, "copy", copy_appearance),
    (STATE_APPEARANCE, false, "copyWithZone:", copy_appearance),
    // iOS 26's: the bar minimises as content scrolls (pomme keeps it whole).
    (TBC, false, "tabBarMinimizeBehavior", |e| {
        let v = bag_get(e, e.arg(0), "tabBarMinimizeBehavior").unwrap_or(0);
        ret(e, v)
    }),
    (TBC, false, "setTabBarMinimizeBehavior:", |e| {
        let (t, v) = (e.arg(0), e.arg(2));
        bag_set(e, t, "tabBarMinimizeBehavior", v);
        Ok(())
    }),
    (BAR, false, "standardAppearance", |e| {
        let v = bag_get(e, e.arg(0), "standardAppearance").unwrap_or(0);
        ret(e, v)
    }),
    (BAR, false, "setStandardAppearance:", |e| {
        let (b, a) = (e.arg(0), e.arg(2));
        set_object(e, b, "standardAppearance", a, true)
    }),
    (BAR, false, "scrollEdgeAppearance", |e| {
        let v = bag_get(e, e.arg(0), "scrollEdgeAppearance").unwrap_or(0);
        ret(e, v)
    }),
    (BAR, false, "setScrollEdgeAppearance:", |e| {
        let (b, a) = (e.arg(0), e.arg(2));
        set_object(e, b, "scrollEdgeAppearance", a, true)
    }),
    (BAR, false, "isTranslucent", |e| {
        let v = bag_get(e, e.arg(0), "opaque").unwrap_or(0) == 0;
        ret(e, v as u64)
    }),
    (BAR, false, "setTranslucent:", |e| {
        let (b, v) = (e.arg(0), e.arg(2) & 0xff);
        bag_set(e, b, "opaque", (v == 0) as u64);
        Ok(())
    }),
    (BAR, false, "sizeThatFits:", |e| {
        let w = d(e, 0);
        let h = bar_height(e) + crate::orientation::safe_area(e)[2];
        ret_size(e, w, h);
        Ok(())
    }),
    (BAR, false, "touchesBegan:withEvent:", |_| Ok(())),
    (BAR, false, "touchesMoved:withEvent:", |_| Ok(())),
    (BAR, false, "touchesEnded:withEvent:", bar_touches_ended),
    (BAR, false, "touchesCancelled:withEvent:", |_| Ok(())),
];

fn bar_set_items(emu: &mut Emu) -> Result<()> {
    let (b, a) = (emu.arg(0), emu.arg(2));
    let items = if a == 0 { Vec::new() } else { foundation::array_values(emu, a)? };
    for &it in &items {
        send(emu, it, "retain", &[])?;
    }
    let bs = bar_mut(emu, b);
    if !items.contains(&bs.selected) {
        bs.selected = 0;
    }
    bs.items = items;
    crate::host::changed(emu);
    Ok(())
}
