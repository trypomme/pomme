//! Light and dark appearance (the Simulator's Features ▸ Appearance, Settings
//! ▸ Display on a phone): `--appearance dark`, and changes while it runs
//! (`--appearance dark@3`).
//!
//! UIKit's system colours are dynamic: each has a light and a dark value
//! (iOS 26's, recorded on its simulator: tests/fixtures/systemcolors), and so
//! are colours an app makes with `colorWithDynamicProvider:`. pomme keeps
//! every colour's current value where drawing reads it; when the appearance
//! changes, the dynamic ones resolve again, the windows' views and view
//! controllers hear `traitCollectionDidChange:`, and everything is drawn
//! again. A `CGColor` is a snapshot, as on iOS: a layer's colour set from one
//! doesn't follow. (The appearance is the phone's: per-view overrides aren't
//! modelled yet.)

use crate::objc;
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// UIUserInterfaceStyle.
pub const LIGHT: u64 = 1;
pub const DARK: u64 = 2;

#[derive(Clone, Copy, Debug)]
enum Dynamic {
    /// A system colour: its light and dark values.
    Pair([f64; 4], [f64; 4]),
    /// An app's: the block that gives the colour for a trait collection.
    Provider(u64),
}

#[derive(Clone, Default)]
pub struct State {
    pub dark: bool,
    /// Scripted changes (`--appearance dark@3`): (virtual ns, dark).
    pub changes: Vec<(u64, bool)>,
    dynamic: HashMap<u64, Dynamic>,
    /// The system colours, one object each (as UIKit's are).
    system: HashMap<&'static str, u64>,
    /// Fixed snapshots of dynamic colours (`CGColor`), per colour and appearance.
    snapshots: HashMap<(u64, bool), u64>,
}

/// UIUserInterfaceStyle now.
pub fn style(emu: &Emu) -> u64 {
    if emu.appearance.dark { DARK } else { LIGHT }
}

/// iOS 26's system colours, light and dark (sRGB), as UIKit resolves them:
/// recorded on the simulator (the systemcolors fixture).
const PALETTE: &[(&str, [f64; 4], [f64; 4])] = &[
    ("systemBackgroundColor", [1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]),
    ("secondarySystemBackgroundColor", [0.949, 0.949, 0.9686, 1.0], [0.1098, 0.1098, 0.1176, 1.0]),
    ("tertiarySystemBackgroundColor", [1.0, 1.0, 1.0, 1.0], [0.1725, 0.1725, 0.1804, 1.0]),
    ("systemGroupedBackgroundColor", [0.949, 0.949, 0.9686, 1.0], [0.0, 0.0, 0.0, 1.0]),
    ("secondarySystemGroupedBackgroundColor", [1.0, 1.0, 1.0, 1.0], [0.1098, 0.1098, 0.1176, 1.0]),
    ("tertiarySystemGroupedBackgroundColor", [0.949, 0.949, 0.9686, 1.0], [0.1725, 0.1725, 0.1804, 1.0]),
    ("labelColor", [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0, 1.0]),
    ("secondaryLabelColor", [0.2353, 0.2353, 0.2627, 0.6], [0.9216, 0.9216, 0.9608, 0.6]),
    ("tertiaryLabelColor", [0.2353, 0.2353, 0.2627, 0.298], [0.9216, 0.9216, 0.9608, 0.298]),
    ("quaternaryLabelColor", [0.2353, 0.2353, 0.2627, 0.1765], [0.9216, 0.9216, 0.9608, 0.1569]),
    ("placeholderTextColor", [0.2353, 0.2353, 0.2627, 0.298], [0.9216, 0.9216, 0.9608, 0.298]),
    ("separatorColor", [0.2353, 0.2353, 0.2627, 0.12], [0.3294, 0.3294, 0.3451, 0.5]),
    ("opaqueSeparatorColor", [0.7765, 0.7765, 0.7843, 1.0], [0.2196, 0.2196, 0.2275, 1.0]),
    ("linkColor", [0.0, 0.4784, 1.0, 1.0], [0.0353, 0.5176, 1.0, 1.0]),
    ("systemFillColor", [0.4706, 0.4706, 0.502, 0.2], [0.4706, 0.4706, 0.502, 0.36]),
    ("secondarySystemFillColor", [0.4706, 0.4706, 0.502, 0.16], [0.4706, 0.4706, 0.502, 0.32]),
    ("tertiarySystemFillColor", [0.4627, 0.4627, 0.502, 0.12], [0.4627, 0.4627, 0.502, 0.24]),
    ("quaternarySystemFillColor", [0.4549, 0.4549, 0.502, 0.08], [0.4627, 0.4627, 0.502, 0.18]),
    ("systemBlueColor", [0.0, 0.5333, 1.0, 1.0], [0.0, 0.5686, 1.0, 1.0]),
    ("systemRedColor", [1.0, 0.2196, 0.2353, 1.0], [1.0, 0.2588, 0.2706, 1.0]),
    ("systemGreenColor", [0.2039, 0.7804, 0.349, 1.0], [0.1882, 0.8196, 0.3451, 1.0]),
    ("systemYellowColor", [1.0, 0.8, 0.0, 1.0], [1.0, 0.8392, 0.0, 1.0]),
    ("systemOrangeColor", [1.0, 0.5529, 0.1569, 1.0], [1.0, 0.5725, 0.1882, 1.0]),
    ("systemPinkColor", [1.0, 0.1765, 0.3333, 1.0], [1.0, 0.2157, 0.3725, 1.0]),
    ("systemPurpleColor", [0.7961, 0.1882, 0.8784, 1.0], [0.8588, 0.2039, 0.949, 1.0]),
    ("systemIndigoColor", [0.3804, 0.3333, 0.9608, 1.0], [0.4275, 0.4863, 1.0, 1.0]),
    ("systemTealColor", [0.0, 0.7647, 0.8157, 1.0], [0.0, 0.8235, 0.8784, 1.0]),
    ("systemCyanColor", [0.0, 0.7529, 0.9098, 1.0], [0.2353, 0.8275, 0.9961, 1.0]),
    ("systemMintColor", [0.0, 0.7843, 0.702, 1.0], [0.0, 0.8549, 0.7647, 1.0]),
    ("systemBrownColor", [0.6745, 0.498, 0.3686, 1.0], [0.7176, 0.5412, 0.4, 1.0]),
    ("systemGrayColor", [0.5569, 0.5569, 0.5765, 1.0], [0.5569, 0.5569, 0.5765, 1.0]),
    ("systemGray2Color", [0.6824, 0.6824, 0.698, 1.0], [0.3882, 0.3882, 0.4, 1.0]),
    ("systemGray3Color", [0.7804, 0.7804, 0.8, 1.0], [0.2824, 0.2824, 0.2902, 1.0]),
    ("systemGray4Color", [0.8196, 0.8196, 0.8392, 1.0], [0.2275, 0.2275, 0.2353, 1.0]),
    ("systemGray5Color", [0.898, 0.898, 0.9176, 1.0], [0.1725, 0.1725, 0.1804, 1.0]),
    ("systemGray6Color", [0.949, 0.949, 0.9686, 1.0], [0.1098, 0.1098, 0.1176, 1.0]),
];

/// Before iOS 26's design, the system blue (and the tint) was iOS 18's.
const BLUE_18: ([f64; 4], [f64; 4]) = ([0.0, 0.478, 1.0, 1.0], [0.039, 0.518, 1.0, 1.0]);

/// A system colour's (light, dark) values, by its class method's name.
pub fn system_values(emu: &Emu, sel: &str) -> Option<([f64; 4], [f64; 4])> {
    let sel = if sel == "tintColor" { "systemBlueColor" } else { sel };
    if sel == "systemBlueColor" && !emu.system.liquid_glass() {
        return Some(BLUE_18);
    }
    PALETTE.iter().find(|(n, _, _)| *n == sel).map(|&(_, l, d)| (l, d))
}

/// A system colour's value in the current appearance.
pub fn system(emu: &Emu, sel: &str) -> [f64; 4] {
    let (l, d) = system_values(emu, sel).expect("a system colour");
    if emu.appearance.dark { d } else { l }
}

/// The text colour now (`labelColor`): what text is drawn in by default.
pub fn label(emu: &Emu) -> [f64; 4] {
    system(emu, "labelColor")
}

/// `+[UIColor systemBackgroundColor]` and the rest: the one dynamic colour for each.
fn system_color(emu: &mut Emu) -> Result<()> {
    let name = objc::sel_name(emu, emu.arg(1));
    let Some(key) = PALETTE.iter().map(|p| p.0).chain(["tintColor"]).find(|n| *n == name) else {
        return Err(crate::Stop::Unsupported(format!("+[UIColor {name}]")));
    };
    if let Some(&c) = emu.appearance.system.get(key) {
        emu.ret(c);
        return Ok(());
    }
    let (l, d) = system_values(emu, key).unwrap();
    let c = crate::uiview::new_color(emu, if emu.appearance.dark { d } else { l })?;
    emu.appearance.dynamic.insert(c, Dynamic::Pair(l, d));
    emu.appearance.system.insert(key, c);
    emu.ret(c);
    Ok(())
}

/// A colour's value in an appearance.
fn resolve(emu: &mut Emu, c: u64, dark: bool) -> Result<[f64; 4]> {
    match emu.appearance.dynamic.get(&c).copied() {
        Some(Dynamic::Pair(l, d)) => Ok(if dark { d } else { l }),
        Some(Dynamic::Provider(block)) => {
            let traits = traits_with(emu, if dark { DARK } else { LIGHT })?;
            let given = crate::libsystem::call_block(emu, block, &[traits])?;
            if given == c {
                return Ok([0.0; 4]);
            }
            resolve(emu, given, dark)
        }
        None => Ok(emu.ui.views.colors.get(&c).copied().unwrap_or([0.0; 4])),
    }
}

/// `colorWithDynamicProvider:` / `initWithDynamicProvider:`.
fn dynamic_color(emu: &mut Emu) -> Result<()> {
    let block = crate::uiview::copy_block(emu, emu.arg(2))?;
    let c = crate::uiview::new_color(emu, [0.0; 4])?;
    emu.appearance.dynamic.insert(c, Dynamic::Provider(block));
    let dark = emu.appearance.dark;
    let v = resolve(emu, c, dark)?;
    emu.ui.views.colors.insert(c, v);
    emu.ret(c);
    Ok(())
}

/// A fixed colour with `c`'s value in an appearance (`c` itself if it's fixed).
fn snapshot(emu: &mut Emu, c: u64, dark: bool) -> Result<u64> {
    if !emu.appearance.dynamic.contains_key(&c) {
        return Ok(c);
    }
    let v = resolve(emu, c, dark)?;
    if let Some(&s) = emu.appearance.snapshots.get(&(c, dark))
        && emu.ui.views.colors.get(&s) == Some(&v)
    {
        return Ok(s);
    }
    let s = crate::uiview::new_color(emu, v)?;
    emu.appearance.snapshots.insert((c, dark), s);
    Ok(s)
}

/// A trait collection for this appearance (the size classes as they are now).
fn traits_with(emu: &mut Emu, style: u64) -> Result<u64> {
    let t = crate::uiview::current_traits(emu)?;
    objc::bag_set(emu, t, "userInterfaceStyle", style);
    Ok(t)
}

/// Change the appearance: dynamic colours resolve again, the windows' views
/// and controllers hear `traitCollectionDidChange:`, and views draw again.
pub fn set(emu: &mut Emu, dark: bool) -> Result<()> {
    if emu.appearance.dark == dark {
        return Ok(());
    }
    let previous = crate::uiview::current_traits(emu)?;
    emu.appearance.dark = dark;
    let colors: Vec<u64> = emu.appearance.dynamic.keys().copied().collect();
    for c in colors {
        let v = resolve(emu, c, dark)?;
        emu.ui.views.colors.insert(c, v);
    }
    for v in emu.ui.views.views.values_mut() {
        v.needs_display = true;
        v.needs_layout = true;
    }
    crate::orientation::traits_did_change(emu, previous)?;
    crate::host::frame_changed(emu);
    Ok(())
}

/// Put the scripted changes (`--appearance dark@3`) on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.appearance.changes.len() {
        let delay = emu.appearance.changes[i].0.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, |e| {
            let dark = e.appearance.changes[e.arg(1) as usize].1;
            set(e, dark)
        }, i as u64)?;
    }
    Ok(())
}

const COLOR: &str = "UIColor";
const TRAITS: &str = "UITraitCollection";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (COLOR, true, "systemBackgroundColor", system_color),
    (COLOR, true, "secondarySystemBackgroundColor", system_color),
    (COLOR, true, "tertiarySystemBackgroundColor", system_color),
    (COLOR, true, "systemGroupedBackgroundColor", system_color),
    (COLOR, true, "secondarySystemGroupedBackgroundColor", system_color),
    (COLOR, true, "tertiarySystemGroupedBackgroundColor", system_color),
    (COLOR, true, "labelColor", system_color),
    (COLOR, true, "secondaryLabelColor", system_color),
    (COLOR, true, "tertiaryLabelColor", system_color),
    (COLOR, true, "quaternaryLabelColor", system_color),
    (COLOR, true, "placeholderTextColor", system_color),
    (COLOR, true, "separatorColor", system_color),
    (COLOR, true, "opaqueSeparatorColor", system_color),
    (COLOR, true, "linkColor", system_color),
    (COLOR, true, "systemFillColor", system_color),
    (COLOR, true, "secondarySystemFillColor", system_color),
    (COLOR, true, "tertiarySystemFillColor", system_color),
    (COLOR, true, "quaternarySystemFillColor", system_color),
    (COLOR, true, "systemBlueColor", system_color),
    (COLOR, true, "systemRedColor", system_color),
    (COLOR, true, "systemGreenColor", system_color),
    (COLOR, true, "systemYellowColor", system_color),
    (COLOR, true, "systemOrangeColor", system_color),
    (COLOR, true, "systemPinkColor", system_color),
    (COLOR, true, "systemPurpleColor", system_color),
    (COLOR, true, "systemIndigoColor", system_color),
    (COLOR, true, "systemTealColor", system_color),
    (COLOR, true, "systemCyanColor", system_color),
    (COLOR, true, "systemMintColor", system_color),
    (COLOR, true, "systemBrownColor", system_color),
    (COLOR, true, "systemGrayColor", system_color),
    (COLOR, true, "systemGray2Color", system_color),
    (COLOR, true, "systemGray3Color", system_color),
    (COLOR, true, "systemGray4Color", system_color),
    (COLOR, true, "systemGray5Color", system_color),
    (COLOR, true, "systemGray6Color", system_color),
    (COLOR, true, "tintColor", system_color),
    (COLOR, true, "colorWithDynamicProvider:", dynamic_color),
    (COLOR, false, "initWithDynamicProvider:", dynamic_color),
    // A dynamic colour's CGColor is its value now, fixed.
    (COLOR, false, "CGColor", |e| {
        let dark = e.appearance.dark;
        let s = snapshot(e, e.arg(0), dark)?;
        e.ret(s);
        Ok(())
    }),
    (COLOR, false, "resolvedColorWithTraitCollection:", |e| {
        let (c, t) = (e.arg(0), e.arg(2));
        let dark = objc::bag_get(e, t, "userInterfaceStyle").map_or(e.appearance.dark, |s| s == DARK);
        let s = snapshot(e, c, dark)?;
        e.ret(s);
        Ok(())
    }),
    (TRAITS, false, "userInterfaceStyle", |e| {
        let s = objc::bag_get(e, e.arg(0), "userInterfaceStyle").unwrap_or_else(|| style(e));
        e.ret(s);
        Ok(())
    }),
    (TRAITS, true, "traitCollectionWithUserInterfaceStyle:", |e| {
        let t = traits_with(e, e.arg(2))?;
        e.ret(t);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_colour_has_both_values() {
        assert!(PALETTE.len() >= 36);
        for (name, l, d) in PALETTE {
            assert!(name.ends_with("Color") && l.iter().chain(d).all(|v| (0.0..=1.0).contains(v)), "{name}");
        }
    }
}
