//! The device pomme presents: an iPhone or iPad model's screen (size in
//! points, scale), its safe area, its sensor housing (a notch or the Dynamic
//! Island, or none on home-button phones and iPads), the screen's corner
//! radius and the keyboard's height.
//!
//! An iPad runs an iPhone-only app (no 2 in `UIDeviceFamily`) in
//! compatibility mode: the app sees a phone-sized screen of the iPad's own
//! ([`Device::compat`]), with the phone idiom, while `UIDevice` and
//! `hw.machine` still name the iPad. pomme keeps both: the screen the app
//! sees (`Emu::device`) and the hardware (`Emu::hardware`).
//!
//! Everything that depends on how the phone is held is asked of the device
//! with an [`Orientation`]: the screen's size, the safe area, the status bar,
//! the keyboard's height and the size classes.
//!
//! Sources: screen sizes and scales are from Xcode 26's simulator device
//! profiles (`/Library/Developer/CoreSimulator/Profiles/DeviceTypes`), notch
//! sizes from their sensor-housing art. Measured in the iOS 26 Simulator on
//! the iPhone 17 (island) and 16e (notch), portrait and landscape: the safe
//! areas, status bars, keyboards and size classes, from which the rules below
//! generalize (landscape: the sides inset by the portrait top, 20 pt at the
//! bottom, no status bar; the same keyboard on both). The other portrait safe
//! areas, islands and corner radii are Apple's published values. Max-size
//! phones' regular width in landscape is UIKit's documented rule (not
//! measured).
//!
//! iPads: screens, scales and model identifiers from the same profiles; the
//! rest measured on the iOS 26.5 Simulator with the devices probe
//! (`crates/pomme-cli/tests/fixtures/devices/ios26.txt`,
//! `scripts/measure/devices/run.sh`): the status bar (32 pt, in every
//! orientation), the safe area (32 at the top, 25 at the bottom full screen,
//! 20 windowed, in every orientation), regular size classes, the keyboards
//! (the predictive bar doesn't change their height), the corners, and the
//! compatibility screens.

/// The sensor housing at the top of the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cutout {
    /// Home-button phones: a plain rectangle of screen.
    None,
    /// A notch hanging from the top edge, centered: width and height (pt),
    /// including the flared shoulders.
    Notch { width: f64, height: f64 },
    /// The Dynamic Island: a capsule, centered, `top` pt below the edge.
    Island { top: f64, width: f64, height: f64 },
}

/// How the interface is turned: UIInterfaceOrientation's values. (A device
/// turned to UIDeviceOrientationLandscapeLeft shows LandscapeRight: the
/// enums mirror each other with the same numbers.)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Orientation {
    #[default]
    Portrait = 1,
    PortraitUpsideDown = 2,
    /// Turned counter-clockwise: the sensor housing on the left.
    LandscapeRight = 3,
    /// Turned clockwise: the sensor housing on the right.
    LandscapeLeft = 4,
}

impl Orientation {
    pub fn is_landscape(self) -> bool {
        matches!(self, Orientation::LandscapeLeft | Orientation::LandscapeRight)
    }

    pub fn from_raw(v: u64) -> Option<Orientation> {
        Some(match v {
            1 => Orientation::Portrait,
            2 => Orientation::PortraitUpsideDown,
            3 => Orientation::LandscapeRight,
            4 => Orientation::LandscapeLeft,
            _ => return None,
        })
    }

    /// Its bit in UIInterfaceOrientationMask.
    pub fn mask(self) -> u64 {
        1 << self as u64
    }

    /// The affine map `[a, b, c, d, tx, ty]` (x' = a·x + c·y + tx,
    /// y' = b·x + d·y + ty) from the device's native portrait frame of size
    /// `w`×`h` into the interface's frame. Measured: in LandscapeRight the
    /// Simulator's portrait screenshot holds app point (x, y) at native
    /// (w − y, x), so the island (native top) is on the interface's left.
    pub fn from_portrait(self, w: f64, h: f64) -> [f64; 6] {
        match self {
            Orientation::Portrait => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            Orientation::LandscapeRight => [0.0, -1.0, 1.0, 0.0, 0.0, w],
            Orientation::LandscapeLeft => [0.0, 1.0, -1.0, 0.0, h, 0.0],
            Orientation::PortraitUpsideDown => [-1.0, 0.0, 0.0, -1.0, w, h],
        }
    }
}

impl std::str::FromStr for Orientation {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        Ok(match s {
            "portrait" => Orientation::Portrait,
            "portrait-upside-down" | "upside-down" => Orientation::PortraitUpsideDown,
            "landscape-left" => Orientation::LandscapeLeft,
            // The usual way: the phone turned to the left, the housing on the left.
            "landscape" | "landscape-right" => Orientation::LandscapeRight,
            o => return Err(format!("{o}: expected portrait, landscape (= landscape-right), landscape-left or portrait-upside-down")),
        })
    }
}

/// UIUserInterfaceIdiom: what kind of device the interface is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idiom {
    Phone = 0,
    Pad = 1,
}

/// UIUserInterfaceSizeClass
pub const COMPACT: u64 = 1;
pub const REGULAR: u64 = 2;

#[derive(Clone, Copy, Debug)]
pub struct Device {
    /// What `--device` takes: `iphone-17-pro-max`.
    pub id: &'static str,
    pub name: &'static str,
    /// The screen in points as the phone is made (portrait), and pixels per point.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    /// Portrait safe area: the status bar / sensor housing, and the home indicator.
    pub safe_top: f64,
    pub safe_bottom: f64,
    /// The portrait status bar's height (shorter than the safe area's top on island phones).
    pub status_bar: f64,
    pub cutout: Cutout,
    /// The display's rounded corners (0: square).
    pub corner_radius: f64,
    pub home_button: bool,
    pub idiom: Idiom,
    /// Touch ID rather than Face ID (home-button phones; the iPad Air's top button).
    pub touch_id: bool,
    /// The keyboard's overall height (bottom inset included) where it was
    /// measured as such: [portrait, landscape] × [predictive, plain]. None:
    /// the iPhone rule (`KEYS` above the bottom inset).
    pub keyboard: Option<[[f64; 2]; 2]>,
    /// The landscape side insets, where they aren't the portrait top
    /// (compatibility screens).
    pub landscape_sides: Option<f64>,
    /// An iPad's compatibility screen: what an iPhone-only app sees.
    pub compat: Option<&'static Device>,
}

/// The keys of the iOS 26 keyboard (English) above the bottom inset, with
/// and without the predictive bar, portrait and landscape: measured on the
/// iPhone 17 and 16e (335 / 304 pt overall in portrait with the 34-pt inset;
/// 209 / 164 in landscape with the 20-pt one). (Portrait without predictions
/// was 304 for Flutter's field and 308 for a plain UITextField with
/// autocorrection and spell checking off: another trait is involved.)
const KEYS: [[f64; 2]; 2] = [[301.0, 270.0], [189.0, 144.0]];

/// The bottom inset in landscape on phones with a home indicator.
const LANDSCAPE_BOTTOM: f64 = 20.0;

/// An iPad's bottom inset when the app runs windowed (without
/// `UIRequiresFullScreen`), iPadOS 26's default; 25 full screen (the table's).
const PAD_WINDOWED_BOTTOM: f64 = 20.0;

impl Device {
    /// The screen's size (points) held this way.
    pub fn screen(&self, o: Orientation) -> (f64, f64) {
        if o.is_landscape() { (self.height, self.width) } else { (self.width, self.height) }
    }

    /// Safe-area insets (top, left, bottom, right) held this way, for a
    /// full-screen app.
    pub fn safe_area(&self, o: Orientation) -> [f64; 4] {
        if self.idiom == Idiom::Pad {
            // The same in every orientation.
            return [self.safe_top, 0.0, self.safe_bottom, 0.0];
        }
        if let (true, Some(side)) = (o.is_landscape(), self.landscape_sides) {
            return [0.0, side, LANDSCAPE_BOTTOM, side];
        }
        match o {
            Orientation::Portrait => [self.safe_top, 0.0, self.safe_bottom, 0.0],
            // Face ID phones don't turn upside down; home-button phones keep a status bar.
            Orientation::PortraitUpsideDown => [self.safe_top, 0.0, self.safe_bottom, 0.0],
            _ if self.home_button => [0.0; 4],
            _ => [0.0, self.safe_top, LANDSCAPE_BOTTOM, self.safe_top],
        }
    }

    /// Safe-area insets held this way for an app that runs full screen or
    /// not: an iPad's bottom inset is smaller for a windowed app.
    pub fn safe_area_for(&self, o: Orientation, full_screen: bool) -> [f64; 4] {
        let mut s = self.safe_area(o);
        if self.idiom == Idiom::Pad && !full_screen {
            s[2] = PAD_WINDOWED_BOTTOM;
        }
        s
    }

    /// The status bar's height held this way (iPhones hide it in landscape; iPads don't).
    pub fn status_bar(&self, o: Orientation) -> f64 {
        if o.is_landscape() && self.idiom == Idiom::Phone { 0.0 } else { self.status_bar }
    }

    /// The keyboard's height held this way.
    pub fn keyboard_height(&self, o: Orientation, predictive: bool) -> f64 {
        let (side, plain) = (o.is_landscape() as usize, !predictive as usize);
        if let Some(k) = self.keyboard {
            return k[side][plain];
        }
        KEYS[side][plain] + self.safe_area(o)[2]
    }

    /// (horizontal, vertical) size classes held this way: on iPads regular
    /// both ways; on phones compact width except Max-size phones (414 pt and
    /// wider) in landscape, and compact height in landscape.
    pub fn size_classes(&self, o: Orientation) -> (u64, u64) {
        if self.idiom == Idiom::Pad {
            (REGULAR, REGULAR)
        } else if o.is_landscape() {
            (if self.width >= 414.0 { REGULAR } else { COMPACT }, COMPACT)
        } else {
            (COMPACT, REGULAR)
        }
    }

    /// Orientations the hardware turns to: Face ID phones never turn upside
    /// down; iPads and home-button phones do.
    pub fn orientations(&self) -> u64 {
        let all = [Orientation::Portrait, Orientation::LandscapeLeft, Orientation::LandscapeRight];
        let mut m = all.iter().fold(0, |m, o| m | o.mask());
        if self.home_button || self.idiom == Idiom::Pad {
            m |= Orientation::PortraitUpsideDown.mask();
        }
        m
    }
}

/// Each phone's hardware model identifier, as `uname` and `sysctl hw.machine`
/// report it (Apple's identifiers; the board-level `hw.model` isn't modelled).
const MODELS: &[(&str, &str)] = &[
    ("iphone-se", "iPhone14,6"),
    ("iphone-8", "iPhone10,4"),
    ("iphone-8-plus", "iPhone10,5"),
    ("iphone-x", "iPhone10,6"),
    ("iphone-xs", "iPhone11,2"),
    ("iphone-xs-max", "iPhone11,6"),
    ("iphone-xr", "iPhone11,8"),
    ("iphone-11", "iPhone12,1"),
    ("iphone-11-pro", "iPhone12,3"),
    ("iphone-11-pro-max", "iPhone12,5"),
    ("iphone-12-mini", "iPhone13,1"),
    ("iphone-12", "iPhone13,2"),
    ("iphone-12-pro", "iPhone13,3"),
    ("iphone-12-pro-max", "iPhone13,4"),
    ("iphone-13-mini", "iPhone14,4"),
    ("iphone-13", "iPhone14,5"),
    ("iphone-13-pro", "iPhone14,2"),
    ("iphone-13-pro-max", "iPhone14,3"),
    ("iphone-14", "iPhone14,7"),
    ("iphone-14-plus", "iPhone14,8"),
    ("iphone-14-pro", "iPhone15,2"),
    ("iphone-14-pro-max", "iPhone15,3"),
    ("iphone-15", "iPhone15,4"),
    ("iphone-15-plus", "iPhone15,5"),
    ("iphone-15-pro", "iPhone16,1"),
    ("iphone-15-pro-max", "iPhone16,2"),
    ("iphone-16", "iPhone17,3"),
    ("iphone-16-plus", "iPhone17,4"),
    ("iphone-16-pro", "iPhone17,1"),
    ("iphone-16-pro-max", "iPhone17,2"),
    ("iphone-16e", "iPhone17,5"),
    ("iphone-17", "iPhone18,3"),
    ("iphone-17-pro", "iPhone18,1"),
    ("iphone-17-pro-max", "iPhone18,2"),
    ("iphone-17e", "iPhone18,5"),
    ("iphone-air", "iPhone18,4"),
    ("ipad-pro-11-m5", "iPad17,2"),
    ("ipad-pro-13-m5", "iPad17,4"),
    ("ipad-air-11-m4", "iPad16,9"),
    ("ipad-air-13-m4", "iPad16,11"),
];

impl Device {
    /// The hardware model identifier ("iPhone17,5").
    pub fn model(&self) -> &'static str {
        MODELS.iter().find(|(id, _)| *id == self.id).map_or(self.kind(), |m| m.1)
    }

    /// `UIDevice.model`: "iPhone" or "iPad".
    pub fn kind(&self) -> &'static str {
        match self.idiom {
            Idiom::Phone => "iPhone",
            Idiom::Pad => "iPad",
        }
    }

    /// `UIDevice.systemName`: "iOS", or "iPadOS" on iPads.
    pub fn system_name(&self) -> &'static str {
        match self.idiom {
            Idiom::Phone => "iOS",
            Idiom::Pad => "iPadOS",
        }
    }

    /// The screen an app sees on this device: an iPad's compatibility
    /// screen for an app that doesn't list the iPad (family 2) in
    /// `UIDeviceFamily`; else the device's own.
    pub fn for_app(&'static self, families: &[u64]) -> &'static Device {
        match self.compat {
            Some(c) if !families.contains(&2) => c,
            _ => self,
        }
    }
}

const fn island(top: f64) -> Cutout {
    Cutout::Island { top, width: 126.0, height: 37.33 }
}

const fn phone(id: &'static str, name: &'static str, size: (f64, f64), scale: f64, safe: (f64, f64), cutout: Cutout, corner_radius: f64) -> Device {
    // The status bar fills the safe area's top on notch phones; on island
    // phones it's 54 pt (measured on the iPhone 17).
    let status_bar = if matches!(cutout, Cutout::Island { .. }) { 54.0 } else { safe.0 };
    Device { id, name, width: size.0, height: size.1, scale, safe_top: safe.0, safe_bottom: safe.1, status_bar, cutout, corner_radius, home_button: false, ..PHONE }
}

const fn home_button(id: &'static str, name: &'static str, size: (f64, f64), scale: f64) -> Device {
    Device { id, name, width: size.0, height: size.1, scale, safe_top: 20.0, safe_bottom: 0.0, status_bar: 20.0, cutout: Cutout::None, corner_radius: 0.0, home_button: true, touch_id: true, ..PHONE }
}

/// What every row shares unless it says otherwise.
const PHONE: Device = Device {
    id: "",
    name: "",
    width: 0.0,
    height: 0.0,
    scale: 3.0,
    safe_top: 0.0,
    safe_bottom: 0.0,
    status_bar: 0.0,
    cutout: Cutout::None,
    corner_radius: 0.0,
    home_button: false,
    idiom: Idiom::Phone,
    touch_id: false,
    keyboard: None,
    landscape_sides: None,
    compat: None,
};

/// An iPad: scale 2, status bar 32 pt, safe area 32 / 25 (full screen),
/// no sensor housing on the screen. `keyboard`: (portrait, landscape),
/// with or without predictions.
const fn ipad(id: &'static str, name: &'static str, size: (f64, f64), corner_radius: f64, keyboard: (f64, f64), touch_id: bool, compat: &'static Device) -> Device {
    Device {
        id,
        name,
        width: size.0,
        height: size.1,
        scale: 2.0,
        safe_top: 32.0,
        safe_bottom: 25.0,
        status_bar: 32.0,
        corner_radius,
        idiom: Idiom::Pad,
        touch_id,
        keyboard: Some([[keyboard.0, keyboard.0], [keyboard.1, keyboard.1]]),
        compat: Some(compat),
        ..PHONE
    }
}

/// The screens iPadOS gives iPhone-only apps (measured): phone-sized, at the
/// iPad's scale 2, insets 20 / 25 in portrait and 0, 25, 20, 25 on its side,
/// the iPhone status bar hidden in landscape. Not in the table: only an iPad
/// shows them.
const COMPAT_375: Device = Device {
    id: "ipad-compat-375",
    name: "iPhone app on iPad (375 × 667)",
    width: 375.0,
    height: 667.0,
    scale: 2.0,
    safe_top: 20.0,
    safe_bottom: 25.0,
    status_bar: 20.0,
    keyboard: Some([[260.0, 233.0], [206.0, 162.0]]),
    landscape_sides: Some(25.0),
    ..PHONE
};

const COMPAT_390: Device = Device {
    id: "ipad-compat-390",
    name: "iPhone app on iPad (390 × 844)",
    width: 390.0,
    height: 844.0,
    scale: 2.0,
    safe_top: 20.0,
    safe_bottom: 25.0,
    status_bar: 44.0,
    corner_radius: 47.33,
    keyboard: Some([[335.0, 308.0], [208.0, 164.0]]),
    landscape_sides: Some(25.0),
    ..PHONE
};

const NOTCH_X: Cutout = Cutout::Notch { width: 223.0, height: 30.0 };
const NOTCH_XR: Cutout = Cutout::Notch { width: 246.0, height: 33.0 };
const NOTCH_12: Cutout = Cutout::Notch { width: 176.0, height: 34.0 };
const NOTCH_MINI: Cutout = Cutout::Notch { width: 189.0, height: 38.0 };

pub const DEVICES: &[Device] = &[
    home_button("iphone-se", "iPhone SE (3rd generation)", (375.0, 667.0), 2.0),
    home_button("iphone-8", "iPhone 8", (375.0, 667.0), 2.0),
    home_button("iphone-8-plus", "iPhone 8 Plus", (414.0, 736.0), 3.0),
    phone("iphone-x", "iPhone X", (375.0, 812.0), 3.0, (44.0, 34.0), NOTCH_X, 39.0),
    phone("iphone-xs", "iPhone Xs", (375.0, 812.0), 3.0, (44.0, 34.0), NOTCH_X, 39.0),
    phone("iphone-xs-max", "iPhone Xs Max", (414.0, 896.0), 3.0, (44.0, 34.0), NOTCH_X, 39.0),
    phone("iphone-xr", "iPhone Xʀ", (414.0, 896.0), 2.0, (48.0, 34.0), NOTCH_XR, 41.5),
    phone("iphone-11", "iPhone 11", (414.0, 896.0), 2.0, (48.0, 34.0), NOTCH_XR, 41.5),
    phone("iphone-11-pro", "iPhone 11 Pro", (375.0, 812.0), 3.0, (44.0, 34.0), NOTCH_X, 39.0),
    phone("iphone-11-pro-max", "iPhone 11 Pro Max", (414.0, 896.0), 3.0, (44.0, 34.0), NOTCH_X, 39.0),
    phone("iphone-12-mini", "iPhone 12 mini", (360.0, 780.0), 3.0, (50.0, 34.0), NOTCH_MINI, 44.0),
    phone("iphone-12", "iPhone 12", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-12-pro", "iPhone 12 Pro", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-12-pro-max", "iPhone 12 Pro Max", (428.0, 926.0), 3.0, (47.0, 34.0), NOTCH_12, 53.33),
    phone("iphone-13-mini", "iPhone 13 mini", (360.0, 780.0), 3.0, (50.0, 34.0), NOTCH_MINI, 44.0),
    phone("iphone-13", "iPhone 13", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-13-pro", "iPhone 13 Pro", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-13-pro-max", "iPhone 13 Pro Max", (428.0, 926.0), 3.0, (47.0, 34.0), NOTCH_12, 53.33),
    phone("iphone-14", "iPhone 14", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-14-plus", "iPhone 14 Plus", (428.0, 926.0), 3.0, (47.0, 34.0), NOTCH_12, 53.33),
    phone("iphone-14-pro", "iPhone 14 Pro", (393.0, 852.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-14-pro-max", "iPhone 14 Pro Max", (430.0, 932.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-15", "iPhone 15", (393.0, 852.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-15-plus", "iPhone 15 Plus", (430.0, 932.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-15-pro", "iPhone 15 Pro", (393.0, 852.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-15-pro-max", "iPhone 15 Pro Max", (430.0, 932.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-16", "iPhone 16", (393.0, 852.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-16-plus", "iPhone 16 Plus", (430.0, 932.0), 3.0, (59.0, 34.0), island(11.0), 55.0),
    phone("iphone-16-pro", "iPhone 16 Pro", (402.0, 874.0), 3.0, (62.0, 34.0), island(14.0), 62.0),
    phone("iphone-16-pro-max", "iPhone 16 Pro Max", (440.0, 956.0), 3.0, (62.0, 34.0), island(14.0), 62.0),
    phone("iphone-16e", "iPhone 16e", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    phone("iphone-17", "iPhone 17", (402.0, 874.0), 3.0, (62.0, 34.0), island(14.0), 62.0),
    phone("iphone-17-pro", "iPhone 17 Pro", (402.0, 874.0), 3.0, (62.0, 34.0), island(14.0), 62.0),
    phone("iphone-17-pro-max", "iPhone 17 Pro Max", (440.0, 956.0), 3.0, (62.0, 34.0), island(14.0), 62.0),
    phone("iphone-17e", "iPhone 17e", (390.0, 844.0), 3.0, (47.0, 34.0), NOTCH_12, 47.33),
    // (Its island sits lower: measured on its iOS 26 simulator, 19.67 pt down.)
    phone("iphone-air", "iPhone Air", (420.0, 912.0), 3.0, (68.0, 34.0), island(19.67), 62.0),
    ipad("ipad-pro-11-m5", "iPad Pro 11-inch (M5)", (834.0, 1210.0), 30.0, (340.0, 428.0), false, &COMPAT_375),
    ipad("ipad-pro-13-m5", "iPad Pro 13-inch (M5)", (1032.0, 1376.0), 30.0, (403.0, 498.0), false, &COMPAT_390),
    ipad("ipad-air-11-m4", "iPad Air 11-inch (M4)", (820.0, 1180.0), 18.0, (337.0, 422.0), true, &COMPAT_375),
    ipad("ipad-air-13-m4", "iPad Air 13-inch (M4)", (1024.0, 1366.0), 18.0, (403.0, 498.0), true, &COMPAT_390),
];

/// The device families an app's Info.plist lists in `UIDeviceFamily` (1
/// iPhone, 2 iPad); iPhone only when it doesn't say, as Xcode's default.
pub fn app_families(info: Option<&plist::Value>) -> Vec<u64> {
    let list = info.and_then(|v| v.as_dictionary()).and_then(|d| d.get("UIDeviceFamily"));
    let number = |v: &plist::Value| v.as_unsigned_integer().or_else(|| v.as_string().and_then(|s| s.trim().parse().ok()));
    match list {
        Some(plist::Value::Array(a)) => a.iter().filter_map(number).collect(),
        Some(v) => number(v).into_iter().collect(),
        None => vec![1],
    }
}

/// The device pomme presents unless told otherwise: the 390×844 iPhone 16e.
pub const DEFAULT: &str = "iphone-16e";

/// A device by id (`iphone-17-pro`) or name (`iPhone 17 Pro`), case and spacing aside.
pub fn find(name: &str) -> Option<&'static Device> {
    let key = |s: &str| s.to_lowercase().replace(['(', ')'], "").split_whitespace().collect::<Vec<_>>().join("-");
    let k = key(name);
    DEVICES.iter().find(|d| d.id == k || key(d.name) == k)
}

pub fn default() -> &'static Device {
    find(DEFAULT).expect("the default device is in the table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_by_id_and_name() {
        assert_eq!(find("iphone-17-pro-max").unwrap().width, 440.0);
        assert_eq!(find("iPhone 17 Pro Max").unwrap().id, "iphone-17-pro-max");
        assert_eq!(find("iPhone SE (3rd generation)").unwrap().scale, 2.0);
        assert!(find("iphone-99").is_none());
        assert_eq!(default().height, 844.0);
    }

    #[test]
    fn keyboard_heights_match_the_simulator() {
        use Orientation::*;
        // The iPhone 17's and 16e's, measured: 335 pt with the predictive bar, 304 without;
        // in landscape 209 and 164.
        for id in ["iphone-17", "iphone-16e"] {
            let d = find(id).unwrap();
            assert_eq!(d.keyboard_height(Portrait, true), 335.0);
            assert_eq!(d.keyboard_height(Portrait, false), 304.0);
            assert_eq!(d.keyboard_height(LandscapeRight, true), 209.0);
            assert_eq!(d.keyboard_height(LandscapeLeft, false), 164.0);
        }
        // No home indicator: no bottom inset under the keys.
        assert_eq!(find("iphone-se").unwrap().keyboard_height(Portrait, true), 301.0);
    }

    #[test]
    fn landscape_matches_the_simulator() {
        use Orientation::*;
        let d = find("iphone-17").unwrap();
        assert_eq!(d.screen(LandscapeRight), (874.0, 402.0));
        assert_eq!(d.safe_area(LandscapeRight), [0.0, 62.0, 20.0, 62.0]);
        assert_eq!(d.safe_area(LandscapeLeft), [0.0, 62.0, 20.0, 62.0]);
        assert_eq!(d.status_bar(Portrait), 54.0);
        assert_eq!(d.status_bar(LandscapeLeft), 0.0);
        assert_eq!(d.size_classes(Portrait), (COMPACT, REGULAR));
        assert_eq!(d.size_classes(LandscapeLeft), (COMPACT, COMPACT));
        let e = find("iphone-16e").unwrap();
        assert_eq!(e.model(), "iPhone17,5");
        assert!(DEVICES.iter().all(|d| d.model().starts_with(d.kind()) && d.model().contains(',')), "every device has a model identifier");
        assert_eq!(e.safe_area(LandscapeLeft), [0.0, 47.0, 20.0, 47.0]);
        assert_eq!(e.status_bar(Portrait), 47.0);
        assert_eq!(find("iphone-17-pro-max").unwrap().size_classes(LandscapeRight), (REGULAR, COMPACT));
        // Face ID phones don't turn upside down; home-button ones do.
        assert_eq!(d.orientations() & PortraitUpsideDown.mask(), 0);
        assert_ne!(find("iphone-se").unwrap().orientations() & PortraitUpsideDown.mask(), 0);
    }

    #[test]
    fn portrait_frame_maps_as_the_simulator_shows() {
        use Orientation::*;
        let apply = |m: [f64; 6], x: f64, y: f64| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        let (w, h) = (402.0, 874.0);
        // The island (native top centre) sits on the left in LandscapeRight,
        // the right in LandscapeLeft, the bottom upside down.
        assert_eq!(apply(LandscapeRight.from_portrait(w, h), w / 2.0, 0.0), (0.0, w / 2.0));
        assert_eq!(apply(LandscapeLeft.from_portrait(w, h), w / 2.0, 0.0), (h, w / 2.0));
        assert_eq!(apply(PortraitUpsideDown.from_portrait(w, h), w / 2.0, 0.0), (w / 2.0, h));
        // The native frame covers the interface frame exactly.
        for o in [Portrait, LandscapeRight, LandscapeLeft, PortraitUpsideDown] {
            let (iw, ih) = if o.is_landscape() { (h, w) } else { (w, h) };
            let m = o.from_portrait(w, h);
            for (x, y) in [(0.0, 0.0), (w, h)] {
                let (px, py) = apply(m, x, y);
                assert!((0.0..=iw).contains(&px) && (0.0..=ih).contains(&py), "{o:?}");
            }
        }
    }

    /// Every line of the devices probe's recording that the table answers,
    /// for each iPad (as an iPad app full screen and windowed, and as an
    /// iPhone-only app) and the iPhone 16e.
    #[test]
    fn devices_match_the_simulator() {
        use Orientation::*;
        let rec = include_str!("../../pomme-cli/tests/fixtures/devices/ios26.txt");
        let mut checked = 0;
        for line in rec.lines().filter_map(|l| l.strip_prefix("METRIC ")) {
            let (key, value) = line.split_once(' ').unwrap();
            let mut k = key.splitn(4, '.');
            let (id, mode, phase, name) = (k.next().unwrap(), k.next().unwrap(), k.next().unwrap(), k.next().unwrap());
            // A windowed iPad app couldn't turn itself: its "landscape" is portrait (see tasks).
            if mode == "native" && phase == "landscape" && id.starts_with("ipad") {
                continue;
            }
            let hw = find(id).unwrap();
            let d = if mode == "compat" { hw.for_app(&[1]) } else { hw.for_app(&[1, 2]) };
            let o = if phase == "landscape" { LandscapeRight } else { Portrait };
            let nums: Vec<f64> = value.split(' ').filter_map(|v| v.parse().ok()).collect();
            let (w, h) = d.screen(o);
            match name {
                "screen" | "window" => assert_eq!(nums, [0.0, 0.0, w, h], "{key}"),
                "native" => assert_eq!(nums, [0.0, 0.0, d.width * d.scale, d.height * d.scale, d.scale, d.scale], "{key}"),
                "safe" => assert_eq!(nums, d.safe_area_for(o, mode != "native"), "{key}"),
                "statusbar" => assert_eq!(nums[3], d.status_bar(o), "{key}"),
                "corner" => assert_eq!(nums[0], d.corner_radius, "{key}"),
                "sizeclasses" => assert_eq!((nums[0] as u64, nums[1] as u64), d.size_classes(o), "{key}"),
                "idiom" => assert_eq!(nums, [d.idiom as u64 as f64; 2], "{key}"),
                "device" => {
                    let f: Vec<&str> = value.split('|').collect();
                    assert_eq!(f, [hw.system_name(), hw.kind(), hw.kind(), hw.model()], "{key}");
                }
                // The iPhone 16e's plain keyboard (308 here, 304 for Flutter's
                // field) and its 26.5 landscape one (208; 209 on 26.2) are
                // known differences: see the KEYS comment and the tasks.
                "kb.predictive" | "kb.plain" if d.keyboard.is_none() => continue,
                "kb.predictive" => assert_eq!(nums[3], d.keyboard_height(o, true), "{key}"),
                "kb.plain" => assert_eq!(nums[3], d.keyboard_height(o, false), "{key}"),
                _ => continue,
            }
            checked += 1;
        }
        assert!(checked > 200, "only {checked} lines checked");
    }

    #[test]
    fn ipads() {
        use Orientation::*;
        let p = find("iPad Pro 11-inch (M5)").unwrap();
        assert_eq!((p.width, p.height, p.scale, p.model()), (834.0, 1210.0, 2.0, "iPad17,2"));
        assert_ne!(p.orientations() & PortraitUpsideDown.mask(), 0, "iPads turn upside down");
        // An iPhone-only app sees a phone screen; one that lists the iPad sees the iPad's.
        assert_eq!(p.for_app(&[1]).width, 375.0);
        assert_eq!(p.for_app(&[1, 2]).id, "ipad-pro-11-m5");
        assert_eq!(find("iphone-17").unwrap().for_app(&[1]).id, "iphone-17");
        // The compatibility screens aren't devices of their own.
        assert!(find("ipad-compat-375").is_none());
        assert!(find("ipad-air-11-m4").unwrap().touch_id && !p.touch_id);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = DEVICES.iter().map(|d| d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), DEVICES.len());
    }
}
