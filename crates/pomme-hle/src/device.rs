//! The device pomme presents: an iPhone model's screen (size in points,
//! scale), its safe area, its sensor housing (a notch or the Dynamic Island,
//! or none on home-button phones), the screen's corner radius and the
//! keyboard's height.
//!
//! Sources: screen sizes and scales are from Xcode 26's simulator device
//! profiles (`/Library/Developer/CoreSimulator/Profiles/DeviceTypes`), notch
//! sizes from their sensor-housing art. The iPhone 17's safe area, Dynamic
//! Island and keyboard were measured in the iOS 26 Simulator; the other safe
//! areas, islands and corner radii are Apple's published values. Keyboard
//! heights elsewhere are the iPhone 17's keys plus the device's bottom inset.

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

#[derive(Clone, Copy, Debug)]
pub struct Device {
    /// What `--device` takes: `iphone-17-pro-max`.
    pub id: &'static str,
    pub name: &'static str,
    /// The screen in points, and pixels per point.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    /// Portrait safe area: the status bar / sensor housing, and the home indicator.
    pub safe_top: f64,
    pub safe_bottom: f64,
    pub cutout: Cutout,
    /// The display's rounded corners (0: square).
    pub corner_radius: f64,
    pub home_button: bool,
}

/// The keys of the iOS 26 keyboard (portrait, English) above the home
/// indicator, with and without the predictive bar: measured on an iPhone 17
/// (335 and 304 pt overall, 34 pt of which is the bottom inset).
const KEYS_WITH_PREDICTIONS: f64 = 301.0;
const KEYS_WITHOUT_PREDICTIONS: f64 = 270.0;

impl Device {
    /// The keyboard's height on this device.
    pub fn keyboard_height(&self, predictive: bool) -> f64 {
        (if predictive { KEYS_WITH_PREDICTIONS } else { KEYS_WITHOUT_PREDICTIONS }) + self.safe_bottom
    }

    /// Safe-area insets as UIEdgeInsets (top, left, bottom, right).
    pub fn safe_area(&self) -> [f64; 4] {
        [self.safe_top, 0.0, self.safe_bottom, 0.0]
    }
}

const fn island(top: f64) -> Cutout {
    Cutout::Island { top, width: 126.0, height: 37.33 }
}

const fn phone(id: &'static str, name: &'static str, size: (f64, f64), scale: f64, safe: (f64, f64), cutout: Cutout, corner_radius: f64) -> Device {
    Device { id, name, width: size.0, height: size.1, scale, safe_top: safe.0, safe_bottom: safe.1, cutout, corner_radius, home_button: false }
}

const fn home_button(id: &'static str, name: &'static str, size: (f64, f64), scale: f64) -> Device {
    Device { id, name, width: size.0, height: size.1, scale, safe_top: 20.0, safe_bottom: 0.0, cutout: Cutout::None, corner_radius: 0.0, home_button: true }
}

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
    phone("iphone-air", "iPhone Air", (420.0, 912.0), 3.0, (68.0, 34.0), island(14.0), 62.0),
];

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
        // The iPhone 17's, measured: 335 pt with the predictive bar, 304 without.
        let d = find("iphone-17").unwrap();
        assert_eq!(d.keyboard_height(true), 335.0);
        assert_eq!(d.keyboard_height(false), 304.0);
        // No home indicator: no bottom inset under the keys.
        assert_eq!(find("iphone-se").unwrap().keyboard_height(true), 301.0);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = DEVICES.iter().map(|d| d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), DEVICES.len());
    }
}
