//! iOS 26's Liquid Glass views, and the corners they're shaped with.
//!
//! Corners: `UICornerRadius` (fixed, or concentric with the container) and
//! `UICornerConfiguration` (a radius for each corner, or a capsule), set on
//! any view as `cornerConfiguration`; Swift apps reach them through the
//! Swift half of UIKit pomme builds (scripts/sysroot/overlays/UIKit). A view
//! with one is drawn and clipped with the resolved radii (continuous curves).
//!
//! Glass: a `UIVisualEffectView` (its `contentView` holds what's on it) with
//! a `UIGlassEffect` (regular or clear, tinted, interactive), or a
//! `UIGlassContainerEffect` whose glass children draw themselves; what
//! Expo's expo-glass-effect and Callstack's liquid-glass give React Native
//! apps. render.rs draws the glass as measured on the iOS 26 Simulator
//! (tests/fixtures/glass): the backdrop blurred, mixed toward white and made
//! more saturated, a tint over it, and a soft shadow below.

use crate::objc::{self, bag_get, bag_set, send};
use crate::{Emu, HostFn, Result, foundation};

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_f64(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

/// A new, autoreleased instance of `cls` (a class object, as a class method's receiver).
fn new_of(emu: &mut Emu, cls: u64) -> Result<u64> {
    let o = objc::create_instance(emu, cls, 0)?;
    send(emu, o, "autorelease", &[])
}

fn new_named(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = objc::host_class(emu, class)?;
    new_of(emu, cls)
}

/// Keep an object in another's bag, retained (the one before released).
fn keep(emu: &mut Emu, o: u64, key: &'static str, v: u64) -> Result<()> {
    if v != 0 {
        send(emu, v, "retain", &[])?;
    }
    if let Some(old) = bag_get(emu, o, key).filter(|&old| old != 0) {
        objc::objc_release_obj(emu, old)?;
    }
    bag_set(emu, o, key, v);
    crate::host::changed(emu);
    Ok(())
}

// ---- Corners ----

const RADIUS: &str = "UICornerRadius";
const CONFIG: &str = "UICornerConfiguration";
/// No value (a concentric radius without a minimum, a capsule without a maximum).
const NONE: u64 = u64::MAX;

/// A radius: fixed (`pommeFixed`), or concentric with the container, at least `pommeMinimum`.
fn new_radius(emu: &mut Emu, cls: u64, fixed: Option<f64>, minimum: Option<f64>) -> Result<u64> {
    let r = new_of(emu, cls)?;
    bag_set(emu, r, "pommeFixed", fixed.map_or(NONE, f64::to_bits));
    bag_set(emu, r, "pommeMinimum", minimum.map_or(NONE, f64::to_bits));
    Ok(r)
}

/// A radius as given: Some(fixed), or None and its minimum (concentric).
fn radius_value(emu: &Emu, r: u64) -> (Option<f64>, f64) {
    let get = |k| bag_get(emu, r, k).filter(|&v| v != NONE).map(f64::from_bits);
    (get("pommeFixed"), get("pommeMinimum").unwrap_or(0.0))
}

/// A configuration of four corners (top-left, top-right, bottom-left, bottom-right; nil: square).
fn new_config(emu: &mut Emu, cls: u64, corners: [u64; 4]) -> Result<u64> {
    let c = new_of(emu, cls)?;
    for (key, r) in ["pommeTL", "pommeTR", "pommeBL", "pommeBR"].into_iter().zip(corners) {
        keep(emu, c, key, r)?;
    }
    Ok(c)
}

fn new_capsule(emu: &mut Emu, cls: u64, maximum: Option<f64>) -> Result<u64> {
    let c = new_of(emu, cls)?;
    bag_set(emu, c, "pommeCapsule", maximum.map_or(NONE, f64::to_bits));
    Ok(c)
}

/// A view's corner radii from its configuration (top-left, top-right,
/// bottom-left, bottom-right), at its size `w` × `h`; None without one.
pub fn corner_radii(emu: &mut Emu, v: u64, w: f64, h: f64) -> Option<[f64; 4]> {
    let c = bag_get(emu, v, "pommeCornerConfiguration").filter(|&c| c != 0)?;
    Some(resolve(emu, c, v, w, h))
}

/// Whether a view's corners are a capsule's: round ends (circular arcs), where
/// other corners are continuous curves.
pub fn is_capsule(emu: &Emu, v: u64) -> bool {
    bag_get(emu, v, "pommeCornerConfiguration").filter(|&c| c != 0).is_some_and(|c| bag_get(emu, c, "pommeCapsule").is_some())
}

fn resolve(emu: &mut Emu, c: u64, v: u64, w: f64, h: f64) -> [f64; 4] {
    if let Some(max) = bag_get(emu, c, "pommeCapsule") {
        // A capsule: half the shorter side (no more than its maximum).
        let r = w.min(h) / 2.0;
        let r = if max == NONE { r } else { r.min(f64::from_bits(max)) };
        return [r; 4];
    }
    let mut out = [0.0; 4];
    for (i, key) in ["pommeTL", "pommeTR", "pommeBL", "pommeBR"].into_iter().enumerate() {
        let r = bag_get(emu, c, key).unwrap_or(0);
        if r == 0 {
            continue;
        }
        out[i] = match radius_value(emu, r) {
            (Some(fixed), _) => fixed,
            // Concentric: the container's corner, less how far in this view sits; at least the minimum.
            (None, minimum) => concentric(emu, v, i).max(minimum),
        };
    }
    out
}

/// The radius concentric with the superview's corner `i`: its radius less
/// the view's inset from that corner (the nearer of the two edges).
fn concentric(emu: &mut Emu, v: u64, i: usize) -> f64 {
    let vw = crate::uiview::view(emu, v);
    let parent = vw.superview;
    if parent == 0 {
        return 0.0;
    }
    let pv = crate::uiview::view(emu, parent);
    let (pw, ph) = (pv.bounds[2], pv.bounds[3]);
    let outer = match corner_radii(emu, parent, pw, ph) {
        Some(r) => r[i],
        None => {
            let ly = if pv.layer != 0 { crate::calayer::layer(emu, pv.layer) } else { Default::default() };
            ly.corner_radius
        }
    };
    let f = crate::uiview::frame_of(emu, v);
    let inset = match i {
        0 => f[0].min(f[1]),
        1 => (pw - f[0] - f[2]).min(f[1]),
        2 => f[0].min(ph - f[1] - f[3]),
        _ => (pw - f[0] - f[2]).min(ph - f[1] - f[3]),
    };
    (outer - inset.max(0.0)).max(0.0)
}

fn corner_methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    vec![
        (RADIUS, true, "fixedRadius:", |e| {
            let (cls, r) = (e.arg(0), d(e, 0));
            let o = new_radius(e, cls, Some(r), None)?;
            ret(e, o)
        }),
        (RADIUS, true, "containerConcentricRadius", |e| {
            let cls = e.arg(0);
            let o = new_radius(e, cls, None, None)?;
            ret(e, o)
        }),
        (RADIUS, true, "containerConcentricRadiusWithMinimum:", |e| {
            let (cls, m) = (e.arg(0), d(e, 0));
            let o = new_radius(e, cls, None, Some(m))?;
            ret(e, o)
        }),
        (RADIUS, false, "copyWithZone:", |e| {
            let o = send(e, e.arg(0), "retain", &[])?;
            ret(e, o)
        }),
        (RADIUS, false, "isEqual:", |e| {
            let (a, b) = (e.arg(0), e.arg(2));
            let same = b != 0 && radius_value(e, a) == radius_value(e, b);
            ret(e, same as u64)
        }),
        (RADIUS, false, "hash", |e| {
            let (f, m) = radius_value(e, e.arg(0));
            ret(e, f.unwrap_or(-1.0).to_bits() ^ m.to_bits().rotate_left(17))
        }),
        (RADIUS, false, "description", |e| {
            let s = match radius_value(e, e.arg(0)) {
                (Some(f), _) => format!("<UICornerRadius fixed {f}>"),
                (None, m) => format!("<UICornerRadius concentric, minimum {m}>"),
            };
            let s = foundation::new_nsstring(e, &s)?;
            ret(e, s)
        }),
        (CONFIG, true, "configurationWithRadius:", |e| {
            let (cls, r) = (e.arg(0), e.arg(2));
            let o = new_config(e, cls, [r; 4])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformRadius:", |e| {
            let (cls, r) = (e.arg(0), e.arg(2));
            let o = new_config(e, cls, [r; 4])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithTopLeftRadius:topRightRadius:bottomLeftRadius:bottomRightRadius:", |e| {
            let (cls, tl, tr, bl, br) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
            let o = new_config(e, cls, [tl, tr, bl, br])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformTopRadius:uniformBottomRadius:", |e| {
            let (cls, t, b) = (e.arg(0), e.arg(2), e.arg(3));
            let o = new_config(e, cls, [t, t, b, b])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformLeftRadius:uniformRightRadius:", |e| {
            let (cls, l, r) = (e.arg(0), e.arg(2), e.arg(3));
            let o = new_config(e, cls, [l, r, l, r])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformTopRadius:bottomLeftRadius:bottomRightRadius:", |e| {
            let (cls, t, bl, br) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
            let o = new_config(e, cls, [t, t, bl, br])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformBottomRadius:topLeftRadius:topRightRadius:", |e| {
            let (cls, b, tl, tr) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
            let o = new_config(e, cls, [tl, tr, b, b])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformLeftRadius:topRightRadius:bottomRightRadius:", |e| {
            let (cls, l, tr, br) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
            let o = new_config(e, cls, [l, tr, l, br])?;
            ret(e, o)
        }),
        (CONFIG, true, "configurationWithUniformRightRadius:topLeftRadius:bottomLeftRadius:", |e| {
            let (cls, r, tl, bl) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
            let o = new_config(e, cls, [tl, r, bl, r])?;
            ret(e, o)
        }),
        (CONFIG, true, "capsuleConfiguration", |e| {
            let cls = e.arg(0);
            let o = new_capsule(e, cls, None)?;
            ret(e, o)
        }),
        (CONFIG, true, "capsuleConfigurationWithMaximumRadius:", |e| {
            let (cls, m) = (e.arg(0), d(e, 0));
            let o = new_capsule(e, cls, Some(m))?;
            ret(e, o)
        }),
        (CONFIG, false, "copyWithZone:", |e| {
            let o = send(e, e.arg(0), "retain", &[])?;
            ret(e, o)
        }),
        (CONFIG, false, "isEqual:", |e| {
            let (a, b) = (e.arg(0), e.arg(2));
            let same = b != 0 && config_key(e, a) == config_key(e, b);
            ret(e, same as u64)
        }),
        (CONFIG, false, "hash", |e| {
            let k = config_key(e, e.arg(0));
            ret(e, k.iter().fold(0u64, |h, &x| h.rotate_left(7) ^ x))
        }),
        (CONFIG, false, "description", |e| {
            let k = config_key(e, e.arg(0));
            let s = foundation::new_nsstring(e, &format!("<UICornerConfiguration {k:x?}>"))?;
            ret(e, s)
        }),
        ("UIView", false, "cornerConfiguration", |e| {
            let v = e.arg(0);
            let c = match bag_get(e, v, "pommeCornerConfiguration").filter(|&c| c != 0) {
                Some(c) => c,
                // (Square corners until one's set.)
                None => {
                    let cls = objc::host_class(e, CONFIG)?;
                    new_config(e, cls, [0; 4])?
                }
            };
            ret(e, c)
        }),
        ("UIView", false, "setCornerConfiguration:", |e| {
            let (v, c) = (e.arg(0), e.arg(2));
            keep(e, v, "pommeCornerConfiguration", c)
        }),
        ("UIView", false, "effectiveRadiusForCorner:", |e| {
            let (v, corners) = (e.arg(0), e.arg(2));
            let b = crate::uiview::view(e, v).bounds;
            let radii = corner_radii(e, v, b[2], b[3]).unwrap_or_else(|| {
                let ly = crate::uiview::view(e, v).layer;
                [if ly != 0 { crate::calayer::layer(e, ly).corner_radius } else { 0.0 }; 4]
            });
            // UIRectCorner: top-left 1, top-right 2, bottom-left 4, bottom-right 8; the largest asked for.
            let r = (0..4).filter(|i| corners & (1 << i) != 0).map(|i| radii[i]).fold(0.0, f64::max);
            ret_f64(e, r)
        }),
    ]
}

/// What a configuration says, to compare: per corner its radius (fixed bits, minimum bits), or the capsule.
fn config_key(emu: &Emu, c: u64) -> Vec<u64> {
    if let Some(m) = bag_get(emu, c, "pommeCapsule") {
        return vec![1, m];
    }
    let mut k = vec![0];
    for key in ["pommeTL", "pommeTR", "pommeBL", "pommeBR"] {
        let r = bag_get(emu, c, key).unwrap_or(0);
        let (f, m) = if r == 0 { (Some(0.0), 0.0) } else { radius_value(emu, r) };
        k.push(f.map_or(NONE, f64::to_bits));
        k.push(m.to_bits());
    }
    k
}

// ---- Glass ----

const EFFECT_VIEW: &str = "UIVisualEffectView";
const CONTENT_VIEW: &str = "_UIVisualEffectContentView";
const GLASS: &str = "UIGlassEffect";
const CONTAINER: &str = "UIGlassContainerEffect";

/// UIGlassEffectStyle.
pub const REGULAR: u64 = 0;
pub const CLEAR: u64 = 1;
/// pomme's own materials for UIAlertController (alert.rs; tests/fixtures/alerts,
/// fitted over teal, white, black and orange): the box's glass, an action's
/// grey fill on it, and the preferred action's tinted fill.
pub const ALERT: u64 = 100;
pub const ALERT_FILL: u64 = 101;
pub const ALERT_TINTED_FILL: u64 = 102;

/// A glass view as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glass {
    /// Regular or clear.
    pub style: u64,
    pub tint: Option<[f64; 4]>,
    pub interactive: bool,
    /// In the dark appearance (its own, a superview's, or the system's).
    pub dark: bool,
}

/// Whether `v` is in the dark appearance: the nearest view up its chain
/// (itself, a superview, its window) that overrides the style says, else
/// the system's.
pub fn is_dark(emu: &Emu, mut v: u64) -> bool {
    for _ in 0..256 {
        if v == 0 {
            break;
        }
        match bag_get(emu, v, "pommeUserInterfaceStyle") {
            Some(1) => return false,
            Some(2) => return true,
            _ => {}
        }
        v = emu.ui.views.views.get(&v).map_or(0, |vw| vw.superview);
    }
    emu.appearance.dark
}

/// The glass `v` shows, if it's a visual effect view with a glass effect.
pub fn glass(emu: &mut Emu, v: u64) -> Option<Glass> {
    let effect = bag_get(emu, v, "pommeEffect").filter(|&e| e != 0)?;
    let cls = objc::host_class(emu, GLASS).ok()?;
    if !objc::is_kind_of(emu, effect, cls).ok()? {
        return None;
    }
    let tint = bag_get(emu, effect, "pommeTint").filter(|&c| c != 0).and_then(|c| emu.ui.views.colors.get(&c).copied());
    Some(Glass { style: bag_get(emu, effect, "pommeStyle").unwrap_or(REGULAR), tint, interactive: bag_get(emu, effect, "pommeInteractive").unwrap_or(0) != 0, dark: is_dark(emu, v) })
}

/// The content view: made with the effect view, filling it, resizing with it.
fn ensure_content(emu: &mut Emu, v: u64) -> Result<u64> {
    if let Some(c) = bag_get(emu, v, "pommeContentView").filter(|&c| c != 0) {
        return Ok(c);
    }
    let c = new_named(emu, CONTENT_VIEW)?;
    let b = crate::uiview::view(emu, v).bounds;
    emu.cpu.q[0] = 0;
    emu.cpu.q[1] = 0;
    emu.cpu.q[2] = b[2].to_bits() as u128;
    emu.cpu.q[3] = b[3].to_bits() as u128;
    let c = send(emu, c, "initWithFrame:", &[])?;
    // Flexible width and height.
    send(emu, c, "setAutoresizingMask:", &[2 | 16])?;
    bag_set(emu, v, "pommeContentView", c);
    send(emu, v, "addSubview:", &[c])?;
    Ok(c)
}

fn set_effect(emu: &mut Emu, v: u64, effect: u64) -> Result<()> {
    let copy = if effect == 0 { 0 } else { send(emu, effect, "copy", &[])? };
    if let Some(old) = bag_get(emu, v, "pommeEffect").filter(|&o| o != 0) {
        objc::objc_release_obj(emu, old)?;
    }
    bag_set(emu, v, "pommeEffect", copy);
    crate::host::changed(emu);
    Ok(())
}

/// An effect's `copy`: one of its class with what it holds.
fn copy_effect(emu: &mut Emu) -> Result<()> {
    let from = emu.arg(0);
    let cls = emu.read_u64(from)?;
    let to = objc::create_instance(emu, cls, 0)?;
    for key in ["pommeStyle", "pommeInteractive", "pommeSpacing"] {
        if let Some(v) = bag_get(emu, from, key) {
            bag_set(emu, to, key, v);
        }
    }
    if let Some(t) = bag_get(emu, from, "pommeTint").filter(|&t| t != 0) {
        send(emu, t, "retain", &[])?;
        bag_set(emu, to, "pommeTint", t);
    }
    ret(emu, to)
}

fn glass_methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    vec![
        (EFFECT_VIEW, false, "initWithFrame:", |e| {
            let v = e.arg(0);
            crate::uiview::view_init_with_frame(e)?;
            ensure_content(e, v)?;
            ret(e, v)
        }),
        (EFFECT_VIEW, false, "initWithEffect:", |e| {
            let (v, effect) = (e.arg(0), e.arg(2));
            e.cpu.q[..4].fill(0);
            let v = send(e, v, "initWithFrame:", &[])?;
            set_effect(e, v, effect)?;
            ret(e, v)
        }),
        (EFFECT_VIEW, false, "contentView", |e| {
            let c = ensure_content(e, e.arg(0))?;
            ret(e, c)
        }),
        (EFFECT_VIEW, false, "effect", |e| {
            let v = bag_get(e, e.arg(0), "pommeEffect").unwrap_or(0);
            ret(e, v)
        }),
        (EFFECT_VIEW, false, "setEffect:", |e| {
            let (v, effect) = (e.arg(0), e.arg(2));
            set_effect(e, v, effect)
        }),
        (GLASS, true, "effectWithStyle:", |e| {
            let (cls, style) = (e.arg(0), e.arg(2));
            let g = new_of(e, cls)?;
            bag_set(e, g, "pommeStyle", style);
            ret(e, g)
        }),
        (GLASS, false, "isInteractive", |e| {
            let v = bag_get(e, e.arg(0), "pommeInteractive").unwrap_or(0);
            ret(e, v)
        }),
        (GLASS, false, "setInteractive:", |e| {
            let (g, v) = (e.arg(0), e.arg(2) & 0xff);
            bag_set(e, g, "pommeInteractive", v);
            Ok(())
        }),
        (GLASS, false, "tintColor", |e| {
            let v = bag_get(e, e.arg(0), "pommeTint").unwrap_or(0);
            ret(e, v)
        }),
        (GLASS, false, "setTintColor:", |e| {
            let (g, c) = (e.arg(0), e.arg(2));
            keep(e, g, "pommeTint", c)
        }),
        (GLASS, false, "copyWithZone:", copy_effect),
        (GLASS, false, "copy", copy_effect),
        (CONTAINER, false, "spacing", |e| {
            let s = bag_get(e, e.arg(0), "pommeSpacing").map_or(0.0, f64::from_bits);
            ret_f64(e, s)
        }),
        (CONTAINER, false, "setSpacing:", |e| {
            let (g, s) = (e.arg(0), d(e, 0));
            bag_set(e, g, "pommeSpacing", s.to_bits());
            Ok(())
        }),
        (CONTAINER, false, "copyWithZone:", copy_effect),
        (CONTAINER, false, "copy", copy_effect),
        // Where a view is in a tab bar's bottom accessory (UITabAccessoryEnvironment; 0: unspecified).
        ("UITraitCollection", false, "tabAccessoryEnvironment", |e| {
            let v = bag_get(e, e.arg(0), "tabAccessoryEnvironment").unwrap_or(0);
            ret(e, v)
        }),
        // A view's own appearance (UIUserInterfaceStyle: 0 unspecified, 1 light, 2 dark).
        ("UIView", false, "overrideUserInterfaceStyle", |e| {
            let v = bag_get(e, e.arg(0), "pommeUserInterfaceStyle").unwrap_or(0);
            ret(e, v)
        }),
        ("UIView", false, "setOverrideUserInterfaceStyle:", |e| {
            let (v, style) = (e.arg(0), e.arg(2));
            bag_set(e, v, "pommeUserInterfaceStyle", style);
            crate::host::changed(e);
            Ok(())
        }),
        // A plain UIVisualEffect (apps set one to clear the glass) copies as itself.
        ("UIVisualEffect", false, "copyWithZone:", copy_effect),
        ("UIVisualEffect", false, "copy", copy_effect),
    ]
}

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut m = corner_methods();
    m.extend(glass_methods());
    m
}
