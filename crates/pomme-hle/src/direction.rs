//! The layout direction: right to left in an app whose chosen localization
//! is written right to left (Arabic, Hebrew, …), as on iOS; an app with no
//! such localization stays left to right on a phone in one of those
//! languages (fixtures/rtlplain). A view's own direction comes from its
//! `semanticContentAttribute`: unspecified follows the app, playback,
//! spatial and forced left-to-right are left to right, forced right-to-left
//! is right to left. Its trait collection always has the app's direction.
//!
//! Auto Layout resolves a constraint's leading and trailing edges in the
//! direction of the view the two items share (the constraint's view): views
//! inside a view forced left to right lay out left to right, even though
//! their own direction is still the app's (fixtures/rtl, `forcedLTR`).

use crate::foundation::{self, nsstring};
use crate::objc;
use crate::{Emu, HostFn, Result};

#[derive(Clone, Default)]
pub struct State {
    /// Whether the app is right to left, once worked out.
    app_rtl: Option<bool>,
}

/// UIUserInterfaceLayoutDirection / UITraitEnvironmentLayoutDirection.
const LTR: u64 = 0;
const RTL: u64 = 1;

/// Whether `language` is written right to left (CoreFoundation's answer).
pub(crate) fn language_rtl(emu: &mut Emu, language: &str) -> Result<bool> {
    let s = foundation::new_nsstring(emu, language)?;
    // kCFLocaleLanguageDirectionRightToLeft
    Ok(foundation::cf_call(emu, "_CFLocaleGetLanguageCharacterDirection", &[s])? == 2)
}

/// Whether the app lays out right to left: its chosen localization's direction.
pub fn app_rtl(emu: &mut Emu) -> Result<bool> {
    if let Some(r) = emu.direction.app_rtl {
        return Ok(r);
    }
    let r = match crate::bundle::main_preferred_localization(emu)? {
        Some(l) => language_rtl(emu, &l)?,
        None => false,
    };
    emu.direction.app_rtl = Some(r);
    Ok(r)
}

/// The direction a semantic content attribute gives.
fn for_semantic(emu: &mut Emu, attribute: u64) -> Result<bool> {
    Ok(match attribute {
        0 => app_rtl(emu)?,
        4 => true,
        _ => false,
    })
}

/// Whether view `v` is right to left (`effectiveUserInterfaceLayoutDirection`).
pub fn view_rtl(emu: &mut Emu, v: u64) -> Result<bool> {
    let attribute = objc::bag_get(emu, v, "semanticContentAttribute").unwrap_or(0);
    for_semantic(emu, attribute)
}

/// Whether a constraint between `a` and `b` (views; 0 for none) lays out
/// right to left: the direction of the nearest view both are in.
pub(crate) fn constraint_rtl(emu: &mut Emu, a: u64, b: u64) -> Result<bool> {
    let ancestors = |emu: &mut Emu, mut v: u64| {
        let mut out = Vec::new();
        while v != 0 && emu.ui.views.views.contains_key(&v) {
            out.push(v);
            v = crate::uiview::view(emu, v).superview;
        }
        out
    };
    let (la, lb) = (ancestors(emu, a), ancestors(emu, b));
    let shared = if b == 0 {
        la.get(1).or(la.first()).copied()
    } else if a == 0 {
        lb.get(1).or(lb.first()).copied()
    } else {
        la.iter().find(|v| lb.contains(v)).copied()
    };
    match shared {
        Some(v) => view_rtl(emu, v),
        None => app_rtl(emu),
    }
}

/// A text alignment as drawn in view `v`: natural (4) and justified (3) are
/// left in a left-to-right view and right in a right-to-left one.
pub(crate) fn resolve_alignment(emu: &mut Emu, v: u64, alignment: u64) -> Result<u64> {
    Ok(match alignment {
        3 | 4 => {
            if view_rtl(emu, v)? {
                2
            } else {
                0
            }
        }
        a => a,
    })
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn dir(rtl: bool) -> u64 {
    if rtl { RTL } else { LTR }
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIApplication", false, "userInterfaceLayoutDirection", |e| {
        let r = app_rtl(e)?;
        ret(e, dir(r))
    }),
    ("UITraitCollection", false, "layoutDirection", |e| {
        let r = app_rtl(e)?;
        ret(e, dir(r))
    }),
    ("UIView", false, "setSemanticContentAttribute:", |e| {
        let (v, a) = (e.arg(0), e.arg(2));
        objc::bag_set(e, v, "semanticContentAttribute", a);
        crate::autolayout::mark_dirty(e);
        crate::host::frame_changed(e);
        Ok(())
    }),
    ("UIView", false, "semanticContentAttribute", |e| {
        let a = objc::bag_get(e, e.arg(0), "semanticContentAttribute").unwrap_or(0);
        ret(e, a)
    }),
    ("UIView", false, "effectiveUserInterfaceLayoutDirection", |e| {
        let r = view_rtl(e, e.arg(0))?;
        ret(e, dir(r))
    }),
    ("UIView", true, "userInterfaceLayoutDirectionForSemanticContentAttribute:", |e| {
        let r = for_semantic(e, e.arg(2))?;
        ret(e, dir(r))
    }),
    // (attribute, a layout direction for unspecified)
    ("UIView", true, "userInterfaceLayoutDirectionForSemanticContentAttribute:relativeToLayoutDirection:", |e| {
        let r = if e.arg(2) == 0 { e.arg(3) == RTL } else { for_semantic(e, e.arg(2))? };
        ret(e, dir(r))
    }),
    // `+appearanceWhenContainedInInstancesOfClasses:` (react-native-screens
    // sets a navigation bar's direction so in a right-to-left app): a proxy
    // that keeps what's set; appearance by container isn't applied.
    ("UIView", true, "appearanceWhenContainedInInstancesOfClasses:", |e| {
        let cls = objc::host_class(e, "_UIAppearanceContainedProxy")?;
        let p = objc::create_instance(e, cls, 0)?;
        ret(e, p)
    }),
    // Natural (4) unless the app set one.
    ("UILabel", false, "textAlignment", |e| {
        let a = objc::bag_get(e, e.arg(0), "textAlignment").unwrap_or(4);
        ret(e, a)
    }),
    ("UITextView", false, "textAlignment", |e| {
        let a = objc::bag_get(e, e.arg(0), "textAlignment").unwrap_or(4);
        ret(e, a)
    }),
    // NSWritingDirection: natural −1, left to right 0, right to left 1. For
    // no language, the app's direction.
    ("NSParagraphStyle", true, "defaultWritingDirectionForLanguage:", |e| {
        let l = e.arg(2);
        let r = if l == 0 {
            app_rtl(e)?
        } else {
            let l = nsstring(e, l)?;
            language_rtl(e, &l)?
        };
        ret(e, dir(r))
    }),
];
