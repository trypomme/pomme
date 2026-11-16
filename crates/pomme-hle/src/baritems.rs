//! Bar button items in the navigation bar: a navigation item's leading and
//! trailing items (system items, titles, images, custom views), laid out,
//! drawn and tapped as iOS does, in both designs.
//!
//! The numbers are the iOS 26 Simulator's (fixtures/baritems, light). Under
//! Liquid Glass each item sits on its own glass capsule, 44 pt tall at the
//! top of the bar's row, 12 pt from the next and 16 pt from the edge; the
//! item's own area is inset 4 pt in it: a 36-pt square for a symbol, an
//! image or a custom view (at least), its title's width and 16 for a title.
//! `hidesSharedBackground` leaves an item no capsule (its area 44 tall, its
//! title 24 wider). A prominent item is blue glass, its content light cyan;
//! the system Done item is prominent of itself, a checkmark. Add is a plus,
//! Edit a title. In the design before iOS 26 the
//! items are tinted titles and symbols without a background, the trailing
//! one ending 16 pt from the edge; Done is a semibold title.
//!
//! In a right-to-left layout the items are mirrored (trailing on the left).

use crate::foundation::{self, nsstring};
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// What an item shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// A symbol, at this size.
    Symbol(String, [f64; 2]),
    /// A title, at this weight.
    Title(String, f64),
    /// An image (a UIImage), at this size.
    Image(u64, [f64; 2]),
    /// The app's own view.
    Custom(u64),
}

/// An item as the bar shows it, in the bar's coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct BarItem {
    pub item: u64,
    /// Its glass capsule (Liquid Glass, unless it hides its background).
    pub capsule: Option<[f64; 4]>,
    /// Where a tap hits it.
    pub area: [f64; 4],
    pub content: Content,
    pub content_frame: [f64; 4],
    pub prominent: bool,
    pub enabled: bool,
}

#[derive(Clone, Default)]
pub struct State {
    /// (navigation item, trailing?) → its items.
    items: HashMap<(u64, bool), Vec<u64>>,
    /// Navigation bar → the custom views it shows.
    custom: HashMap<u64, Vec<u64>>,
    /// View controller → its toolbar items.
    toolbar_items: HashMap<u64, Vec<u64>>,
    /// Navigation controller → its toolbar (a UIToolbar view), and whether it's hidden.
    toolbars: HashMap<u64, u64>,
    toolbar_shown: HashMap<u64, bool>,
}

// ---- measured ----

const CAPSULE: f64 = 44.0;
const GAP: f64 = 12.0;
const EDGE: f64 = 16.0;
const INSET: f64 = 4.0;
const ICON_AREA: f64 = 36.0;
pub const GLASS_FILL: [f64; 4] = [253.0 / 255.0, 253.0 / 255.0, 253.0 / 255.0, 1.0];
pub const GLASS_CONTENT: [f64; 4] = [33.0 / 255.0, 33.0 / 255.0, 33.0 / 255.0, 1.0];
pub const PROMINENT_FILL: [f64; 4] = [0.0, 157.0 / 255.0, 254.0 / 255.0, 1.0];
pub const PROMINENT_CONTENT: [f64; 4] = [146.0 / 255.0, 253.0 / 255.0, 1.0, 1.0];
pub const TINT: [f64; 4] = [0.0, 136.0 / 255.0, 1.0, 1.0];
/// A symbol image item's symbol (the gear: 27.67 square).
const IMAGE_SYMBOL: f64 = 83.0 / 3.0;
const SEMIBOLD: f64 = 0.3;

/// UIBarButtonSystemItem → what it shows: Done 0, Cancel 1, Edit 2, Save 3,
/// Add 4, flexible 5 and fixed 6 space, Compose 7, Reply 8, Action 9,
/// Organize 10, Bookmarks 11, Search 12, Refresh 13, Stop 14, Camera 15,
/// Trash 16, Play 17, Pause 18, Rewind 19, Fast forward 20, Undo 21, Redo 22,
/// Close 24. (Recorded: Done, Add and Edit, in both designs.)
fn system_content(emu: &mut Emu, kind: u64, glass: bool) -> Option<Content> {
    let symbol = |name: &str, size: [f64; 2]| Some(Content::Symbol(name.to_string(), size));
    let natural = |emu: &mut Emu, name: &str| -> Option<Content> {
        let m = crate::symbols::metrics(name, &Default::default(), emu.device.scale);
        Some(Content::Symbol(name.to_string(), [m[0], m[1]]))
    };
    match kind {
        0 if glass => symbol("checkmark", [24.0, 68.0 / 3.0]),
        0 => Some(Content::Title("Done".into(), SEMIBOLD)),
        1 => Some(Content::Title("Cancel".into(), 0.0)),
        2 => Some(Content::Title("Edit".into(), 0.0)),
        3 => Some(Content::Title("Save".into(), if glass { 0.0 } else { SEMIBOLD })),
        4 if glass => symbol("plus", [70.0 / 3.0, 22.0]),
        4 => symbol("plus", [23.0, 65.0 / 3.0]),
        5 | 6 => None,
        7 => natural(emu, "square.and.pencil"),
        8 => natural(emu, "arrowshape.turn.up.left"),
        9 => natural(emu, "square.and.arrow.up"),
        10 => natural(emu, "folder"),
        11 => natural(emu, "book"),
        12 => natural(emu, "magnifyingglass"),
        13 => natural(emu, "arrow.clockwise"),
        14 | 24 => natural(emu, "xmark"),
        15 => natural(emu, "camera"),
        16 => natural(emu, "trash"),
        17 => natural(emu, "play.fill"),
        18 => natural(emu, "pause.fill"),
        19 => natural(emu, "backward.fill"),
        20 => natural(emu, "forward.fill"),
        21 => natural(emu, "arrow.uturn.backward"),
        22 => natural(emu, "arrow.uturn.forward"),
        _ => None,
    }
}

fn content_of(emu: &mut Emu, item: u64, glass: bool) -> Result<Option<Content>> {
    let custom = objc::bag_get(emu, item, "customView").unwrap_or(0);
    if custom != 0 {
        return Ok(Some(Content::Custom(custom)));
    }
    if let Some(kind) = objc::bag_get(emu, item, "pomme.systemItem") {
        return Ok(system_content(emu, kind, glass));
    }
    let image = objc::bag_get(emu, item, "image").unwrap_or(0);
    if image != 0 {
        let symbol = emu.render.images.get(&image).is_some_and(|i| i.look.symbol.is_some());
        let s = emu.render.images.get(&image).map_or([24.0, 24.0], |i| i.size);
        let size = if symbol && glass { [IMAGE_SYMBOL, IMAGE_SYMBOL * s[1] / s[0].max(1.0)] } else { s };
        return Ok(Some(Content::Image(image, size)));
    }
    let title = objc::bag_get(emu, item, "title").unwrap_or(0);
    if title != 0 {
        let style = objc::bag_get(emu, item, "style").unwrap_or(0);
        // (UIBarButtonItemStyleDone: semibold, before iOS 26.)
        let weight = if style == 2 && !glass { SEMIBOLD } else { 0.0 };
        return Ok(Some(Content::Title(nsstring(emu, title)?, weight)));
    }
    Ok(None)
}

fn content_size(emu: &mut Emu, c: &Content) -> [f64; 2] {
    match c {
        Content::Symbol(_, s) | Content::Image(_, s) => *s,
        Content::Title(t, w) => [crate::navigation::text_width(emu, t, *w), 61.0 / 3.0],
        Content::Custom(v) => {
            let b = crate::uiview::view(emu, *v).bounds;
            [b[2], b[3]]
        }
    }
}

/// The items of a navigation item's side.
pub(crate) fn items_of(emu: &Emu, nav_item: u64, trailing: bool) -> Vec<u64> {
    emu.bar_items.items.get(&(nav_item, trailing)).cloned().unwrap_or_default()
}

/// Lay out a navigation item's items in a bar `width` wide: `leading` its
/// leading inset, `back` whether a back button takes the leading edge.
pub(crate) fn layout(emu: &mut Emu, nav_item: u64, width: f64, leading: f64, back: bool, glass: bool) -> Result<Vec<BarItem>> {
    let mut out = Vec::new();
    for trailing in [true, false] {
        let mut edge = if trailing { width - EDGE - leading } else if back && glass { EDGE + leading + CAPSULE + GAP } else { EDGE + leading };
        if back && !glass && !trailing {
            // (Before iOS 26: after the back button's chevron and label; not recorded.)
            edge += 64.0;
        }
        for item in items_of(emu, nav_item, trailing) {
            let Some(content) = content_of(emu, item, glass)? else { continue };
            let size = content_size(emu, &content);
            let bare = objc::bag_get(emu, item, "hidesSharedBackground").unwrap_or(0) & 0xff != 0;
            let done = objc::bag_get(emu, item, "pomme.systemItem") == Some(0);
            let prominent = glass && (objc::bag_get(emu, item, "style").unwrap_or(0) == 2 || done);
            let enabled = objc::bag_get(emu, item, "enabled").is_none_or(|e| e & 0xff != 0);
            let is_title = matches!(content, Content::Title(..));
            let b = if glass {
                // Its area, and its capsule 4 pt around it.
                let area_w = if is_title { size[0] + if bare { 24.0 } else { 16.0 } } else { size[0].max(ICON_AREA) };
                let area_h = if bare { CAPSULE } else if matches!(content, Content::Custom(_)) { size[1] } else { ICON_AREA };
                let cap_w = area_w + 2.0 * INSET;
                let x = if trailing { edge - cap_w } else { edge };
                edge = if trailing { x - GAP } else { x + cap_w + GAP };
                let area = [x + INSET, (CAPSULE - area_h) / 2.0, area_w, area_h];
                let mut cf = [area[0] + (area_w - size[0]) / 2.0, (CAPSULE - size[1]) / 2.0, size[0], size[1]];
                if matches!(content, Content::Symbol(..)) {
                    // (A symbol sits a third of a point low.)
                    cf[1] += 1.0 / 3.0;
                }
                if matches!(content, Content::Custom(_)) {
                    cf = area;
                }
                BarItem { item, capsule: (!bare).then_some([x, 0.0, cap_w, CAPSULE]), area, content, content_frame: cf, prominent, enabled }
            } else {
                // Before iOS 26: the content, with a tap area around it (a
                // symbol's 11 pt before it, everything's 8 pt after).
                let x = if trailing { edge - size[0] } else { edge };
                let (before, after) = if is_title { (0.0, 8.0) } else { (11.0, 8.0) };
                let (before, after) = if trailing { (before, after) } else { (after, 3.0) };
                edge = if trailing { x - before - after } else { x + size[0] + before + after };
                let cf = [x, (CAPSULE - size[1]) / 2.0, size[0], size[1]];
                BarItem { item, capsule: None, area: [x - before, 0.0, size[0] + before + after, CAPSULE], content, content_frame: cf, prominent: false, enabled }
            };
            out.push(b);
        }
    }
    Ok(out)
}

/// The items mirrored across a bar `width` wide (right to left).
pub(crate) fn mirror(items: &mut [BarItem], width: f64) {
    let flip = |r: &mut [f64; 4]| r[0] = width - r[0] - r[2];
    for i in items {
        if let Some(c) = &mut i.capsule {
            flip(c);
        }
        flip(&mut i.area);
        flip(&mut i.content_frame);
    }
}

/// Put the custom views a bar shows in it, where its items go (and take out
/// those it no longer shows).
pub(crate) fn place_custom(emu: &mut Emu, bar: u64, items: &[BarItem], row_top: f64) -> Result<()> {
    let mut now = Vec::new();
    for i in items {
        if let Content::Custom(v) = i.content {
            if crate::uiview::view(emu, v).superview != bar {
                send(emu, bar, "addSubview:", &[v])?;
            }
            let f = i.content_frame;
            let r = [f[0], f[1] + row_top, f[2], f[3]];
            if crate::uiview::frame_of(emu, v) != r {
                crate::uiview::set_frame_of(emu, v, r);
            }
            now.push(v);
        }
    }
    let before = emu.bar_items.custom.insert(bar, now.clone()).unwrap_or_default();
    for v in before {
        if !now.contains(&v) && crate::uiview::view(emu, v).superview == bar {
            send(emu, v, "removeFromSuperview", &[])?;
        }
    }
    Ok(())
}

/// A tap at (`x`, `y`) (bar coordinates) on one of these items: send its
/// action. Whether it hit one.
pub(crate) fn tap(emu: &mut Emu, items: &[BarItem], x: f64, y: f64) -> Result<bool> {
    let Some(hit) = items.iter().find(|i| x >= i.area[0] - INSET && x < i.area[0] + i.area[2] + INSET && y >= i.area[1] - INSET && y < i.area[1] + i.area[3] + INSET) else { return Ok(false) };
    if !hit.enabled || matches!(hit.content, Content::Custom(_)) {
        return Ok(true);
    }
    let (target, action) = (objc::bag_get(emu, hit.item, "target").unwrap_or(0), objc::bag_get(emu, hit.item, "action").unwrap_or(0));
    if action == 0 {
        return Ok(true);
    }
    let sel = objc::sel_name(emu, action);
    if target != 0 {
        send(emu, target, &sel, &[hit.item])?;
    } else {
        // No target: the responder chain (UIApplication sends it on).
        let app = emu.ui.app;
        send(emu, app, "sendAction:to:from:forEvent:", &[action, 0, hit.item, 0])?;
    }
    Ok(true)
}

/// What VoiceOver reads for an item: its accessibility label, else its title,
/// else a system item's name (iOS's: "Add", "Done", …); None for a custom
/// view (it is its own element).
pub(crate) fn accessibility_label(emu: &mut Emu, b: &BarItem) -> Result<Option<String>> {
    if matches!(b.content, Content::Custom(_)) {
        return Ok(None);
    }
    for key in ["accessibilityLabel", "title"] {
        let s = objc::bag_get(emu, b.item, key).unwrap_or(0);
        if s != 0 {
            return Ok(Some(nsstring(emu, s)?));
        }
    }
    const NAMES: [&str; 25] = ["Done", "Cancel", "Edit", "Save", "Add", "", "", "Compose", "Reply", "Share", "Organize", "Bookmarks", "Search", "Refresh", "Stop", "Camera", "Delete", "Play", "Pause", "Rewind", "Fast Forward", "Undo", "Redo", "", "Close"];
    let name = objc::bag_get(emu, b.item, "pomme.systemItem").and_then(|k| NAMES.get(k as usize)).filter(|n| !n.is_empty());
    Ok(Some(match (name, &b.content) {
        (Some(n), _) => n.to_string(),
        (None, Content::Title(t, _)) => t.clone(),
        _ => String::new(),
    }))
}

// ---- the navigation controller's toolbar ----
//
// Recorded (fixtures/baritems, `toolbar`): under Liquid Glass each item on
// its own capsule, 48 pt tall, its bottom 28 pt above the screen's, 28 pt
// from the sides; a symbol's capsule round, a title's its text and 27. The
// flexible spaces split the items into groups: the first at the leading
// side, the last at the trailing side, those between centred. The screen's
// safe area ends at the capsules' top (76 pt up on the iPhone 16e). Before
// iOS 26: a 49-pt bar above the home indicator, the content without
// backgrounds 16 pt from the sides, centred 21.5 pt down the bar.

const TOOLBAR_CAPSULE: f64 = 48.0;
const TOOLBAR_GAP_BELOW: f64 = 28.0;
const TOOLBAR_SIDE: f64 = 28.0;
const TOOLBAR_TITLE_PAD: f64 = 27.0;
const TOOLBAR_HEIGHT: f64 = 49.0;
const TOOLBAR_CONTENT_MID: f64 = 21.5;

/// The toolbar's height above the screen's bottom safe area (what it covers of the content).
fn toolbar_cover(glass: bool, bottom_safe: f64) -> f64 {
    if glass { (TOOLBAR_GAP_BELOW + TOOLBAR_CAPSULE - bottom_safe).max(0.0) } else { TOOLBAR_HEIGHT }
}

/// The toolbar items of a navigation controller's top screen, laid out in a
/// toolbar `width` wide.
fn toolbar_layout(emu: &mut Emu, items: &[u64], width: f64, glass: bool) -> Result<Vec<BarItem>> {
    // Groups between flexible spaces: (item, content, width) each.
    let mut groups: Vec<Vec<(u64, Content, f64)>> = vec![Vec::new()];
    for &item in items {
        match objc::bag_get(emu, item, "pomme.systemItem") {
            Some(5) => {
                groups.push(Vec::new());
                continue;
            }
            Some(6) => continue,
            _ => {}
        }
        let Some(content) = content_of(emu, item, glass)? else { continue };
        let size = content_size(emu, &content);
        let w = if !glass {
            size[0]
        } else if matches!(content, Content::Title(..)) {
            size[0] + TOOLBAR_TITLE_PAD
        } else {
            size[0].max(TOOLBAR_CAPSULE)
        };
        groups.last_mut().unwrap().push((item, content, w));
    }
    let side = if glass { TOOLBAR_SIDE } else { EDGE };
    let gap = if glass { GAP } else { 8.0 };
    let n = groups.len();
    let mut out = Vec::new();
    for (g, group) in groups.into_iter().enumerate() {
        let total: f64 = group.iter().map(|i| i.2).sum::<f64>() + gap * group.len().saturating_sub(1) as f64;
        let mut x = if g == 0 {
            side
        } else if g == n - 1 {
            width - side - total
        } else {
            // (Between: centred, as the one middle group is; more spread evenly.)
            let centre = width * g as f64 / (n - 1) as f64;
            centre - total / 2.0
        };
        for (item, content, w) in group {
            let size = content_size(emu, &content);
            let enabled = objc::bag_get(emu, item, "enabled").is_none_or(|e| e & 0xff != 0);
            let prominent = glass && (objc::bag_get(emu, item, "style").unwrap_or(0) == 2 || objc::bag_get(emu, item, "pomme.systemItem") == Some(0));
            let b = if glass {
                let cap = [x, 0.0, w, TOOLBAR_CAPSULE];
                let mut cf = [x + (w - size[0]) / 2.0, (TOOLBAR_CAPSULE - size[1]) / 2.0, size[0], size[1]];
                if matches!(content, Content::Custom(_)) {
                    cf = cap;
                }
                BarItem { item, capsule: Some(cap), area: cap, content, content_frame: cf, prominent, enabled }
            } else {
                let cf = [x, TOOLBAR_CONTENT_MID - size[1] / 2.0, size[0], size[1]];
                BarItem { item, capsule: None, area: [x, 0.0, size[0], TOOLBAR_HEIGHT], content, content_frame: cf, prominent: false, enabled }
            };
            out.push(b);
            x += w + gap;
        }
    }
    Ok(out)
}

/// Whether navigation controller `nc` shows its toolbar: not hidden, and its top screen has items.
fn toolbar_visible(emu: &Emu, nc: u64, top: Option<u64>) -> bool {
    emu.bar_items.toolbar_shown.get(&nc).copied().unwrap_or(false) && top.is_some_and(|t| emu.bar_items.toolbar_items.get(&t).is_some_and(|l| !l.is_empty()))
}

/// The navigation controllers' toolbar views.
pub fn toolbars(emu: &Emu) -> Vec<u64> {
    emu.bar_items.toolbars.values().copied().collect()
}

/// Where a toolbar view is in its window.
pub fn toolbar_origin(emu: &mut Emu, t: u64) -> [f64; 2] {
    let o = crate::uiview::window_origin(emu, t);
    [o[0], o[1]]
}

/// Navigation controller `nc`'s toolbar view, if made.
pub(crate) fn toolbar_of(emu: &Emu, nc: u64) -> Option<u64> {
    emu.bar_items.toolbars.get(&nc).copied()
}

/// Navigation controller `nc`'s toolbar view, made on first use.
fn toolbar_view(emu: &mut Emu, nc: u64) -> Result<u64> {
    if let Some(&t) = emu.bar_items.toolbars.get(&nc) {
        return Ok(t);
    }
    let cls = objc::host_class(emu, "UIToolbar")?;
    let t = send(emu, cls, "alloc", &[])?;
    let t = objc::send_fp_id(emu, t, "initWithFrame:", &[], &[0.0; 4])?;
    emu.bar_items.toolbars.insert(nc, t);
    Ok(t)
}

/// Place a navigation controller's toolbar in its container (`w` × `h`),
/// showing its top screen's items; how much of the content it covers.
pub(crate) fn layout_toolbar(emu: &mut Emu, nc: u64, container: u64, top: Option<u64>, w: f64, h: f64) -> Result<f64> {
    let shown = toolbar_visible(emu, nc, top);
    if !shown && !emu.bar_items.toolbars.contains_key(&nc) {
        return Ok(0.0);
    }
    let t = toolbar_view(emu, nc)?;
    if crate::uiview::view(emu, t).superview != container {
        send(emu, container, "addSubview:", &[t])?;
    }
    send(emu, container, "bringSubviewToFront:", &[t])?;
    crate::uiview::view_mut(emu, t).hidden = !shown;
    let glass = emu.system.liquid_glass();
    let bottom = crate::orientation::safe_area(emu)[2];
    let frame = if glass { [0.0, h - TOOLBAR_GAP_BELOW - TOOLBAR_CAPSULE, w, TOOLBAR_CAPSULE] } else { [0.0, h - bottom - TOOLBAR_HEIGHT, w, TOOLBAR_HEIGHT] };
    if crate::uiview::frame_of(emu, t) != frame {
        crate::uiview::set_frame_of(emu, t, frame);
    }
    Ok(if shown { toolbar_cover(glass, bottom) } else { 0.0 })
}

/// How much of a navigation controller's screens its toolbar covers (above the bottom safe area).
pub(crate) fn toolbar_covers(emu: &mut Emu, nc: u64, top: Option<u64>) -> f64 {
    if !toolbar_visible(emu, nc, top) {
        return 0.0;
    }
    toolbar_cover(emu.system.liquid_glass(), crate::orientation::safe_area(emu)[2])
}

/// The items a toolbar view shows (None: not a navigation controller's toolbar).
pub fn toolbar_look(emu: &mut Emu, t: u64) -> Result<Option<Vec<BarItem>>> {
    let Some(nc) = emu.bar_items.toolbars.iter().find(|(_, v)| **v == t).map(|(k, _)| *k) else { return Ok(None) };
    let top = crate::navigation::stack_of(emu, nc).and_then(|s| s.last().copied());
    let Some(top) = top else { return Ok(None) };
    let items = emu.bar_items.toolbar_items.get(&top).cloned().unwrap_or_default();
    let w = crate::uiview::view(emu, t).bounds[2];
    let glass = emu.system.liquid_glass();
    let mut out = toolbar_layout(emu, &items, w, glass)?;
    if crate::direction::view_rtl(emu, t)? {
        mirror(&mut out, w);
    }
    // (Custom views go into the toolbar, where their items are.)
    place_custom(emu, t, &out, 0.0)?;
    Ok(Some(out))
}

fn toolbar_touches_ended(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    let Some(p) = crate::touch::active_location(emu) else { return Ok(()) };
    let o = crate::uiview::window_origin(emu, t);
    if let Some(items) = toolbar_look(emu, t)? {
        tap(emu, &items, p[0] - o[0], p[1] - o[1])?;
    }
    Ok(())
}

fn relayout_navs(emu: &mut Emu) {
    crate::autolayout::mark_dirty(emu);
    crate::host::frame_changed(emu);
}

// ---- methods ----

fn set_items(emu: &mut Emu, nav_item: u64, trailing: bool, list: Vec<u64>) -> Result<()> {
    for &i in &list {
        send(emu, i, "retain", &[])?;
    }
    emu.bar_items.items.insert((nav_item, trailing), list);
    crate::host::frame_changed(emu);
    Ok(())
}

fn set_one(emu: &mut Emu, trailing: bool) -> Result<()> {
    let (n, i) = (emu.arg(0), emu.arg(2));
    set_items(emu, n, trailing, if i == 0 { vec![] } else { vec![i] })
}

fn set_many(emu: &mut Emu, trailing: bool) -> Result<()> {
    let (n, a) = (emu.arg(0), emu.arg(2));
    let list = if a == 0 { vec![] } else { foundation::array_values(emu, a)? };
    set_items(emu, n, trailing, list)
}

fn get_one(emu: &mut Emu, trailing: bool) -> Result<()> {
    let i = items_of(emu, emu.arg(0), trailing).first().copied().unwrap_or(0);
    emu.ret(i);
    Ok(())
}

fn get_many(emu: &mut Emu, trailing: bool) -> Result<()> {
    let list = items_of(emu, emu.arg(0), trailing);
    let a = if list.is_empty() { 0 } else { foundation::new_array_of(emu, &list)? };
    emu.ret(a);
    Ok(())
}

fn store(emu: &mut Emu, obj: u64, key: &str, v: u64, retain: bool) -> Result<()> {
    if retain && v != 0 {
        send(emu, v, "retain", &[])?;
    }
    objc::bag_set(emu, obj, key, v);
    Ok(())
}

const NI: &str = "UINavigationItem";
const BBI: &str = "UIBarButtonItem";

fn set_toolbar_items(emu: &mut Emu) -> Result<()> {
    let (vc, a) = (emu.arg(0), emu.arg(2));
    let list = if a == 0 { vec![] } else { foundation::array_values(emu, a)? };
    for &i in &list {
        send(emu, i, "retain", &[])?;
    }
    emu.bar_items.toolbar_items.insert(vc, list);
    relayout_navs(emu);
    Ok(())
}

fn set_toolbar_hidden(emu: &mut Emu) -> Result<()> {
    let (nc, hidden) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    emu.bar_items.toolbar_shown.insert(nc, !hidden);
    relayout_navs(emu);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIViewController", false, "setToolbarItems:", set_toolbar_items),
    ("UIViewController", false, "setToolbarItems:animated:", set_toolbar_items),
    ("UIViewController", false, "toolbarItems", |e| {
        let list = e.bar_items.toolbar_items.get(&e.arg(0)).cloned().unwrap_or_default();
        let a = if list.is_empty() { 0 } else { foundation::new_array_of(e, &list)? };
        e.ret(a);
        Ok(())
    }),
    ("UINavigationController", false, "setToolbarHidden:", set_toolbar_hidden),
    ("UINavigationController", false, "setToolbarHidden:animated:", set_toolbar_hidden),
    ("UINavigationController", false, "isToolbarHidden", |e| {
        let hidden = !e.bar_items.toolbar_shown.get(&e.arg(0)).copied().unwrap_or(false);
        e.ret(hidden as u64);
        Ok(())
    }),
    ("UINavigationController", false, "toolbar", |e| {
        let t = toolbar_view(e, e.arg(0))?;
        e.ret(t);
        Ok(())
    }),
    ("UIToolbar", false, "touchesEnded:withEvent:", toolbar_touches_ended),
    (NI, false, "setRightBarButtonItem:", |e| set_one(e, true)),
    (NI, false, "setRightBarButtonItem:animated:", |e| set_one(e, true)),
    (NI, false, "setLeftBarButtonItem:", |e| set_one(e, false)),
    (NI, false, "setLeftBarButtonItem:animated:", |e| set_one(e, false)),
    (NI, false, "setRightBarButtonItems:", |e| set_many(e, true)),
    (NI, false, "setRightBarButtonItems:animated:", |e| set_many(e, true)),
    (NI, false, "setLeftBarButtonItems:", |e| set_many(e, false)),
    (NI, false, "setLeftBarButtonItems:animated:", |e| set_many(e, false)),
    (NI, false, "rightBarButtonItem", |e| get_one(e, true)),
    (NI, false, "leftBarButtonItem", |e| get_one(e, false)),
    (NI, false, "rightBarButtonItems", |e| get_many(e, true)),
    (NI, false, "leftBarButtonItems", |e| get_many(e, false)),
    (BBI, false, "initWithBarButtonSystemItem:target:action:", |e| {
        let (i, kind, target, action) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        objc::bag_set(e, i, "pomme.systemItem", kind);
        objc::bag_set(e, i, "target", target);
        objc::bag_set(e, i, "action", action);
        e.ret(i);
        Ok(())
    }),
    (BBI, false, "initWithTitle:style:target:action:", |e| {
        let (i, title, style, target, action) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
        let t = if title != 0 { send(e, title, "copy", &[])? } else { 0 };
        store(e, i, "title", t, false)?;
        objc::bag_set(e, i, "style", style);
        objc::bag_set(e, i, "target", target);
        objc::bag_set(e, i, "action", action);
        e.ret(i);
        Ok(())
    }),
    (BBI, false, "initWithImage:style:target:action:", |e| {
        let (i, image, style, target, action) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
        store(e, i, "image", image, true)?;
        objc::bag_set(e, i, "style", style);
        objc::bag_set(e, i, "target", target);
        objc::bag_set(e, i, "action", action);
        e.ret(i);
        Ok(())
    }),
    (BBI, false, "initWithCustomView:", |e| {
        let (i, v) = (e.arg(0), e.arg(2));
        store(e, i, "customView", v, true)?;
        e.ret(i);
        Ok(())
    }),
    (BBI, false, "setCustomView:", |e| {
        let (i, v) = (e.arg(0), e.arg(2));
        store(e, i, "customView", v, true)?;
        crate::host::frame_changed(e);
        Ok(())
    }),
    (BBI, false, "setTitle:", |e| {
        let (i, t) = (e.arg(0), e.arg(2));
        let t = if t != 0 { send(e, t, "copy", &[])? } else { 0 };
        store(e, i, "title", t, false)?;
        crate::host::frame_changed(e);
        Ok(())
    }),
    (BBI, false, "setImage:", |e| {
        let (i, img) = (e.arg(0), e.arg(2));
        store(e, i, "image", img, true)?;
        crate::host::frame_changed(e);
        Ok(())
    }),
    (BBI, false, "isEnabled", |e| {
        let on = objc::bag_get(e, e.arg(0), "enabled").is_none_or(|v| v & 0xff != 0);
        e.ret(on as u64);
        Ok(())
    }),
    (BBI, false, "setEnabled:", |e| {
        let (i, on) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, i, "enabled", on);
        crate::host::frame_changed(e);
        Ok(())
    }),
];
