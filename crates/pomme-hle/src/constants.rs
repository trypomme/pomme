//! System data symbols: constants that apps reference directly.
//!
//! The loader bound every system import to a stub, but these are variables,
//! not functions: the guest reads them. Each one the app imports gets real
//! backing memory here:
//! - NSString constants (notification names, dictionary keys, text styles,
//!   ...): by Apple's convention their value is usually their own name, and
//!   apps only compare them with each other, so that's what pomme uses.
//! - Numbers and structs with Apple's values (font weights, accessibility
//!   traits, `UIBackgroundTaskInvalid`, `UIEdgeInsetsZero`, ...).

use crate::{Emu, HostFn, Result, Stop, foundation};

/// Name patterns of NSString constants.
const STRING_SUFFIXES: &[&str] = &[
    "Notification", "Key", "Name", "Category", "Attribute", "Identifier", "Option", "Type", "Domain", "Exception", "Mode",
    // Font descriptor trait keys (UIFontWeightTrait, kCTFontSymbolicTrait …).
    "Trait",
];
const STRING_PREFIXES: &[&str] = &[
    "NSLocale",
    "kIOSurface",
    "UIApplicationLaunchOptions",
    "UIApplicationOpenURLOptions",
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
    // CoreTelephony's radio technologies ("CTRadioAccessTechnologyLTE").
    "CTRadioAccessTechnology",
    "kCTFont",
    // ImageIO's option keys are their own names.
    "kCGImageSource",
    "kCGImageDestination",
];

/// String constants whose values aren't their names: (name, value).
const STRINGS: &[(&str, &str)] = &[
    // Key-value observing's change dictionary keys (kvo.rs uses these).
    ("NSKeyValueChangeKindKey", "kind"),
    ("NSKeyValueChangeNewKey", "new"),
    ("NSKeyValueChangeOldKey", "old"),
    ("NSKeyValueChangeIndexesKey", "indexes"),
    ("NSKeyValueChangeNotificationIsPriorKey", "notificationIsPrior"),
    // Core Animation's names, as its functions and properties compare them.
    ("kCAFillModeForwards", "forwards"),
    ("kCAFillModeBackwards", "backwards"),
    ("kCAFillModeBoth", "both"),
    ("kCAFillModeRemoved", "removed"),
    ("kCAMediaTimingFunctionLinear", "linear"),
    ("kCAMediaTimingFunctionEaseIn", "easeIn"),
    ("kCAMediaTimingFunctionEaseOut", "easeOut"),
    ("kCAMediaTimingFunctionEaseInEaseOut", "easeInEaseOut"),
    ("kCAMediaTimingFunctionDefault", "default"),
    ("kCAAnimationLinear", "linear"),
    ("kCAAnimationDiscrete", "discrete"),
    ("kCAAnimationPaced", "paced"),
    ("kCAAnimationCubic", "cubic"),
    ("kCAAnimationCubicPaced", "cubicPaced"),
    ("kCACornerCurveCircular", "circular"),
    ("kCACornerCurveContinuous", "continuous"),
    ("kCAFillRuleNonZero", "non-zero"),
    ("kCAFillRuleEvenOdd", "even-odd"),
    ("kCAGradientLayerAxial", "axial"),
    ("kCAGradientLayerRadial", "radial"),
    ("kCAGradientLayerConic", "conic"),
    ("kCAFilterNearest", "nearest"),
    ("kCAFilterLinear", "linear"),
    ("kCAFilterTrilinear", "trilinear"),
    ("kCALineCapButt", "butt"),
    ("kCALineCapRound", "round"),
    ("kCALineCapSquare", "square"),
    ("kCALineJoinMiter", "miter"),
    ("kCALineJoinRound", "round"),
    ("kCALineJoinBevel", "bevel"),
    // Foundation's calendar identifiers.
    ("NSCalendarIdentifierGregorian", "gregorian"),
    ("NSCalendarIdentifierBuddhist", "buddhist"),
    ("NSCalendarIdentifierChinese", "chinese"),
    ("NSCalendarIdentifierCoptic", "coptic"),
    ("NSCalendarIdentifierEthiopicAmeteMihret", "ethiopic"),
    ("NSCalendarIdentifierEthiopicAmeteAlem", "ethiopic-amete-alem"),
    ("NSCalendarIdentifierHebrew", "hebrew"),
    ("NSCalendarIdentifierISO8601", "iso8601"),
    ("NSCalendarIdentifierIndian", "indian"),
    ("NSCalendarIdentifierIslamic", "islamic"),
    ("NSCalendarIdentifierIslamicCivil", "islamic-civil"),
    ("NSCalendarIdentifierIslamicTabular", "islamic-tbla"),
    ("NSCalendarIdentifierIslamicUmmAlQura", "islamic-umalqura"),
    ("NSCalendarIdentifierJapanese", "japanese"),
    ("NSCalendarIdentifierPersian", "persian"),
    ("NSCalendarIdentifierRepublicOfChina", "roc"),
    // UIKit's sheet detent identifiers.
    ("UISheetPresentationControllerDetentIdentifierMedium", "com.apple.UIKit.medium"),
    ("UISheetPresentationControllerDetentIdentifierLarge", "com.apple.UIKit.large"),
    // ImageIO property keys and dictionaries.
    ("kCGImagePropertyPixelWidth", "PixelWidth"),
    ("kCGImagePropertyPixelHeight", "PixelHeight"),
    ("kCGImagePropertyOrientation", "Orientation"),
    ("kCGImagePropertyHasAlpha", "HasAlpha"),
    ("kCGImagePropertyDepth", "Depth"),
    ("kCGImagePropertyColorModel", "ColorModel"),
    ("kCGImagePropertyDPIWidth", "DPIWidth"),
    ("kCGImagePropertyDPIHeight", "DPIHeight"),
    ("kCGImagePropertyFileSize", "FileSize"),
    ("kCGImagePropertyGIFDictionary", "{GIF}"),
    ("kCGImagePropertyGIFDelayTime", "DelayTime"),
    ("kCGImagePropertyGIFUnclampedDelayTime", "UnclampedDelayTime"),
    ("kCGImagePropertyGIFLoopCount", "LoopCount"),
    ("kCGImagePropertyPNGDictionary", "{PNG}"),
    ("kCGImagePropertyAPNGDelayTime", "DelayTime"),
    ("kCGImagePropertyAPNGUnclampedDelayTime", "UnclampedDelayTime"),
    ("kCGImagePropertyAPNGLoopCount", "LoopCount"),
    ("kCGImagePropertyWebPDictionary", "{WebP}"),
    ("kCGImagePropertyWebPDelayTime", "DelayTime"),
    ("kCGImagePropertyWebPUnclampedDelayTime", "UnclampedDelayTime"),
    ("kCGImagePropertyWebPLoopCount", "LoopCount"),
    ("kCGImagePropertyHEICSDictionary", "{HEICS}"),
    ("kCGImagePropertyHEICSDelayTime", "DelayTime"),
    ("kCGImagePropertyHEICSUnclampedDelayTime", "UnclampedDelayTime"),
    ("kCGImagePropertyHEICSLoopCount", "LoopCount"),
    ("kCGImagePropertyJFIFDictionary", "{JFIF}"),
    ("kCGImagePropertyTIFFDictionary", "{TIFF}"),
    ("kCGImagePropertyTIFFOrientation", "Orientation"),
    ("kCGImagePropertyExifDictionary", "{Exif}"),
    // Security: item classes, attributes, search and return keys, and their values.
    ("kSecClass", "class"),
    ("kSecClassGenericPassword", "genp"),
    ("kSecClassInternetPassword", "inet"),
    ("kSecClassCertificate", "cert"),
    ("kSecClassKey", "keys"),
    ("kSecClassIdentity", "idnt"),
    ("kSecAttrAccessible", "pdmn"),
    ("kSecAttrAccessibleWhenUnlocked", "ak"),
    ("kSecAttrAccessibleAfterFirstUnlock", "ck"),
    ("kSecAttrAccessibleAlways", "dk"),
    ("kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly", "akpu"),
    ("kSecAttrAccessibleWhenUnlockedThisDeviceOnly", "aku"),
    ("kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly", "cku"),
    ("kSecAttrAccessibleAlwaysThisDeviceOnly", "dku"),
    ("kSecAttrAccessControl", "accc"),
    ("kSecAttrAccessGroup", "agrp"),
    ("kSecAttrSynchronizable", "sync"),
    ("kSecAttrSynchronizableAny", "syna"),
    ("kSecAttrCreationDate", "cdat"),
    ("kSecAttrModificationDate", "mdat"),
    ("kSecAttrDescription", "desc"),
    ("kSecAttrComment", "icmt"),
    ("kSecAttrCreator", "crtr"),
    ("kSecAttrType", "type"),
    ("kSecAttrLabel", "labl"),
    ("kSecAttrIsInvisible", "invi"),
    ("kSecAttrIsNegative", "nega"),
    ("kSecAttrAccount", "acct"),
    ("kSecAttrService", "svce"),
    ("kSecAttrGeneric", "gena"),
    ("kSecAttrSecurityDomain", "sdmn"),
    ("kSecAttrServer", "srvr"),
    ("kSecAttrProtocol", "ptcl"),
    ("kSecAttrAuthenticationType", "atyp"),
    ("kSecAttrPort", "port"),
    ("kSecAttrPath", "path"),
    ("kSecMatchLimit", "m_Limit"),
    ("kSecMatchLimitOne", "m_LimitOne"),
    ("kSecMatchLimitAll", "m_LimitAll"),
    ("kSecReturnData", "r_Data"),
    ("kSecReturnAttributes", "r_Attributes"),
    ("kSecReturnRef", "r_Ref"),
    ("kSecReturnPersistentRef", "r_PersistentRef"),
    ("kSecValueData", "v_Data"),
    ("kSecValueRef", "v_Ref"),
    ("kSecValuePersistentRef", "v_PersistentRef"),
    ("kSecUseOperationPrompt", "u_OpPrompt"),
    ("kSecUseAuthenticationUI", "u_AuthUI"),
    ("kSecUseAuthenticationUIAllow", "u_AuthUIA"),
    ("kSecUseAuthenticationUIFail", "u_AuthUIF"),
    ("kSecUseAuthenticationUISkip", "u_AuthUIS"),
    ("kSecUseAuthenticationContext", "u_AuthCtx"),
    ("kSecUseDataProtectionKeychain", "nleg"),
    // Uniform Type Identifiers (MobileCoreServices' kUTType…).
    ("kUTTypeItem", "public.item"),
    ("kUTTypeContent", "public.content"),
    ("kUTTypeData", "public.data"),
    ("kUTTypeText", "public.text"),
    ("kUTTypePlainText", "public.plain-text"),
    ("kUTTypeUTF8PlainText", "public.utf8-plain-text"),
    ("kUTTypeUTF16PlainText", "public.utf16-plain-text"),
    ("kUTTypeRTF", "public.rtf"),
    ("kUTTypeHTML", "public.html"),
    ("kUTTypeFlatRTFD", "com.apple.flat-rtfd"),
    ("kUTTypeURL", "public.url"),
    ("kUTTypeFileURL", "public.file-url"),
    ("kUTTypeJSON", "public.json"),
    ("kUTTypePDF", "com.adobe.pdf"),
    ("kUTTypeImage", "public.image"),
    ("kUTTypePNG", "public.png"),
    ("kUTTypeJPEG", "public.jpeg"),
    ("kUTTypeGIF", "com.compuserve.gif"),
    ("kUTTypeTIFF", "public.tiff"),
    ("kUTTypeBMP", "com.microsoft.bmp"),
    ("kUTTypeICO", "com.microsoft.ico"),
    ("kUTTypeAudiovisualContent", "public.audiovisual-content"),
    ("kUTTypeMovie", "public.movie"),
    ("kUTTypeVideo", "public.video"),
    ("kUTTypeAudio", "public.audio"),
    ("kUTTagClassFilenameExtension", "public.filename-extension"),
    ("kUTTagClassMIMEType", "public.mime-type"),
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
    // Window levels (tests/fixtures/devmisc, iOS 26 Simulator).
    ("UIWindowLevelNormal", 0.0),
    ("UIWindowLevelAlert", 2000.0),
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

/// Other integer constants: (name, value).
const INTEGERS: &[(&str, u64)] = &[
    // UIKit's "no background task" identifier: 0.
    ("UIBackgroundTaskInvalid", 0),
];

/// Struct constants that are all zeros: (name, size).
const ZEROS: &[(&str, u64)] = &[("UIEdgeInsetsZero", 32), ("CGRectZero", 32), ("CGPointZero", 16), ("CGSizeZero", 16), ("CGAffineTransformIdentity", 48), ("CATransform3DIdentity", 128)];

/// Rect constants that aren't zero: (name, {x, y, width, height}).
const RECTS: &[(&str, [f64; 4])] = &[
    ("CGRectNull", [f64::INFINITY, f64::INFINITY, 0.0, 0.0]),
    ("CGRectInfinite", [-8.988_465_674_311_579e307, -8.988_465_674_311_579e307, f64::MAX, f64::MAX]),
];

fn is_string_constant(name: &str) -> bool {
    let constant_shape = (name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('k')) && !name.starts_with("OBJC_") && !name.contains('$');
    constant_shape && (STRING_PREFIXES.iter().any(|p| name.starts_with(p)) || (STRING_SUFFIXES.iter().any(|s| name.ends_with(s)) && !names_a_function(name)))
}

/// A CoreFoundation-style function name: one with a verb word in it
/// (`SCNetworkReachabilityCreateWithName`, `CFBundleGetIdentifier`), which a
/// constant's name never has.
fn names_a_function(name: &str) -> bool {
    const VERBS: &[&str] = &["Create", "Copy", "Get", "Set", "Add", "Remove", "Schedule", "Unschedule"];
    VERBS.iter().any(|v| {
        name.match_indices(v).any(|(i, _)| {
            let next = name[i + v.len()..].chars().next();
            i > 0 && next.is_none_or(|c| c.is_ascii_uppercase())
        })
    })
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
    for &(name, v) in TRAITS.iter().chain(INTEGERS) {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let cell = emu.alloc(8)?;
            emu.write_u64(cell, v)?;
            rebind_all(emu, stubs, cell)?;
        }
        handled.insert(name);
    }
    for &(name, value) in STRINGS {
        let stubs = emu.p.stubs.find(&format!("_{name}"));
        if !stubs.is_empty() {
            let s = foundation::new_nsstring(emu, value)?;
            let cell = emu.alloc(8)?;
            emu.write_u64(cell, s)?;
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
            if name == "CATransform3DIdentity" {
                // m11, m22, m33, m44 = 1.
                crate::calayer::write_transform(emu, cell, &crate::calayer::IDENTITY)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_are_not_string_constants() {
        assert!(is_string_constant("UIApplicationDidBecomeActiveNotification"));
        assert!(is_string_constant("NSFileProtectionKey"));
        assert!(is_string_constant("kCFStreamPropertySSLSettingsKey"));
        assert!(!is_string_constant("SCNetworkReachabilityCreateWithName"));
        assert!(!is_string_constant("CFBundleGetIdentifierKey"));
        // Prefixed families are constants whatever their words.
        assert!(is_string_constant("kCGImageSourceCreateThumbnailFromImageAlways"));
    }
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
