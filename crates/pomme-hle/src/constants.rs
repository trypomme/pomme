//! System data symbols: constants that apps reference directly.
//!
//! The loader bound every system import to a stub, but these are variables,
//! not functions: the guest reads them. Each one the app imports gets real
//! backing memory here:
//! - NSString constants (notification names, dictionary keys, text styles,
//!   ...): by Apple's convention their value is usually their own name, and
//!   apps only compare them with each other, so that's what pomme uses.
//! - Numbers and structs with Apple's values (font weights, accessibility
//!   traits, `UIEdgeInsetsZero`, ...).

use crate::{Emu, HostFn, Result, Stop, foundation};

/// Name patterns of NSString constants.
const STRING_SUFFIXES: &[&str] = &[
    "Notification", "Key", "Name", "Category", "Attribute", "Identifier", "Option", "Type", "Domain", "Exception", "Mode",
];
const STRING_PREFIXES: &[&str] = &[
    "UIContentSizeCategory",
    "UIFontTextStyle",
    "UIFontDescriptorSystemDesign",
    "UITextContentType",
    "UIMenu",
    "UIKeyInput",
    "UIAccessibilityPriority",
    "UIAccessibilitySpeechAttribute",
    "NSUserActivityType",
    "NSFile",
    "kSec",
    "kUT",
    "kCTFont",
];

/// Numeric constants: (name, f64 value).
const DOUBLES: &[(&str, f64)] = &[
    ("UIFontWeightUltraLight", -0.8),
    ("UIFontWeightThin", -0.6),
    ("UIFontWeightLight", -0.4),
    ("UIFontWeightRegular", 0.0),
    ("UIFontWeightMedium", 0.23),
    ("UIFontWeightSemibold", 0.3),
    ("UIFontWeightBold", 0.4),
    ("UIFontWeightHeavy", 0.56),
    ("UIFontWeightBlack", 0.62),
    ("UIScrollViewDecelerationRateNormal", 0.998),
    ("UIScrollViewDecelerationRateFast", 0.99),
    ("UINavigationControllerHideShowBarDuration", 0.33),
];

/// `UIAccessibilityTraits` bits.
const TRAITS: &[(&str, u64)] = &[
    ("UIAccessibilityTraitNone", 0),
    ("UIAccessibilityTraitButton", 1),
    ("UIAccessibilityTraitLink", 1 << 1),
    ("UIAccessibilityTraitImage", 1 << 2),
    ("UIAccessibilityTraitSelected", 1 << 3),
    ("UIAccessibilityTraitPlaysSound", 1 << 4),
    ("UIAccessibilityTraitKeyboardKey", 1 << 5),
    ("UIAccessibilityTraitStaticText", 1 << 6),
    ("UIAccessibilityTraitSummaryElement", 1 << 7),
    ("UIAccessibilityTraitNotEnabled", 1 << 8),
    ("UIAccessibilityTraitUpdatesFrequently", 1 << 9),
    ("UIAccessibilityTraitSearchField", 1 << 10),
    ("UIAccessibilityTraitStartsMediaSession", 1 << 11),
    ("UIAccessibilityTraitAdjustable", 1 << 12),
    ("UIAccessibilityTraitAllowsDirectInteraction", 1 << 13),
    ("UIAccessibilityTraitCausesPageTurn", 1 << 14),
    ("UIAccessibilityTraitHeader", 1 << 16),
    ("UIAccessibilityTraitTabBar", 1 << 28),
];

/// Struct constants that are all zeros: (name, size).
const ZEROS: &[(&str, u64)] = &[("UIEdgeInsetsZero", 32), ("CGRectZero", 32), ("CGPointZero", 16), ("CGSizeZero", 16), ("CGAffineTransformIdentity", 48)];

/// Rect constants that aren't zero: (name, {x, y, width, height}).
const RECTS: &[(&str, [f64; 4])] = &[
    ("CGRectNull", [f64::INFINITY, f64::INFINITY, 0.0, 0.0]),
    ("CGRectInfinite", [-8.988_465_674_311_579e307, -8.988_465_674_311_579e307, f64::MAX, f64::MAX]),
];

fn is_string_constant(name: &str) -> bool {
    let constant_shape = (name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('k')) && !name.starts_with("OBJC_") && !name.contains('$');
    constant_shape && (STRING_SUFFIXES.iter().any(|s| name.ends_with(s)) || STRING_PREFIXES.iter().any(|p| name.starts_with(p)))
}

fn rebind_all(emu: &mut Emu, stubs: Vec<u64>, target: u64) -> Result<()> {
    for s in stubs {
        emu.p.rebind(s, target).map_err(|e| Stop::Unsupported(e.to_string()))?;
    }
    Ok(())
}

/// Give every system data constant the app imports real memory. Needs CoreFoundation.
pub(crate) fn bind(emu: &mut Emu) -> Result<()> {
    // Numbers and structs first: their names can look like string constants.
    let mut handled = std::collections::HashSet::new();
    for &(name, v) in DOUBLES {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let cell = emu.alloc(8)?;
            emu.write_u64(cell, v.to_bits())?;
            rebind_all(emu, stubs, cell)?;
        }
        handled.insert(name);
    }
    for &(name, v) in TRAITS {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let cell = emu.alloc(8)?;
            emu.write_u64(cell, v)?;
            rebind_all(emu, stubs, cell)?;
        }
        handled.insert(name);
    }
    for &(name, size) in ZEROS {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let cell = emu.alloc(size)?;
            if name == "CGAffineTransformIdentity" {
                // {a=1, b=0, c=0, d=1, tx=0, ty=0}
                emu.write_u64(cell, 1f64.to_bits())?;
                emu.write_u64(cell + 24, 1f64.to_bits())?;
            }
            rebind_all(emu, stubs, cell)?;
        }
        handled.insert(name);
    }
    for &(name, r) in RECTS {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let cell = emu.alloc(32)?;
            for (i, v) in r.iter().enumerate() {
                emu.write_u64(cell + i as u64 * 8, v.to_bits())?;
            }
            rebind_all(emu, stubs, cell)?;
        }
        handled.insert(name);
    }

    // NSString constants from the system frameworks.
    let candidates: Vec<(u64, String)> = emu
        .p
        .stubs
        .iter()
        .filter(|(_, s)| s.library.starts_with("/System/Library/Frameworks/"))
        .filter_map(|(addr, s)| {
            let name = s.symbol.strip_prefix('_')?;
            (is_string_constant(name) && !handled.contains(name)).then(|| (addr, name.to_string()))
        })
        .collect();
    for (stub, name) in candidates {
        if emu.p.stubs.index_of(stub).is_some_and(|i| emu.has_handler(i)) {
            continue; // implemented as a function
        }
        let s = foundation::new_nsstring(emu, &name)?;
        let cell = emu.alloc(8)?;
        emu.write_u64(cell, s)?;
        emu.p.rebind(stub, cell).map_err(|e| Stop::Unsupported(e.to_string()))?;
    }
    Ok(())
}

// Accessibility queries: a sighted user with default settings.
fn no(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_UIAccessibilityIsVoiceOverRunning", no),
    ("_UIAccessibilityIsSwitchControlRunning", no),
    ("_UIAccessibilityIsBoldTextEnabled", no),
    ("_UIAccessibilityIsGrayscaleEnabled", no),
    ("_UIAccessibilityIsInvertColorsEnabled", no),
    ("_UIAccessibilityIsReduceMotionEnabled", no),
    ("_UIAccessibilityIsReduceTransparencyEnabled", no),
    ("_UIAccessibilityDarkerSystemColorsEnabled", no),
    ("_UIAccessibilityPrefersCrossFadeTransitions", no),
    ("_UIAccessibilityIsMonoAudioEnabled", no),
    ("_UIAccessibilityIsClosedCaptioningEnabled", no),
    ("_UIAccessibilityIsGuidedAccessEnabled", no),
    ("_UIAccessibilityIsShakeToUndoEnabled", no),
    ("_UIAccessibilityIsSpeakScreenEnabled", no),
    ("_UIAccessibilityIsSpeakSelectionEnabled", no),
    ("_UIAccessibilityIsAssistiveTouchRunning", no),
    ("_UIAccessibilityIsOnOffSwitchLabelsEnabled", no),
    ("_UIAccessibilityIsVideoAutoplayEnabled", no),
    ("_UIAccessibilityButtonShapesEnabled", no),
    ("_UIAccessibilityFocusedElement", no),
    ("_UIAccessibilityPostNotification", no),
];
