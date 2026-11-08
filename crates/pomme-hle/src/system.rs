//! Which iOS pomme presents itself as, and what the app was built against.
//!
//! iOS has no single "design mode". What an app sees comes from two inputs:
//!
//! - the OS version on the device, which decides which APIs exist and what
//!   `@available` / `#available` checks answer; and
//! - the SDK the app was linked against (`LC_BUILD_VERSION`), which UIKit
//!   consults behaviour by behaviour ("linked on or after") so old apps keep
//!   old behaviour on new systems. `UIDesignRequiresCompatibility` in the
//!   Info.plist opts an iOS 26-SDK app out of the new design.
//!
//! Every version source pomme answers (availability checks, NSProcessInfo,
//! UIDevice, uname, class lookup) reads [`System::os`]; UIKit behaviours ask
//! [`System::uikit_at_least`]. Liquid Glass is one such behaviour.
//!
//! One exception: pomme's system libraries (the Swift runtime, Foundation)
//! *are* the OS, built from current sources. A real iOS 18 ships an iOS 18
//! runtime, whose availability checks agree with itself; ours must likewise
//! see the release it was built for, so availability checks are answered by
//! caller: app code sees the presented OS, and checks inside pomme's system
//! libraries always pass (the Swift runtime's checks are about its own
//! feature set, e.g. "Swift 6.3", not about the device).

/// A version packed as dyld does: `major << 16 | minor << 8 | patch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32);

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Version(major << 16 | (minor & 0xff) << 8 | (patch & 0xff))
    }

    pub fn major(self) -> u32 {
        self.0 >> 16
    }

    pub fn minor(self) -> u32 {
        (self.0 >> 8) & 0xff
    }

    pub fn patch(self) -> u32 {
        self.0 & 0xff
    }

    /// "18.6" or "18.6.2", as UIDevice's `systemVersion` writes it.
    pub fn short(self) -> String {
        if self.patch() == 0 { format!("{}.{}", self.major(), self.minor()) } else { format!("{}.{}.{}", self.major(), self.minor(), self.patch()) }
    }

    /// The Darwin kernel release iOS of this version ships with
    /// (iOS 17 → 23, iOS 18 → 24, iOS 26 → 25).
    pub fn darwin_major(self) -> u32 {
        let m = self.major();
        if m >= 26 { m - 1 } else { m + 6 }
    }
}

impl std::str::FromStr for Version {
    type Err = String;

    /// "18", "18.6" or "18.6.1".
    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<u32> = s.split('.').map(|p| p.parse().map_err(|_| format!("not a version: {s}"))).collect::<Result<_, _>>()?;
        match parts[..] {
            [major] => Ok(Version::new(major, 0, 0)),
            [major, minor] => Ok(Version::new(major, minor, 0)),
            [major, minor, patch] => Ok(Version::new(major, minor, patch)),
            _ => Err(format!("not a version: {s}")),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.short())
    }
}

/// The newest iOS pomme models; the default.
pub const LATEST: Version = Version::new(26, 0, 0);

#[derive(Clone, Debug)]
pub struct System {
    /// The iOS version pomme presents.
    pub os: Version,
    /// The SDK the app was linked against (from its `LC_BUILD_VERSION`).
    pub sdk: Version,
    /// `UIDesignRequiresCompatibility`: keep the pre-iOS 26 design.
    pub requires_compatibility: bool,
}

impl Default for System {
    fn default() -> Self {
        System { os: LATEST, sdk: LATEST, requires_compatibility: false }
    }
}

impl System {
    /// `@available(iOS v, *)`.
    pub fn os_at_least(&self, v: Version) -> bool {
        self.os >= v
    }

    /// A UIKit behaviour introduced in iOS `major`: the device runs it and the
    /// app was linked against an SDK that knows about it.
    pub fn uikit_at_least(&self, major: u32) -> bool {
        self.os.major() >= major && self.sdk.major() >= major
    }

    /// The iOS 26 design (Liquid Glass bars, controls and sheets).
    pub fn liquid_glass(&self) -> bool {
        self.uikit_at_least(26) && !self.requires_compatibility
    }
}

/// System classes newer than iOS 16, by the major version that introduced
/// them: on older systems they don't exist (`NSClassFromString` is nil).
const INTRODUCED: &[(&str, u32)] = &[
    ("UIViewLayoutRegion", 26),
    ("UIScrollEdgeEffect", 26),
    ("UIScrollEdgeEffectStyle", 26),
    ("UIGlassEffect", 26),
    ("UIGlassContainerEffect", 26),
    ("UICornerConfiguration", 26),
    ("UIBackgroundExtensionView", 26),
    ("UIMainMenuSystem", 26),
    ("UITab", 18),
    ("UITabGroup", 18),
    ("UIUpdateLink", 18),
    ("UITextFormattingViewController", 18),
    ("UIContentUnavailableConfiguration", 17),
    ("UIContentUnavailableView", 17),
    ("UITraitDefinition", 17),
];

/// Does `class` exist on this system?
pub fn class_available(system: &System, class: &str) -> bool {
    INTRODUCED.iter().find(|(c, _)| *c == class).is_none_or(|&(_, major)| system.os.major() >= major)
}

/// Does code at `return_address` belong to the app (its executable or bundled
/// frameworks), rather than to pomme's system libraries?
pub(crate) fn is_app_code(emu: &crate::Emu, return_address: u64) -> bool {
    let app_dir = emu.p.main().path.parent().map(std::path::Path::to_path_buf);
    emu.p.image_at(return_address).is_some_and(|i| app_dir.as_ref().is_some_and(|d| i.path.starts_with(d)))
}

/// An availability check (`@available` / `#available`) from code at
/// `return_address`: app code is answered with the presented OS; pomme's
/// system libraries always pass (see the module docs).
pub(crate) fn available_for_caller(emu: &crate::Emu, return_address: u64, want: Version) -> bool {
    !is_app_code(emu, return_address) || emu.system.os >= want
}

/// `_stdlib_isOSVersionAtLeast(major, minor, patch)`, which older Swift
/// compilers call for `#available`: answered for its caller.
fn stdlib_is_os_version_at_least(emu: &mut crate::Emu) -> crate::Result<()> {
    let want = Version::new(emu.arg(0) as u32, emu.arg(1) as u32, emu.arg(2) as u32);
    let ok = available_for_caller(emu, emu.cpu.x[30], want);
    emu.ret(ok as u64);
    Ok(())
}

pub(crate) fn install(emu: &mut crate::Emu) -> crate::Result<()> {
    let f = emu.p.images.iter().find_map(|i| i.symbol("_$ss26_stdlib_isOSVersionAtLeastyBi1_Bw_BwBwtF"));
    if let Some(addr) = f {
        emu.hook(addr, stdlib_is_os_version_at_least)?;
    }
    Ok(())
}

/// Read the main executable's SDK and the Info.plist opt-out.
pub(crate) fn init(emu: &mut crate::Emu) {
    if let Some(b) = emu.p.main().build.as_ref() {
        emu.system.sdk = Version(b.sdk.0);
    }
    if let Some(plist::Value::Dictionary(d)) = crate::bundle::host_info_plist(emu) {
        emu.system.requires_compatibility = d.get("UIDesignRequiresCompatibility").and_then(|v| v.as_boolean()).unwrap_or(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        let v: Version = "18.6".parse().unwrap();
        assert_eq!((v.major(), v.minor(), v.patch()), (18, 6, 0));
        assert_eq!(v.short(), "18.6");
        assert_eq!("26.0.1".parse::<Version>().unwrap().short(), "26.0.1");
        assert_eq!(v.darwin_major(), 24);
        assert_eq!(LATEST.darwin_major(), 25);
        assert!("17".parse::<Version>().unwrap() < v);
        assert!("x".parse::<Version>().is_err());
    }

    #[test]
    fn design_follows_os_sdk_and_opt_out() {
        let s = |os: &str, sdk: &str, compat| System { os: os.parse().unwrap(), sdk: sdk.parse().unwrap(), requires_compatibility: compat };
        assert!(s("26.0", "26.5", false).liquid_glass());
        // An iOS 26-SDK app on iOS 18: the old design.
        assert!(!s("18.6", "26.5", false).liquid_glass());
        // An app built with the iOS 18 SDK on iOS 26: still the old design.
        assert!(!s("26.0", "18.4", false).liquid_glass());
        // Opted out in its Info.plist.
        assert!(!s("26.0", "26.5", true).liquid_glass());
        assert!(s("26.0", "26.5", true).uikit_at_least(26));
    }

    #[test]
    fn classes_exist_from_their_release() {
        let ios18 = System { os: "18.6".parse().unwrap(), ..Default::default() };
        assert!(!class_available(&ios18, "UIViewLayoutRegion"));
        assert!(class_available(&ios18, "UITab"));
        assert!(class_available(&ios18, "UIView"));
        assert!(class_available(&System::default(), "UIViewLayoutRegion"));
    }
}
