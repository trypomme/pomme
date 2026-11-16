//! UIAccessibility: what assistive technologies and UI tests see of an app.
//!
//! On iOS every object can describe itself to accessibility through an
//! informal protocol on NSObject: whether it's an element, its label, value,
//! traits and frame on screen, and the elements it contains. UIKit's views
//! and controls answer with defaults (a label's text, a button's title); apps
//! set or override the rest. React Native's `testID` becomes a view's
//! `accessibilityIdentifier`, pomme's SwiftUI sets it from
//! `.accessibilityIdentifier(_:)`, and Flutter builds its own tree of
//! `UIAccessibilityElement`s from its semantics tree, which it turns on as
//! soon as something asks its view for its elements (as XCUITest does).
//!
//! The properties are kept host-side by object, like views' state. Reading
//! goes through messages ([`snapshot`]), so an app's overrides answer for
//! themselves, as they do for VoiceOver.

use crate::foundation;
use crate::objc::{self, send, send_fp};
use crate::uikit::ret_rect;
use crate::uiview::{self, view};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// `NSNotFound`, which `accessibilityElementCount` answers when there's no count.
const NOT_FOUND: u64 = i64::MAX as u64;

/// `UIAccessibilityTraits` bits UIKit's own views carry.
pub const TRAIT_BUTTON: u64 = 1;
pub const TRAIT_LINK: u64 = 1 << 1;
pub const TRAIT_IMAGE: u64 = 1 << 2;
pub const TRAIT_SELECTED: u64 = 1 << 3;
pub const TRAIT_STATIC_TEXT: u64 = 1 << 6;
pub const TRAIT_NOT_ENABLED: u64 = 1 << 8;
pub const TRAIT_SEARCH_FIELD: u64 = 1 << 10;
pub const TRAIT_ADJUSTABLE: u64 = 1 << 12;
pub const TRAIT_TAB_BAR: u64 = 1 << 15;
pub const TRAIT_HEADER: u64 = 1 << 16;
/// UIKit's private trait for text you can edit (`UITextField`'s traits carry
/// it on iOS, and VoiceOver says "text field" for it).
pub const TRAIT_TEXT_ENTRY: u64 = 1 << 18;
/// UIKit's private trait for a tab bar's buttons (VoiceOver says "tab").
pub const TRAIT_TAB: u64 = 1 << 28;

/// `UIAccessibilityContainerTypeList`.
const CONTAINER_LIST: u64 = 2;

/// What an object has been told about itself (nothing told: UIKit's defaults).
#[derive(Debug, Default, Clone)]
pub struct Props {
    /// Strings and arrays are retained copies.
    pub identifier: u64,
    pub label: u64,
    pub value: u64,
    pub hint: u64,
    pub elements: u64,
    pub traits: Option<u64>,
    pub is_element: Option<bool>,
    pub elements_hidden: bool,
    pub modal: bool,
    /// Screen coordinates (`accessibilityFrame`).
    pub frame: Option<[f64; 4]>,
    /// A UIAccessibilityElement's frame in its container (iOS 10+), which follows the container.
    pub frame_in_container: Option<[f64; 4]>,
    /// A UIAccessibilityElement's container (not retained, as UIKit's is weak).
    pub container: u64,
    /// Where activating it touches, in screen coordinates (default: its frame's center).
    pub activation_point: Option<[f64; 2]>,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub props: HashMap<u64, Props>,
    /// Elements of what pomme draws itself (a navigation bar's back button and
    /// title, a tab bar's buttons) by (container, part), kept so they stay the
    /// same objects while they're shown.
    drawn: HashMap<(u64, u32), u64>,
}

/// An element of something pomme draws: its part (a key within its container), label, value, traits and frame there.
struct Part {
    key: u32,
    label: String,
    value: Option<String>,
    traits: u64,
    frame: [f64; 4],
}

fn props(emu: &Emu, o: u64) -> Props {
    emu.ax.props.get(&o).cloned().unwrap_or_default()
}

/// For a setter: what assistive technology sees may change.
fn props_mut(emu: &mut Emu, o: u64) -> &mut Props {
    crate::host::accessibility_changed(emu);
    emu.ax.props.entry(o).or_default()
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn is_kind(emu: &mut Emu, o: u64, class: &str) -> Result<bool> {
    let cls = objc::host_class(emu, class)?;
    objc::is_kind_of(emu, o, cls)
}

/// Store a retained copy of the object argument (a string or array) in a slot.
fn set_object(emu: &mut Emu, slot: fn(&mut Props) -> &mut u64) -> Result<()> {
    let (o, v) = (emu.arg(0), emu.arg(2));
    let copy = if v == 0 { 0 } else { send(emu, v, "copy", &[])? };
    let old = std::mem::replace(slot(props_mut(emu, o)), copy);
    if old != 0 {
        objc::objc_release_obj(emu, old)?;
    }
    Ok(())
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

// ---- defaults ----

/// Whether UIKit makes this an element by default: its standard controls and text.
fn element_by_default(emu: &mut Emu, o: u64) -> Result<bool> {
    for class in ["UILabel", "UIButton", "UITextField", "UITextView", "UISwitch", "UISlider", "UISegmentedControl"] {
        if is_kind(emu, o, class)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A string from a message, if the object answers one.
fn string_message(emu: &mut Emu, o: u64, sel: &str) -> Result<u64> {
    let s = objc::intern(emu, sel)?;
    if !objc::responds(emu, o, s)? {
        return Ok(0);
    }
    send(emu, o, sel, &[])
}

fn default_label(emu: &mut Emu, o: u64) -> Result<u64> {
    if is_kind(emu, o, "UITabBar")? {
        return foundation::new_nsstring(emu, "Tab Bar");
    }
    if is_kind(emu, o, "UILabel")? {
        return string_message(emu, o, "text");
    }
    if is_kind(emu, o, "UIButton")? {
        return string_message(emu, o, "currentTitle");
    }
    if is_kind(emu, o, "UITextField")? {
        return string_message(emu, o, "placeholder");
    }
    Ok(0)
}

fn default_value(emu: &mut Emu, o: u64) -> Result<u64> {
    if is_kind(emu, o, "UITextField")? || is_kind(emu, o, "UITextView")? {
        return string_message(emu, o, "text");
    }
    if is_kind(emu, o, "UISwitch")? {
        let on = send(emu, o, "isOn", &[])? & 0xff != 0;
        return foundation::new_nsstring(emu, if on { "1" } else { "0" });
    }
    if is_kind(emu, o, "UISlider")? {
        let (v, lo, hi) = crate::valuecontrols::slider_values(emu, o);
        let (v, lo, hi) = (v as f64, lo as f64, hi as f64);
        let pct = if hi > lo { ((v - lo) / (hi - lo) * 100.0).round() } else { 0.0 };
        return foundation::new_nsstring(emu, &format!("{pct}%"));
    }
    Ok(0)
}

fn default_traits(emu: &mut Emu, o: u64) -> Result<u64> {
    let mut t = if is_kind(emu, o, "UIButton")? {
        TRAIT_BUTTON
    } else if is_kind(emu, o, "UILabel")? {
        TRAIT_STATIC_TEXT
    } else if is_kind(emu, o, "UISlider")? {
        TRAIT_ADJUSTABLE
    } else if is_kind(emu, o, "UIImageView")? {
        TRAIT_IMAGE
    } else {
        0
    };
    if is_kind(emu, o, "UITextField")? || (is_kind(emu, o, "UITextView")? && send(emu, o, "isEditable", &[])? & 0xff != 0) {
        t |= TRAIT_TEXT_ENTRY;
    }
    if is_kind(emu, o, "UIControl")? {
        if send(emu, o, "isEnabled", &[])? & 0xff == 0 {
            t |= TRAIT_NOT_ENABLED;
        }
        if send(emu, o, "isSelected", &[])? & 0xff != 0 {
            t |= TRAIT_SELECTED;
        }
    }
    Ok(t)
}

/// A rect in a view's coordinates, in screen coordinates (the window is the screen).
pub fn to_screen(emu: &mut Emu, v: u64, r: [f64; 4]) -> [f64; 4] {
    if v == 0 {
        return r;
    }
    let o = uiview::window_origin(emu, v);
    let b = view(emu, v).bounds;
    [r[0] + o[0] - b[0], r[1] + o[1] - b[1], r[2], r[3]]
}

fn is_view(emu: &mut Emu, o: u64) -> bool {
    emu.ui.views.views.contains_key(&o)
}

// ---- NSObject (UIAccessibility) ----

fn is_accessibility_element(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let v = match props(emu, o).is_element {
        Some(v) => v,
        None => element_by_default(emu, o)?,
    };
    ret(emu, v as u64)
}

fn set_is_accessibility_element(emu: &mut Emu) -> Result<()> {
    let (o, v) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    props_mut(emu, o).is_element = Some(v);
    Ok(())
}

fn label(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let l = match props(emu, o).label {
        0 => default_label(emu, o)?,
        l => l,
    };
    ret(emu, l)
}

fn value(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let v = match props(emu, o).value {
        0 => default_value(emu, o)?,
        v => v,
    };
    ret(emu, v)
}

fn traits(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let t = match props(emu, o).traits {
        Some(t) => t,
        None => default_traits(emu, o)?,
    };
    ret(emu, t)
}

fn set_traits(emu: &mut Emu) -> Result<()> {
    let (o, t) = (emu.arg(0), emu.arg(2));
    props_mut(emu, o).traits = Some(t);
    Ok(())
}

/// Where it is on screen: what it was told, a UIAccessibilityElement's frame
/// in its container, or a view's bounds.
fn frame(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let p = props(emu, o);
    let r = if let Some(r) = p.frame {
        r
    } else if let Some(r) = p.frame_in_container {
        let c = p.container;
        if c != 0 && is_view(emu, c) { to_screen(emu, c, r) } else { r }
    } else if is_view(emu, o) {
        let b = view(emu, o).bounds;
        to_screen(emu, o, b)
    } else {
        [0.0; 4]
    };
    ret_rect(emu, r);
    Ok(())
}

fn set_frame(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    props_mut(emu, o).frame = Some(r);
    Ok(())
}

fn elements_hidden(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let h = props(emu, o).elements_hidden;
    ret(emu, h as u64)
}

fn set_elements_hidden(emu: &mut Emu) -> Result<()> {
    let (o, h) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    props_mut(emu, o).elements_hidden = h;
    Ok(())
}

fn view_is_modal(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let m = props(emu, o).modal;
    ret(emu, m as u64)
}

fn set_view_is_modal(emu: &mut Emu) -> Result<()> {
    let (o, m) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    props_mut(emu, o).modal = m;
    Ok(())
}

/// The UIAccessibilityContainer protocol's count: the elements it was given, if any.
fn element_count(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let n = match send(emu, o, "accessibilityElements", &[])? {
        0 => 0,
        a => send(emu, a, "count", &[])?,
    };
    ret(emu, n)
}

fn element_at_index(emu: &mut Emu) -> Result<()> {
    let (o, i) = (emu.arg(0), emu.arg(2));
    let a = send(emu, o, "accessibilityElements", &[])?;
    let n = if a == 0 { 0 } else { send(emu, a, "count", &[])? };
    let e = if i < n { send(emu, a, "objectAtIndex:", &[i])? } else { 0 };
    ret(emu, e)
}

fn index_of_element(emu: &mut Emu) -> Result<()> {
    let (o, e) = (emu.arg(0), emu.arg(2));
    let a = send(emu, o, "accessibilityElements", &[])?;
    let i = if a == 0 { NOT_FOUND } else { send(emu, a, "indexOfObject:", &[e])? };
    ret(emu, i)
}

fn activation_point(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let p = match props(emu, o).activation_point {
        Some(p) => p,
        None => {
            let [x, y, w, h] = send_fp(emu, o, "accessibilityFrame", &[], &[])?;
            [x + w / 2.0, y + h / 2.0]
        }
    };
    emu.cpu.q[0] = p[0].to_bits() as u128;
    emu.cpu.q[1] = p[1].to_bits() as u128;
    Ok(())
}

fn set_activation_point(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let p = [d(emu, 0), d(emu, 1)];
    props_mut(emu, o).activation_point = Some(p);
    Ok(())
}

// ---- UIAccessibilityElement ----

fn element_init_with_container(emu: &mut Emu) -> Result<()> {
    let (o, c) = (emu.arg(0), emu.arg(2));
    props_mut(emu, o).container = c;
    ret(emu, o)
}

fn container(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let c = props(emu, o).container;
    ret(emu, c)
}

fn set_container(emu: &mut Emu) -> Result<()> {
    let (o, c) = (emu.arg(0), emu.arg(2));
    props_mut(emu, o).container = c;
    Ok(())
}

fn frame_in_container(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let r = props(emu, o).frame_in_container.unwrap_or_default();
    ret_rect(emu, r);
    Ok(())
}

fn set_frame_in_container(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    props_mut(emu, o).frame_in_container = Some(r);
    Ok(())
}

const O: &str = "NSObject";
const E: &str = "UIAccessibilityElement";

macro_rules! object_prop {
    ($class:expr, $get:literal, $set:literal, $field:ident) => {
        [
            ($class, false, $get, (|e: &mut Emu| {
                let v = props(e, e.arg(0)).$field;
                ret(e, v)
            }) as HostFn),
            ($class, false, $set, (|e: &mut Emu| set_object(e, |p| &mut p.$field)) as HostFn),
        ]
    };
}

const STRINGS: &[[(&str, bool, &str, HostFn); 2]] = &[
    // An identifier is UIAccessibilityIdentification's: views, elements, images and bar items.
    object_prop!("UIView", "accessibilityIdentifier", "setAccessibilityIdentifier:", identifier),
    object_prop!(E, "accessibilityIdentifier", "setAccessibilityIdentifier:", identifier),
    object_prop!("UIImage", "accessibilityIdentifier", "setAccessibilityIdentifier:", identifier),
    object_prop!("UIBarItem", "accessibilityIdentifier", "setAccessibilityIdentifier:", identifier),
    object_prop!(O, "accessibilityHint", "setAccessibilityHint:", hint),
    object_prop!(O, "accessibilityElements", "setAccessibilityElements:", elements),
    [(O, false, "accessibilityLabel", label), (O, false, "setAccessibilityLabel:", |e| set_object(e, |p| &mut p.label))],
    [(O, false, "accessibilityValue", value), (O, false, "setAccessibilityValue:", |e| set_object(e, |p| &mut p.value))],
];

/// The rest of the protocol: kept as given and read back, for apps and
/// assistive technology; pomme doesn't act on them (yet).
macro_rules! kept {
    (object $get:literal, $set:literal) => {
        [
            (O, false, $get, (|e: &mut Emu| {
                let v = objc::bag_get(e, e.arg(0), $get).unwrap_or(0);
                ret(e, v)
            }) as HostFn),
            (O, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                let copy = if v == 0 { 0 } else { send(e, v, "copy", &[])? };
                if let Some(old) = objc::bag_get(e, o, $get).filter(|&old| old != 0) {
                    objc::objc_release_obj(e, old)?;
                }
                objc::bag_set(e, o, $get, copy);
                Ok(())
            }) as HostFn),
        ]
    };
    (scalar $get:literal, $set:literal, $default:expr) => {
        [
            (O, false, $get, (|e: &mut Emu| {
                let v = objc::bag_get(e, e.arg(0), $get).unwrap_or($default);
                ret(e, v)
            }) as HostFn),
            (O, false, $set, (|e: &mut Emu| {
                let (o, v) = (e.arg(0), e.arg(2));
                objc::bag_set(e, o, $get, v);
                Ok(())
            }) as HostFn),
        ]
    };
}

const KEPT: &[[(&str, bool, &str, HostFn); 2]] = &[
    kept!(object "accessibilityLanguage", "setAccessibilityLanguage:"),
    kept!(object "accessibilityAttributedLabel", "setAccessibilityAttributedLabel:"),
    kept!(object "accessibilityAttributedValue", "setAccessibilityAttributedValue:"),
    kept!(object "accessibilityAttributedHint", "setAccessibilityAttributedHint:"),
    kept!(object "accessibilityUserInputLabels", "setAccessibilityUserInputLabels:"),
    kept!(object "accessibilityCustomActions", "setAccessibilityCustomActions:"),
    kept!(object "accessibilityCustomRotors", "setAccessibilityCustomRotors:"),
    kept!(object "accessibilityPath", "setAccessibilityPath:"),
    kept!(object "accessibilityTextualContext", "setAccessibilityTextualContext:"),
    kept!(object "accessibilityHeaderElements", "setAccessibilityHeaderElements:"),
    kept!(scalar "accessibilityNavigationStyle", "setAccessibilityNavigationStyle:", 0),
    kept!(scalar "accessibilityContainerType", "setAccessibilityContainerType:", 0),
    kept!(scalar "shouldGroupAccessibilityChildren", "setShouldGroupAccessibilityChildren:", 0),
    kept!(scalar "accessibilityIgnoresInvertColors", "setAccessibilityIgnoresInvertColors:", 0),
    kept!(scalar "accessibilityRespondsToUserInteraction", "setAccessibilityRespondsToUserInteraction:", 0),
];

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut m: Vec<_> = STRINGS.iter().chain(KEPT).flatten().copied().collect();
    m.extend_from_slice(METHODS);
    m
}

const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (O, false, "isAccessibilityElement", is_accessibility_element),
    (O, false, "setIsAccessibilityElement:", set_is_accessibility_element),
    (O, false, "accessibilityTraits", traits),
    (O, false, "setAccessibilityTraits:", set_traits),
    (O, false, "accessibilityFrame", frame),
    (O, false, "setAccessibilityFrame:", set_frame),
    (O, false, "accessibilityElementsHidden", elements_hidden),
    (O, false, "setAccessibilityElementsHidden:", set_elements_hidden),
    (O, false, "accessibilityViewIsModal", view_is_modal),
    (O, false, "setAccessibilityViewIsModal:", set_view_is_modal),
    (O, false, "accessibilityActivationPoint", activation_point),
    (O, false, "setAccessibilityActivationPoint:", set_activation_point),
    // What VoiceOver asks of an element: NSObject's answers do nothing (an
    // activation that isn't handled becomes a tap at the activation point).
    (O, false, "accessibilityActivate", |e| ret(e, 0)),
    (O, false, "accessibilityIncrement", |_| Ok(())),
    (O, false, "accessibilityDecrement", |_| Ok(())),
    (O, false, "accessibilityElementDidBecomeFocused", |_| Ok(())),
    (O, false, "accessibilityElementDidLoseFocus", |_| Ok(())),
    (O, false, "accessibilityElementCount", element_count),
    (O, false, "accessibilityElementAtIndex:", element_at_index),
    (O, false, "indexOfAccessibilityElement:", index_of_element),
    ("UINavigationBar", false, "accessibilityElementCount", bar_element_count),
    ("UINavigationBar", false, "accessibilityElementAtIndex:", bar_element_at_index),
    ("UINavigationBar", false, "indexOfAccessibilityElement:", bar_index_of_element),
    ("UITabBar", false, "accessibilityElementCount", bar_element_count),
    ("UITabBar", false, "accessibilityElementAtIndex:", bar_element_at_index),
    ("UITabBar", false, "indexOfAccessibilityElement:", bar_index_of_element),
    ("UIMoreListView", false, "accessibilityElementCount", bar_element_count),
    ("UIMoreListView", false, "accessibilityElementAtIndex:", bar_element_at_index),
    ("UIMoreListView", false, "indexOfAccessibilityElement:", bar_index_of_element),
    (E, false, "initWithAccessibilityContainer:", element_init_with_container),
    (E, false, "accessibilityContainer", container),
    (E, false, "setAccessibilityContainer:", set_container),
    (E, false, "accessibilityFrameInContainerSpace", frame_in_container),
    (E, false, "setAccessibilityFrameInContainerSpace:", set_frame_in_container),
];

/// `UIAccessibilityConvertFrameToScreenCoordinates(rect, view)`: rect (d0–d3) in the view's coordinates.
fn convert_frame_to_screen(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let r = [d(emu, 0), d(emu, 1), d(emu, 2), d(emu, 3)];
    let r = if is_view(emu, v) { to_screen(emu, v, r) } else { r };
    ret_rect(emu, r);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[("_UIAccessibilityConvertFrameToScreenCoordinates", convert_frame_to_screen)];

// ---- UINavigationBar ----

/// What a navigation bar shows accessibility, as UIKit's bar views do: its
/// back button and its title (a header). pomme draws the bar itself, so these
/// are UIAccessibilityElements in the bar, at what's drawn.
fn bar_elements(emu: &mut Emu, bar: u64) -> Result<Vec<u64>> {
    let Some(look) = crate::navigation::bar_look(emu, bar)? else { return Ok(Vec::new()) };
    let [_, _, w, h] = view(emu, bar).bounds;
    let mut parts = Vec::new();
    if let Some(b) = &look.back {
        // The chevron alone reads as "Back", as on iOS.
        let label = b.label.clone().unwrap_or_else(|| "Back".into());
        parts.push(Part { key: 0, label, value: None, traits: TRAIT_BUTTON, frame: [b.hit.0, 0.0, b.hit.1, look.row] });
    }
    if !look.title.is_empty() {
        let frame = if look.large {
            let x = look.leading + 16.0;
            [x, look.row, (w - x - 16.0).max(0.0), (h - look.row).max(0.0)]
        } else {
            let tw = crate::navigation::text_width(emu, &look.title, crate::navigation::TITLE_WEIGHT).min(w);
            [(w - tw) / 2.0, 0.0, tw, look.row]
        };
        parts.push(Part { key: 1, label: look.title.clone(), value: None, traits: TRAIT_HEADER, frame });
    }
    // Its button items, each a button by its title or name (a custom view is its own).
    for (i, item) in look.items.iter().enumerate() {
        let Some(label) = crate::baritems::accessibility_label(emu, item)? else { continue };
        let traits = if item.enabled { TRAIT_BUTTON } else { TRAIT_BUTTON | TRAIT_NOT_ENABLED };
        parts.push(Part { key: 10 + i as u32, label, value: None, traits, frame: item.area });
    }
    drawn_elements(emu, bar, parts)
}

/// The UIAccessibilityElements for parts of something pomme draws in
/// `container`, updated to say what they show now.
fn drawn_elements(emu: &mut Emu, container: u64, parts: Vec<Part>) -> Result<Vec<u64>> {
    let mut out = Vec::new();
    for part in parts {
        let el = match emu.ax.drawn.get(&(container, part.key)) {
            Some(&el) => el,
            None => {
                let cls = objc::host_class(emu, E)?;
                let el = objc::create_instance(emu, cls, 0)?;
                emu.ax.drawn.insert((container, part.key), el);
                el
            }
        };
        set_string(emu, el, |p| &mut p.label, Some(&part.label))?;
        set_string(emu, el, |p| &mut p.value, part.value.as_deref())?;
        let p = props(emu, el);
        if p.traits != Some(part.traits) || p.container != container || p.frame_in_container != Some(part.frame) || p.is_element != Some(true) {
            let p = props_mut(emu, el);
            p.traits = Some(part.traits);
            p.container = container;
            p.frame_in_container = Some(part.frame);
            p.is_element = Some(true);
        }
        out.push(el);
    }
    Ok(out)
}

/// Set a string property of a drawn element, if it changed.
fn set_string(emu: &mut Emu, el: u64, slot: fn(&mut Props) -> &mut u64, text: Option<&str>) -> Result<()> {
    let old = *slot(&mut props(emu, el));
    let same = match text {
        None => old == 0,
        Some(t) => old != 0 && foundation::nsstring(emu, old).is_ok_and(|s| s == t),
    };
    if same {
        return Ok(());
    }
    let new = match text {
        Some(t) => foundation::new_nsstring(emu, t)?,
        None => 0,
    };
    if old != 0 {
        objc::objc_release_obj(emu, old)?;
    }
    *slot(props_mut(emu, el)) = new;
    Ok(())
}

/// What a tab bar shows accessibility, as UIKit's tab buttons say it: each
/// tab a button (and a tab), selected or not, by its title; before iOS 26 a
/// badge reads as its value ("3 items"), as measured.
fn tab_bar_elements(emu: &mut Emu, bar: u64) -> Result<Vec<u64>> {
    let glass = emu.system.liquid_glass();
    let mut parts = Vec::new();
    for (i, b) in crate::tabbar::buttons(emu, bar)?.into_iter().enumerate() {
        let mut traits = TRAIT_BUTTON | TRAIT_TAB;
        if b.selected {
            traits |= TRAIT_SELECTED;
        }
        if !b.enabled {
            traits |= TRAIT_NOT_ENABLED;
        }
        let value = if glass {
            None
        } else {
            b.badge.map(|badge| match badge.text.parse::<i64>() {
                Ok(1) => "1 item".to_string(),
                Ok(n) => format!("{n} items"),
                Err(_) => badge.text,
            })
        };
        parts.push(Part { key: i as u32, label: b.title, value, traits, frame: b.frame });
    }
    drawn_elements(emu, bar, parts)
}

/// The bar's own elements, unless the app gave it some.
fn bar_children(emu: &mut Emu, bar: u64) -> Result<Option<Vec<u64>>> {
    if props(emu, bar).elements != 0 {
        return Ok(None);
    }
    if emu.nav.bars.contains_key(&bar) {
        return Ok(Some(bar_elements(emu, bar)?));
    }
    if crate::tabbar::is_bar(emu, bar) {
        return Ok(Some(tab_bar_elements(emu, bar)?));
    }
    if let Some(list) = crate::tabbar::more_list(emu, bar)? {
        // The More list's rows: a button each, by its tab's title, its badge the value.
        let parts = list.rows.into_iter().enumerate().map(|(i, r)| Part { key: i as u32, label: r.title, value: r.badge, traits: TRAIT_BUTTON, frame: r.frame }).collect();
        return Ok(Some(drawn_elements(emu, bar, parts)?));
    }
    Ok(None)
}

fn bar_element_count(emu: &mut Emu) -> Result<()> {
    let bar = emu.arg(0);
    let n = match bar_children(emu, bar)? {
        Some(els) => els.len() as u64,
        None => return element_count(emu),
    };
    ret(emu, n)
}

fn bar_element_at_index(emu: &mut Emu) -> Result<()> {
    let (bar, i) = (emu.arg(0), emu.arg(2));
    let e = match bar_children(emu, bar)? {
        Some(els) => els.get(i as usize).copied().unwrap_or(0),
        None => return element_at_index(emu),
    };
    ret(emu, e)
}

fn bar_index_of_element(emu: &mut Emu) -> Result<()> {
    let (bar, e) = (emu.arg(0), emu.arg(2));
    let i = match bar_children(emu, bar)? {
        Some(els) => els.iter().position(|&x| x == e).map_or(NOT_FOUND, |i| i as u64),
        None => return index_of_element(emu),
    };
    ret(emu, i)
}

// ---- Reading the tree ----

/// Something on screen as accessibility sees it: a view, or an element a
/// container provides (Flutter's semantics nodes, a paragraph's links).
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// How deep in the tree (windows are 0).
    pub depth: usize,
    /// The object (a view or a UIAccessibilityElement).
    pub object: u64,
    /// The view it's in: itself, for a view.
    pub view: u64,
    pub class: String,
    pub identifier: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub hint: Option<String>,
    pub traits: u64,
    /// Whether it's an accessibility element itself (VoiceOver stops on it).
    pub is_element: bool,
    /// Whether it's modal (`accessibilityViewIsModal`): VoiceOver ignores its siblings.
    pub modal: bool,
    /// `accessibilityContainerType`: a container VoiceOver announces (a group, a list…), or 0.
    pub container_type: u64,
    /// Whether the elements in it are in the order it gave them
    /// (`accessibilityElements`, a container's own count); otherwise they're its subviews.
    pub ordered: bool,
    /// Where it is on screen, in points: x, y, width, height.
    pub frame: [f64; 4],
    /// Where VoiceOver taps to activate it, in screen points (its frame's center by default).
    pub activation_point: [f64; 2],
}

impl Element {
    pub fn center(&self) -> (f64, f64) {
        (self.frame[0] + self.frame[2] / 2.0, self.frame[1] + self.frame[3] / 2.0)
    }

    /// Whether any of it is on a screen of this size.
    pub fn on_screen(&self, (w, h): (f64, f64)) -> bool {
        let [x, y, fw, fh] = self.frame;
        fw > 0.0 && fh > 0.0 && x < w && y < h && x + fw > 0.0 && y + fh > 0.0
    }

    /// Whether it says anything about itself (an identifier, a label or a value).
    pub fn is_described(&self) -> bool {
        self.identifier.is_some() || self.label.is_some() || self.value.is_some()
    }
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [x, y, w, h] = self.frame;
        write!(f, "{}", self.class)?;
        if let Some(id) = &self.identifier {
            write!(f, " #{id}")?;
        }
        if let Some(l) = &self.label {
            write!(f, " {l:?}")?;
        }
        if let Some(v) = &self.value {
            write!(f, " value {v:?}")?;
        }
        if self.traits & TRAIT_BUTTON != 0 {
            write!(f, " button")?;
        }
        write!(f, " ({x:.1}, {y:.1}, {w:.1}×{h:.1})")
    }
}

fn string_of(emu: &mut Emu, o: u64, sel: &str) -> Result<Option<String>> {
    let s = string_message(emu, o, sel)?;
    if s == 0 {
        return Ok(None);
    }
    let s = if is_kind(emu, s, "NSAttributedString")? { send(emu, s, "string", &[])? } else { s };
    Ok(foundation::nsstring(emu, s).ok().filter(|s| !s.is_empty()))
}

/// The objects accessibility finds inside `o`: the elements it provides (in
/// its order: true), or a view's subviews.
fn children(emu: &mut Emu, o: u64) -> Result<(Vec<u64>, bool)> {
    let elements = send(emu, o, "accessibilityElements", &[])?;
    if elements != 0 {
        let n = send(emu, elements, "count", &[])?;
        return Ok(((0..n).map(|i| send(emu, elements, "objectAtIndex:", &[i])).collect::<Result<_>>()?, true));
    }
    // A container that counts its own elements (UIAccessibilityContainer).
    let n = send(emu, o, "accessibilityElementCount", &[])?;
    if n != 0 && n != NOT_FOUND {
        return Ok(((0..n.min(10_000)).map(|i| send(emu, o, "accessibilityElementAtIndex:", &[i])).collect::<Result<_>>()?, true));
    }
    Ok((if is_view(emu, o) { view(emu, o).subviews } else { Vec::new() }, false))
}

fn walk(emu: &mut Emu, o: u64, in_view: u64, depth: usize, out: &mut Vec<Element>) -> Result<()> {
    if o == 0 || depth > 200 {
        return Ok(());
    }
    let own_view = is_view(emu, o);
    if own_view {
        let vw = view(emu, o);
        if vw.hidden || vw.alpha < 0.01 {
            return Ok(());
        }
    }
    if send(emu, o, "accessibilityElementsHidden", &[])? & 0xff != 0 {
        return Ok(());
    }
    let r = send_fp(emu, o, "accessibilityFrame", &[], &[])?;
    let isa = emu.read_u64(o)?;
    let class = emu.objc.class_name(isa);
    let element = Element {
        depth,
        object: o,
        view: if own_view { o } else { in_view },
        class,
        identifier: string_of(emu, o, "accessibilityIdentifier")?,
        label: string_of(emu, o, "accessibilityLabel")?,
        value: string_of(emu, o, "accessibilityValue")?,
        hint: string_of(emu, o, "accessibilityHint")?,
        traits: send(emu, o, "accessibilityTraits", &[])?,
        is_element: send(emu, o, "isAccessibilityElement", &[])? & 0xff != 0,
        modal: send(emu, o, "accessibilityViewIsModal", &[])? & 0xff != 0,
        container_type: send(emu, o, "accessibilityContainerType", &[])?,
        ordered: false,
        frame: r,
        activation_point: {
            let p = send_fp(emu, o, "accessibilityActivationPoint", &[], &[])?;
            [p[0], p[1]]
        },
    };
    let in_view = element.view;
    let at = out.len();
    out.push(element);
    let (children, ordered) = children(emu, o)?;
    out[at].ordered = ordered;
    for c in children {
        if c != o {
            walk(emu, c, in_view, depth + 1, out)?;
        }
    }
    Ok(())
}

/// Everything accessibility can see on screen, window by window, in tree
/// order (later is in front). Hidden views and hidden elements are left out.
pub fn snapshot(emu: &mut Emu) -> Result<Vec<Element>> {
    let mut out = Vec::new();
    for w in emu.ui.windows.clone() {
        walk(emu, w, w, 0, &mut out)?;
    }
    Ok(out)
}

/// The snapshot as indented text: the elements that describe themselves,
/// each under the nearest of its ancestors that does.
pub fn describe(elements: &[Element]) -> String {
    let mut s = String::new();
    // The path to the current element (pre-order): each ancestor's depth, and whether it's shown.
    let mut path: Vec<(usize, bool)> = Vec::new();
    for e in elements {
        while path.last().is_some_and(|&(d, _)| d >= e.depth) {
            path.pop();
        }
        let shown = e.is_described() || e.depth == 0;
        if shown {
            let indent = path.iter().filter(|&&(_, shown)| shown).count();
            s += &format!("{}{e}\n", "  ".repeat(indent));
        }
        path.push((e.depth, shown));
    }
    s
}

/// Whether a touch at the element's center would land on it: the window's
/// hit test finds its view or something inside it (XCUITest's `isHittable`).
pub fn hittable(emu: &mut Emu, e: &Element) -> Result<bool> {
    let window = if emu.ui.key_window != 0 { emu.ui.key_window } else { emu.ui.windows.first().copied().unwrap_or(0) };
    if window == 0 {
        return Ok(false);
    }
    let (x, y) = e.center();
    let mut hit = objc::send_fp_id(emu, window, "hitTest:withEvent:", &[0], &[x, y])?;
    while hit != 0 && hit != e.view {
        hit = view(emu, hit).superview;
    }
    Ok(hit != 0)
}

// ---- For the host's assistive technology ----

/// What kind of thing a [`Node`] is, from its traits (and class, for a switch):
/// what a screen reader announces it as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    /// A window: the root of what's on screen in it.
    Window,
    /// A container VoiceOver announces on the way in (`accessibilityContainerType`:
    /// a semantic group, a landmark, a data table), by its label.
    Group,
    /// A list (`UIAccessibilityContainerTypeList`).
    List,
    Button,
    Link,
    /// A `UISwitch`: its value is "1" when on.
    Switch,
    /// Adjustable: VoiceOver swipes it up and down (a slider, a stepper, a picker).
    Adjustable,
    TextField,
    SearchField,
    Heading,
    Image,
    StaticText,
    /// A tab bar (`UIAccessibilityTraitTabBar`, or UIKit's UITabBar): it holds tabs.
    TabBar,
    /// A tab in a tab bar: selected or not.
    Tab,
    /// An element with no more specific trait.
    Element,
}

impl Role {
    fn of(e: &Element, switch: bool, tab_bar: bool) -> Role {
        let t = e.traits;
        if !e.is_element {
            if e.depth == 0 {
                Role::Window
            } else if t & TRAIT_TAB_BAR != 0 || tab_bar {
                Role::TabBar
            } else if e.container_type == CONTAINER_LIST {
                Role::List
            } else {
                Role::Group
            }
        } else if t & TRAIT_TAB != 0 {
            Role::Tab
        } else if t & TRAIT_SEARCH_FIELD != 0 {
            Role::SearchField
        } else if t & TRAIT_TEXT_ENTRY != 0 {
            Role::TextField
        } else if switch {
            Role::Switch
        } else if t & TRAIT_ADJUSTABLE != 0 {
            Role::Adjustable
        } else if t & TRAIT_LINK != 0 {
            Role::Link
        } else if t & TRAIT_BUTTON != 0 {
            Role::Button
        } else if t & TRAIT_HEADER != 0 {
            Role::Heading
        } else if t & TRAIT_IMAGE != 0 {
            Role::Image
        } else if t & TRAIT_STATIC_TEXT != 0 {
            Role::StaticText
        } else if t & TRAIT_TAB_BAR != 0 {
            Role::TabBar
        } else {
            Role::Element
        }
    }
}

/// Something a host's screen reader can see: a window, a group, or an
/// accessibility element. Hosts get them as a list in reading order (a
/// parent before its children, as VoiceOver moves through them).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// The object it is (stable while it's on screen): actions name it.
    pub id: u64,
    /// The node it's in (none for a window).
    pub parent: Option<u64>,
    pub role: Role,
    pub label: Option<String>,
    pub value: Option<String>,
    pub hint: Option<String>,
    pub identifier: Option<String>,
    pub traits: u64,
    /// In screen points: x, y, width, height.
    pub frame: [f64; 4],
    /// Where a tap activates it, in screen points.
    pub activation_point: [f64; 2],
}

impl Node {
    /// An accessibility element: a screen reader stops on it (windows and groups only hold them).
    pub fn is_element(&self) -> bool {
        !matches!(self.role, Role::Window | Role::Group | Role::List | Role::TabBar)
    }

    pub fn enabled(&self) -> bool {
        self.traits & TRAIT_NOT_ENABLED == 0
    }

    pub fn selected(&self) -> bool {
        self.traits & TRAIT_SELECTED != 0
    }

    /// A switch that's on.
    pub fn on(&self) -> bool {
        self.role == Role::Switch && self.value.as_deref() == Some("1")
    }
}

/// What a screen reader can do to an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Double-tap: `accessibilityActivate`, or else a tap at its activation point.
    Activate,
    /// Swipe up on an adjustable element.
    Increment,
    /// Swipe down on an adjustable element.
    Decrement,
    /// The screen reader's cursor arrived on it.
    Focus,
}

/// The screen as VoiceOver would present it, for a host's screen reader:
/// windows, the accessibility elements in them, and the containers it
/// announces (those with an `accessibilityContainerType`, and tab bars).
/// VoiceOver doesn't look inside an element, nor at what's beside a modal
/// view; other containers are left out (their elements belong to the nearest
/// one that's kept).
pub fn tree(emu: &mut Emu) -> Result<Vec<Node>> {
    let elements = snapshot(emu)?;
    let ends: Vec<usize> = (0..elements.len()).map(|i| subtree_end(&elements, i)).collect();
    // Each element's window, and the front-most modal view in each window.
    let mut window_of = Vec::with_capacity(elements.len());
    let mut modal: HashMap<usize, usize> = HashMap::new();
    for (i, e) in elements.iter().enumerate() {
        let w = if e.depth == 0 { i } else { window_of.last().copied().unwrap_or(0) };
        window_of.push(w);
        if e.modal && e.depth > 0 {
            modal.insert(w, i);
        }
    }
    // Each element's parent in the snapshot.
    let mut parent_of = vec![None; elements.len()];
    for i in 0..elements.len() {
        let mut j = i + 1;
        while j < ends[i] {
            parent_of[j] = Some(i);
            j = ends[j];
        }
    }
    // In what the modal view's siblings hold (VoiceOver ignores the modal view's siblings).
    let beside_modal = |i: usize| {
        modal.get(&window_of[i]).is_some_and(|&m| {
            let Some(p) = parent_of[m] else { return false };
            p < i && i < ends[p] && !(m <= i && i < ends[m])
        })
    };
    let mut out: Vec<Node> = Vec::new();
    // Where each node goes among its parent's: see the sort below.
    let mut keys: Vec<(f64, f64, usize)> = Vec::new();
    // The kept ancestors of the current element: (depth, id).
    let mut path: Vec<(usize, u64)> = Vec::new();
    // The containers above it that keep their own order: (depth, index).
    let mut ordered: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < elements.len() {
        let e = &elements[i];
        while path.last().is_some_and(|&(d, _)| d >= e.depth) {
            path.pop();
        }
        while ordered.last().is_some_and(|&(d, _)| d >= e.depth) {
            ordered.pop();
        }
        if beside_modal(i) {
            i = ends[i]; // not this, nor anything in it
            continue;
        }
        // (UIKit's UITabBar doesn't carry the tab bar trait; VoiceOver knows it as one.)
        let tab_bar = !e.is_element && is_kind(emu, e.object, "UITabBar")?;
        let keep = e.depth == 0 || e.is_element || e.container_type != 0 || e.traits & TRAIT_TAB_BAR != 0 || tab_bar;
        if keep {
            let switch = e.is_element && is_kind(emu, e.object, "UISwitch")?;
            out.push(Node {
                id: e.object,
                parent: path.last().map(|&(_, id)| id),
                role: Role::of(e, switch, tab_bar),
                label: e.label.clone(),
                value: e.value.clone(),
                hint: e.hint.clone(),
                identifier: e.identifier.clone(),
                traits: e.traits,
                frame: e.frame,
                activation_point: e.activation_point,
            });
            // UIKit reads the elements in a container top to bottom, then left
            // to right, wherever they are among its subviews, unless something
            // between gives its elements in its own order: they stay together
            // in that order, where that container is.
            let parent_depth = path.last().map(|&(d, _)| d);
            let key = match ordered.iter().find(|&&(d, _)| parent_depth.is_some_and(|p| d >= p)) {
                Some(&(d, _)) if Some(d) == parent_depth => (0.0, 0.0, i),
                Some(&(_, c)) => (elements[c].frame[1], elements[c].frame[0], i),
                None => (e.frame[1], e.frame[0], i),
            };
            keys.push(key);
            if e.is_element {
                i = ends[i]; // VoiceOver doesn't look inside an element
                continue;
            }
            path.push((e.depth, e.object));
        }
        if e.ordered {
            ordered.push((e.depth, i));
        }
        i += 1;
    }
    let mut out = in_reading_order(out, keys);
    // Groups with no elements in them say nothing a screen reader can visit.
    loop {
        let has_child: std::collections::HashSet<u64> = out.iter().filter_map(|n| n.parent).collect();
        let before = out.len();
        out.retain(|n| n.role == Role::Window || n.is_element() || has_child.contains(&n.id));
        if out.len() == before {
            break;
        }
    }
    Ok(out)
}

/// The nodes with each one's children sorted by their keys, parents before children.
fn in_reading_order(nodes: Vec<Node>, keys: Vec<(f64, f64, usize)>) -> Vec<Node> {
    let mut children: HashMap<Option<u64>, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        children.entry(n.parent).or_default().push(i);
    }
    for kids in children.values_mut() {
        kids.sort_by(|&a, &b| {
            let (ka, kb) = (keys[a], keys[b]);
            ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(ka.2.cmp(&kb.2))
        });
    }
    let mut order = Vec::with_capacity(nodes.len());
    let mut stack: Vec<usize> = children.get(&None).map(|k| k.iter().rev().copied().collect()).unwrap_or_default();
    // Windows stay in their order (back to front).
    stack.sort_by_key(|&i| std::cmp::Reverse(keys[i].2));
    while let Some(i) = stack.pop() {
        order.push(i);
        if let Some(kids) = children.get(&Some(nodes[i].id)) {
            stack.extend(kids.iter().rev());
        }
    }
    let mut slots: Vec<Option<Node>> = nodes.into_iter().map(Some).collect();
    order.into_iter().filter_map(|i| slots[i].take()).collect()
}

/// The index just past `i`'s subtree in a pre-order snapshot.
fn subtree_end(elements: &[Element], i: usize) -> usize {
    let d = elements[i].depth;
    elements[i + 1..].iter().position(|e| e.depth <= d).map_or(elements.len(), |p| i + 1 + p)
}

/// Do what a screen reader asked of an element on the screen it was shown
/// (`shown`: the last tree the host got; anything else is gone, and ignored).
pub fn perform(emu: &mut Emu, shown: &[Node], id: u64, action: Action) -> Result<()> {
    let Some(node) = shown.iter().find(|n| n.id == id) else { return Ok(()) };
    match action {
        Action::Activate => {
            // As VoiceOver: the element's own activation first, else a tap.
            if send(emu, id, "accessibilityActivate", &[])? & 0xff == 0 {
                let [x, y] = node.activation_point;
                crate::touch::send_touch(emu, crate::touch::BEGAN, x, y)?;
                let info = ((x as f32).to_bits() as u64) << 32 | (y as f32).to_bits() as u64;
                crate::uikit::after_on_main(emu, TAP_HOLD_S, lift, info)?;
            }
        }
        Action::Increment => {
            send(emu, id, "accessibilityIncrement", &[])?;
        }
        Action::Decrement => {
            send(emu, id, "accessibilityDecrement", &[])?;
        }
        Action::Focus => {
            send(emu, id, "accessibilityElementDidBecomeFocused", &[])?;
        }
    }
    Ok(())
}

/// How long VoiceOver's tap holds the finger down.
const TAP_HOLD_S: f64 = 0.05;

/// Timer callout `(timer, info)`: the activation's finger lifts (info: the point, two f32s).
fn lift(emu: &mut Emu) -> Result<()> {
    let info = emu.arg(1);
    let (x, y) = (f32::from_bits((info >> 32) as u32) as f64, f32::from_bits(info as u32) as f64);
    crate::touch::send_touch(emu, crate::touch::ENDED, x, y)
}
