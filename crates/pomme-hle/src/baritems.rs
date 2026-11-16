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

/// A tap at `x` (bar coordinates) on one of these items: send its action.
/// Whether it hit one.
pub(crate) fn tap(emu: &mut Emu, items: &[BarItem], x: f64, y: f64) -> Result<bool> {
    let Some(hit) = items.iter().find(|i| x >= i.area[0] - INSET && x < i.area[0] + i.area[2] + INSET && (0.0..CAPSULE).contains(&y)) else { return Ok(false) };
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

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
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
