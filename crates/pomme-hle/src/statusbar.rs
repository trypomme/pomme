//! The status bar: the time, the cellular signal, Wi-Fi (or the cellular data
//! network) and the battery, drawn over the app as iOS 26 draws them (see
//! render.rs). Each shows the emulated phone's state: its clock, the network
//! pomme models (`--network`), and its battery (`--battery`), which apps see
//! through UIDevice too. `--status-bar` overrides what's shown, as
//! `simctl status_bar override` does on the Simulator, for screenshots.
//!
//! Where each piece goes and how big it is was measured on the iOS 26.2
//! simulator, per screen layout (`simctl status_bar override --time 9:41
//! --cellularBars 4 --wifiBars 3 --batteryLevel 100`): the 2x and 3x phones
//! don't scale one layout, their glyphs snap to sizes of their own.

use crate::device::Device;
use crate::{Emu, HostFn, Result, objc};

/// Whether the phone is plugged in (UIDeviceBatteryState: unplugged 1,
/// charging 2, full 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charge {
    #[default]
    Unplugged,
    Charging,
    Full,
}

impl std::str::FromStr for Charge {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s.trim() {
            "unplugged" | "discharging" => Ok(Charge::Unplugged),
            "charging" => Ok(Charge::Charging),
            "full" | "charged" => Ok(Charge::Full),
            o => Err(format!("{o}: expected unplugged, charging or full")),
        }
    }
}

/// The battery: how full (0…1), whether it's plugged in, and whether Low
/// Power Mode is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Battery {
    pub level: f64,
    pub charge: Charge,
    pub low_power: bool,
}

impl Default for Battery {
    fn default() -> Self {
        Battery { level: 1.0, charge: Charge::Unplugged, low_power: false }
    }
}

impl Battery {
    /// This battery with `c`'s changes.
    pub fn with(self, c: BatteryChange) -> Battery {
        Battery { level: c.level.unwrap_or(self.level), charge: c.charge.unwrap_or(self.charge), low_power: c.low_power.unwrap_or(self.low_power) }
    }
}

/// A change to the battery (`--battery 20%`, `--battery charging`,
/// `--battery 80%,charging`, `--battery low-power`): what it names changes,
/// the rest stays.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BatteryChange {
    pub level: Option<f64>,
    pub charge: Option<Charge>,
    pub low_power: Option<bool>,
}

impl std::str::FromStr for BatteryChange {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let mut c = BatteryChange::default();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            if let Some(n) = part.strip_suffix('%').or_else(|| part.parse::<f64>().ok().map(|_| part)) {
                let v: f64 = n.trim().parse().map_err(|_| format!("{part}: expected a level like 80%"))?;
                if !(0.0..=100.0).contains(&v) {
                    return Err(format!("{part}: a level is 0% to 100%"));
                }
                c.level = Some(v / 100.0);
            } else if part == "low-power" {
                c.low_power = Some(true);
            } else if part == "low-power-off" {
                c.low_power = Some(false);
            } else {
                c.charge = Some(part.parse().map_err(|e: String| format!("{e}, low-power or low-power-off"))?);
            }
        }
        // Full means full.
        if c.charge == Some(Charge::Full) && c.level.is_none() {
            c.level = Some(1.0);
        }
        Ok(c)
    }
}

/// What the Wi-Fi slot shows: the Wi-Fi fan (with this many of its 3 bars
/// lit), the fan while it searches, the cellular data network ("5G"), or nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Wifi(u8),
    WifiSearching,
    Cellular(String),
    Hidden,
}

/// The cellular signal: this many of its 4 bars lit; searching (a wave
/// running along four dots); failed (no service: four dim dots); or none
/// (no cellular).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Signal {
    Bars(u8),
    Searching,
    Failed,
    Hidden,
}

/// What `--status-bar` pins, as `simctl status_bar override` does:
/// `time=9:41,battery=100,battery-state=charging,cellular-bars=4,wifi-bars=3,data-network=5g`,
/// `cellular-mode=active|searching|failed|not-supported`, `wifi-mode=active|searching|failed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// The time of day shown, in minutes (shown as the phone shows the time).
    pub time: Option<u32>,
    pub level: Option<f64>,
    pub charge: Option<Charge>,
    pub cellular_bars: Option<u8>,
    pub wifi_bars: Option<u8>,
    /// `wifi`, `hide`, or a cellular network's label.
    pub data_network: Option<Data>,
    /// The signal's mode (bars come from `cellular_bars`).
    pub cellular_mode: Option<Signal>,
    /// Wi-Fi's mode: searching, or failed (the fan, dim).
    pub wifi_mode: Option<Data>,
}

impl std::str::FromStr for Overrides {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let mut o = Overrides::default();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (k, v) = part.split_once('=').ok_or_else(|| format!("{part}: expected key=value"))?;
            let (k, v) = (k.trim(), v.trim());
            let bars = |max: u8| v.parse::<u8>().ok().filter(|&n| n <= max).ok_or_else(|| format!("{k}: expected 0 to {max}"));
            match k {
                "time" => o.time = Some(time_of_day(v).ok_or_else(|| format!("{v}: expected a time like 9:41 or 21:41, or an ISO date"))?),
                "battery" | "battery-level" => {
                    let n: f64 = v.trim_end_matches('%').parse().map_err(|_| format!("{k}: expected 0 to 100"))?;
                    if !(0.0..=100.0).contains(&n) {
                        return Err(format!("{k}: expected 0 to 100"));
                    }
                    o.level = Some(n / 100.0);
                }
                "battery-state" => o.charge = Some(v.parse()?),
                "cellular-bars" => o.cellular_bars = Some(bars(4)?),
                "cellular-mode" => {
                    o.cellular_mode = Some(match v {
                        "active" => Signal::Bars(4),
                        "searching" => Signal::Searching,
                        "failed" => Signal::Failed,
                        "not-supported" => Signal::Hidden,
                        m => return Err(format!("{m}: expected active, searching, failed or not-supported")),
                    })
                }
                "wifi-mode" => {
                    o.wifi_mode = Some(match v {
                        "active" => Data::Wifi(3),
                        "searching" => Data::WifiSearching,
                        "failed" => Data::Wifi(0),
                        m => return Err(format!("{m}: expected active, searching or failed")),
                    })
                }
                "wifi-bars" => o.wifi_bars = Some(bars(3)?),
                "data-network" => {
                    o.data_network = Some(match v {
                        "wifi" => Data::Wifi(3),
                        "hide" => Data::Hidden,
                        n => Data::Cellular(cellular_label(n).ok_or_else(|| format!("{n}: expected wifi, hide, 3g, 4g, lte, lte-a, lte+, 5g, 5g+, 5g-uwb or 5g-uc"))?.to_string()),
                    })
                }
                o => return Err(format!("{o}: expected time, battery, battery-state, cellular-mode, cellular-bars, wifi-mode, wifi-bars or data-network")),
            }
        }
        Ok(o)
    }
}

/// A time of day in minutes: `9:41`, `21:41`, or an ISO date's
/// (`2026-03-14T09:41:00Z`: its time as written).
fn time_of_day(s: &str) -> Option<u32> {
    let t = s.split_once('T').map_or(s, |(_, t)| t);
    let mut parts = t.split(':');
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.get(..2)?.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// How the status bar names a cellular data network (simctl's names).
fn cellular_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "3g" => "3G",
        "4g" => "4G",
        "lte" => "LTE",
        "lte-a" => "LTE-A",
        "lte+" => "LTE+",
        "5g" => "5G",
        "5g+" => "5G+",
        "5g-uwb" => "5G UW",
        "5g-uc" => "5G UC",
        _ => return None,
    })
}

#[derive(Clone, Default)]
pub struct State {
    pub battery: Battery,
    /// Scripted changes (`--battery 15%@4`): (virtual ns, the change).
    pub battery_changes: Vec<(u64, BatteryChange)>,
    pub overrides: Overrides,
    /// `UIDevice.batteryMonitoringEnabled`: until an app turns it on, it reads
    /// the battery as unknown, as on iOS.
    monitoring: bool,
}

/// What the status bar shows now.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub time: String,
    pub signal: Signal,
    pub data: Data,
    pub battery: Battery,
}

/// The status bar's contents: the phone's state, as overridden.
pub fn shown(emu: &Emu, layout: &Layout) -> Shown {
    use crate::network::Path;
    let o = &emu.status_bar.overrides;
    // On Wi-Fi a phone shows its signal and the Wi-Fi fan; on cellular, the
    // signal and the network's name ("5G"); wired, just the signal; offline,
    // no service.
    let (signal, data) = match emu.network.path {
        Path::Wifi => (Signal::Bars(4), Data::Wifi(3)),
        Path::Cellular => (Signal::Bars(4), Data::Cellular("5G".into())),
        Path::Wired => (Signal::Bars(4), Data::Hidden),
        Path::Offline => (Signal::Failed, Data::Hidden),
    };
    let mut data = o.wifi_mode.clone().or(o.data_network.clone()).unwrap_or(data);
    if let (Data::Wifi(n), Some(w)) = (&mut data, o.wifi_bars) {
        *n = w;
    }
    let mut signal = o.cellular_mode.unwrap_or(signal);
    if let (Signal::Bars(n), Some(b)) = (&mut signal, o.cellular_bars) {
        *n = b;
    }
    let time = match o.time {
        Some(minutes) => format_time(minutes as i64 * 60, layout.am_pm),
        None => clock_text(emu, layout.am_pm),
    };
    let b = emu.status_bar.battery;
    Shown {
        time,
        signal,
        data,
        battery: b.with(BatteryChange { level: o.level, charge: o.charge, low_power: None }),
    }
}

/// Searching's wave (measured from the simulator's frames, about 0.35 s
/// apart: it runs across in about 0.9 s and starts again every 1.05 s): how
/// far (0…1) each of `n` marks, first to last, is lit now.
pub fn wave(emu: &Emu, n: usize) -> Vec<f64> {
    wave_at(crate::threads::now_ns(emu) as f64 / 1e9, n)
}

fn wave_at(seconds: f64, n: usize) -> Vec<f64> {
    const PERIOD: f64 = 1.05;
    const ACROSS: f64 = 0.9;
    let t = seconds % PERIOD;
    let centre = -1.0 + (n as f64 + 2.0) * t / ACROSS;
    (0..n).map(|i| (1.0 - (i as f64 - centre).abs()).max(0.0)).collect()
}

/// Whether the status bar moves by itself (searching): the live window keeps drawing it.
pub fn animating(emu: &Emu) -> bool {
    let o = &emu.status_bar.overrides;
    o.cellular_mode == Some(Signal::Searching) || o.wifi_mode == Some(Data::WifiSearching)
}

/// The phone's wall clock, in seconds since 1970 in its time zone.
fn local_seconds(emu: &Emu) -> i64 {
    let t = (crate::libsystem::realtime_ns(emu) / 1_000_000_000) as i64;
    t + crate::calendar::Gregorian::device().zone.offset_at(t) as i64
}

/// The time as the status bar shows it: `9:41` (a 12-hour clock, as the
/// en-US locale pomme presents has), and `9:41 AM` on home-button phones.
pub fn clock_text(emu: &Emu, am_pm: bool) -> String {
    format_time(local_seconds(emu), am_pm)
}

fn format_time(local: i64, am_pm: bool) -> String {
    let minutes = local.div_euclid(60);
    let (h, m) = (minutes.div_euclid(60).rem_euclid(24), minutes.rem_euclid(60));
    let h12 = if h % 12 == 0 { 12 } else { h % 12 };
    if am_pm { format!("{h12}:{m:02} {}", if h < 12 { "AM" } else { "PM" }) } else { format!("{h12}:{m:02}") }
}

/// The minute the clock shows, and when (virtual ns) the next one starts:
/// the live window draws the status bar again then.
pub fn minute(emu: &Emu) -> (i64, u64) {
    let real = crate::libsystem::realtime_ns(emu);
    let into = real % 60_000_000_000;
    ((real / 60_000_000_000) as i64, crate::threads::now_ns(emu) + (60_000_000_000 - into))
}

// ---- layout ----

/// Where a screen layout's status bar puts things, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// The time: its centre, its baseline, and its digits' height.
    pub time: (f64, f64, f64),
    /// Home-button phones show AM / PM.
    pub am_pm: bool,
    /// The signal bars: the first's left edge, their bottom, width, pitch, heights.
    pub bars: (f64, f64, f64, f64, [f64; 4]),
    /// The Wi-Fi fan's box (left, top, right, bottom); a data network's name
    /// is centred in it.
    pub wifi: [f64; 4],
    /// The battery's body and its cap.
    pub battery: [f64; 4],
    pub cap: [f64; 4],
}

impl Layout {
    /// The same layout on a screen `dx` points wider: what's at the right
    /// edge moves with it, the centred time halfway.
    const fn wider(self, dx: f64) -> Layout {
        let [wx0, wy0, wx1, wy1] = self.wifi;
        let [bx0, by0, bx1, by1] = self.battery;
        let [cx0, cy0, cx1, cy1] = self.cap;
        Layout {
            time: (self.time.0 + dx / 2.0, self.time.1, self.time.2),
            battery: [bx0 + dx, by0, bx1 + dx, by1],
            cap: [cx0 + dx, cy0, cx1 + dx, cy1],
            wifi: if self.am_pm { self.wifi } else { [wx0 + dx, wy0, wx1 + dx, wy1] },
            ..self
        }
    }
}

const fn layout(time: (f64, f64, f64), bars: (f64, f64, f64, f64, [f64; 4]), wifi: [f64; 4], battery: [f64; 4], cap: [f64; 4]) -> Layout {
    Layout { time, am_pm: false, bars, wifi, battery, cap }
}

// Measured (the iOS 26.2 simulator; the phone each was measured on first).
/// iPhone 16e: 390 pt with the notch (and the 12, 13, 14 and their Pros, the 17e).
const NOTCH_390: Layout = layout((50.83, 30.33, 11.0), (302.33, 30.33, 3.0, 4.67, [4.0, 6.0, 8.67, 11.33]), [325.0, 18.33, 340.67, 30.33], [346.67, 18.67, 369.33, 30.67], [370.33, 23.0, 371.67, 26.33]);
/// iPhone 12 mini (and the 13 mini).
const NOTCH_360: Layout = layout((40.5, 31.33, 10.33), (285.67, 31.33, 3.0, 4.44, [3.67, 5.0, 7.67, 10.33]), [307.33, 20.67, 322.0, 31.33], [327.0, 21.0, 347.0, 31.67], [347.67, 24.67, 349.0, 28.0]);
/// iPhone 11 Pro (and the X and Xs).
const NOTCH_375: Layout = layout((45.67, 28.33, 10.67), (294.0, 28.33, 3.0, 4.67, [4.0, 6.0, 8.33, 10.67]), [316.0, 17.33, 331.33, 28.33], [336.33, 17.33, 358.33, 28.67], [359.33, 21.0, 360.67, 25.0]);
/// iPhone 11 (and the Xʀ): 2x.
const NOTCH_414_2X: Layout = layout((50.25, 30.5, 11.5), (326.5, 30.5, 3.5, 5.0, [4.5, 6.5, 9.0, 11.5]), [349.5, 18.5, 366.0, 30.5], [370.5, 18.5, 394.5, 31.0], [395.5, 23.0, 397.0, 26.5]);
/// iPhone 11 Pro Max (and the Xs Max).
const NOTCH_414_3X: Layout = layout((55.83, 28.33, 11.33), (319.0, 28.33, 3.33, 5.0, [4.33, 6.33, 9.0, 11.67]), [344.0, 16.33, 359.67, 28.33], [366.33, 16.33, 390.33, 28.67], [391.33, 20.33, 392.67, 24.67]);
/// iPhone 12 Pro Max (and the 13 Pro Max, 14 Plus).
const NOTCH_428: Layout = layout((61.33, 30.33, 12.0), (324.33, 30.33, 3.33, 5.33, [4.67, 7.0, 9.67, 12.33]), [350.0, 18.0, 367.0, 30.33], [373.33, 17.67, 398.33, 30.67], [399.33, 22.33, 400.67, 26.0]);
/// iPhone 15 (and the 14 Pro, 15 Pro, 16): 393 pt with the Dynamic Island.
const ISLAND_393: Layout = layout((70.33, 35.33, 12.0), (282.0, 35.33, 3.33, 5.33, [4.67, 7.0, 9.67, 12.33]), [308.67, 23.0, 325.67, 35.33], [333.0, 22.67, 358.0, 35.67], [359.0, 27.33, 360.33, 31.0]);
/// iPhone 17 (and the 16 Pro, 17 Pro).
const ISLAND_402: Layout = layout((72.67, 38.67, 12.0), (288.33, 38.67, 3.33, 5.33, [4.67, 7.0, 9.67, 12.33]), [315.0, 26.33, 332.0, 38.67], [339.33, 26.0, 364.33, 39.0], [365.33, 30.67, 366.67, 34.33]);
/// iPhone Air.
const ISLAND_420: Layout = layout((76.17, 45.0, 12.67), (298.33, 45.0, 3.67, 6.0, [5.33, 7.67, 10.67, 13.67]), [327.67, 31.0, 346.67, 45.0], [354.33, 31.0, 382.67, 45.33], [383.67, 36.0, 385.33, 40.33]);
/// iPhone 15 Plus (and the 14 Pro Max, 15 Pro Max, 16 Plus).
const ISLAND_430: Layout = layout((79.17, 35.67, 12.67), (305.33, 35.67, 3.67, 6.0, [5.33, 7.67, 10.67, 13.67]), [334.67, 21.67, 353.67, 35.67], [361.33, 21.67, 389.67, 36.0], [390.67, 26.67, 392.33, 31.0]);
/// iPhone 17 Pro Max (and the 16 Pro Max).
const ISLAND_440: Layout = layout((82.17, 39.0, 12.67), (312.33, 39.0, 3.67, 6.0, [5.33, 7.67, 10.67, 13.67]), [341.67, 25.0, 360.67, 39.0], [368.33, 25.0, 396.67, 39.33], [397.67, 30.0, 399.33, 34.33]);
/// iPhone SE (3rd generation; and the 8): the signal and Wi-Fi at the left,
/// the time (with AM / PM) centred, the battery at the right; 2x.
const HOME_375: Layout = Layout {
    time: (187.25, 14.5, 8.5),
    am_pm: true,
    bars: (6.5, 15.0, 3.0, 4.5, [4.0, 6.0, 8.5, 10.5]),
    wifi: [27.5, 4.5, 42.0, 15.0],
    battery: [345.5, 4.0, 367.5, 15.5],
    cap: [368.5, 8.0, 369.5, 11.5],
};

/// The status bar layout for a phone (by the screen it shares with the phone
/// it was measured on). None where none was measured: iPads (their status
/// bar has the date by the time, left-aligned) and the screens iPadOS gives
/// iPhone apps. Apps still see the measured status bar frame there.
pub fn layout_for(d: &Device) -> Option<Layout> {
    if d.idiom == crate::device::Idiom::Pad || d.id.starts_with("ipad-") {
        return None;
    }
    Some(match d.id {
        "iphone-se" | "iphone-8" => HOME_375,
        // Not measured (no iOS 26 simulator): the SE's, on its wider screen.
        "iphone-8-plus" => HOME_375.wider(d.width - 375.0),
        "iphone-x" | "iphone-xs" | "iphone-11-pro" => NOTCH_375,
        "iphone-xs-max" | "iphone-11-pro-max" => NOTCH_414_3X,
        "iphone-xr" | "iphone-11" => NOTCH_414_2X,
        "iphone-12-mini" | "iphone-13-mini" => NOTCH_360,
        "iphone-12-pro-max" | "iphone-13-pro-max" | "iphone-14-plus" => NOTCH_428,
        "iphone-14-pro" | "iphone-15" | "iphone-15-pro" | "iphone-16" => ISLAND_393,
        "iphone-14-pro-max" | "iphone-15-plus" | "iphone-15-pro-max" | "iphone-16-plus" => ISLAND_430,
        "iphone-16-pro" | "iphone-17" | "iphone-17-pro" => ISLAND_402,
        "iphone-16-pro-max" | "iphone-17-pro-max" => ISLAND_440,
        "iphone-air" => ISLAND_420,
        _ => NOTCH_390,
    })
}

// ---- the battery, as apps see it ----

/// Change the battery; apps monitoring it hear (UIDevice's notifications).
pub fn set_battery(emu: &mut Emu, c: BatteryChange) -> Result<()> {
    let before = emu.status_bar.battery;
    let after = before.with(c);
    emu.status_bar.battery = after;
    crate::host::frame_changed(emu);
    // Low Power Mode is ProcessInfo's, heard whether or not the battery's monitored.
    if after.low_power != before.low_power {
        let cls = objc::host_class(emu, "NSProcessInfo")?;
        let info = objc::send(emu, cls, "processInfo", &[])?;
        crate::foundation::post_named(emu, "NSProcessInfoPowerStateDidChangeNotification", info)?;
    }
    if !emu.status_bar.monitoring {
        return Ok(());
    }
    let device = emu.ui.views.device;
    if after.charge != before.charge {
        crate::foundation::post_named(emu, "UIDeviceBatteryStateDidChangeNotification", device)?;
    }
    if after.level != before.level {
        crate::foundation::post_named(emu, "UIDeviceBatteryLevelDidChangeNotification", device)?;
    }
    Ok(())
}

/// Put the scripted changes (`--battery 15%@4`) on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.status_bar.battery_changes.len() {
        let delay = emu.status_bar.battery_changes[i].0.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, |e| {
            let c = e.status_bar.battery_changes[e.arg(1) as usize].1;
            set_battery(e, c)
        }, i as u64)?;
    }
    Ok(())
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

const DEVICE: &str = "UIDevice";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSProcessInfo", false, "isLowPowerModeEnabled", |e| {
        let on = e.status_bar.battery.low_power as u64;
        ret(e, on)
    }),
    (DEVICE, false, "isBatteryMonitoringEnabled", |e| {
        let on = e.status_bar.monitoring as u64;
        ret(e, on)
    }),
    (DEVICE, false, "setBatteryMonitoringEnabled:", |e| {
        e.status_bar.monitoring = e.arg(2) & 0xff != 0;
        Ok(())
    }),
    // -1 and unknown until the app monitors the battery.
    (DEVICE, false, "batteryLevel", |e| {
        let level = if e.status_bar.monitoring { e.status_bar.battery.level as f32 } else { -1.0 };
        e.cpu.q[0] = level.to_bits() as u128;
        Ok(())
    }),
    (DEVICE, false, "batteryState", |e| {
        let state = if !e.status_bar.monitoring {
            0
        } else {
            match e.status_bar.battery.charge {
                Charge::Unplugged => 1,
                Charge::Charging => 2,
                Charge::Full => 3,
            }
        };
        ret(e, state)
    }),
];

/// Whether the status bar is hidden, and whether its content is light, as the
/// screen's controllers ask: the root view controller's (or one presented
/// full screen over it; or the child it defers to:
/// `childViewControllerForStatusBarHidden` / `…Style`; a navigation
/// controller's top controller while its bar is hidden) `prefersStatusBarHidden`
/// and `preferredStatusBarStyle`. Light content is white, dark content
/// black, and the default follows the appearance.
pub fn appearance(emu: &mut Emu) -> Result<(bool, bool)> {
    let Some(&window) = emu.ui.windows.last() else { return Ok((false, false)) };
    let root = objc::send(emu, window, "rootViewController", &[])?;
    if root == 0 {
        return Ok((false, false));
    }
    let root = crate::navigation::status_bar_root(emu, root);
    let deciding = |emu: &mut Emu, sel: &str| -> Result<u64> {
        let mut vc = root;
        for _ in 0..16 {
            let child = match crate::navigation::status_bar_child(emu, vc, sel.ends_with("Style")) {
                Some(c) => c,
                None => objc::send(emu, vc, sel, &[])?,
            };
            if child == 0 || child == vc {
                break;
            }
            vc = child;
        }
        Ok(vc)
    };
    let h = deciding(emu, "childViewControllerForStatusBarHidden")?;
    let hidden = objc::send(emu, h, "prefersStatusBarHidden", &[])? & 0xff != 0;
    let s = deciding(emu, "childViewControllerForStatusBarStyle")?;
    // UIStatusBarStyle: default (dark content in light mode, light in dark),
    // light content (1), dark content (3).
    let light = match objc::send(emu, s, "preferredStatusBarStyle", &[])? {
        1 => true,
        3 => false,
        _ => emu.appearance.dark,
    };
    Ok((hidden, light))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_on_the_status_bar() {
        assert_eq!(format_time(9 * 3600 + 41 * 60, false), "9:41");
        assert_eq!(format_time(0, false), "12:00");
        assert_eq!(format_time(13 * 3600 + 5 * 60, false), "1:05");
        assert_eq!(format_time(13 * 3600 + 5 * 60, true), "1:05 PM");
        assert_eq!(format_time(9 * 3600 + 41 * 60, true), "9:41 AM");
    }

    #[test]
    fn searching_runs_a_wave_along() {
        // A third of the way across, the second mark is lit; between runs, none.
        assert_eq!(wave_at(1.05 + 0.3, 4).iter().map(|k| (k * 100.0).round()).collect::<Vec<_>>(), [0.0, 100.0, 0.0, 0.0]);
        assert!(wave_at(1.0, 4).iter().all(|&k| k == 0.0));
    }

    #[test]
    fn battery_changes_parse() {
        assert_eq!("80%".parse(), Ok(BatteryChange { level: Some(0.8), ..Default::default() }));
        assert_eq!("20%,charging".parse(), Ok(BatteryChange { level: Some(0.2), charge: Some(Charge::Charging), low_power: None }));
        assert_eq!("full".parse(), Ok(BatteryChange { level: Some(1.0), charge: Some(Charge::Full), low_power: None }));
        assert_eq!("30%,low-power".parse(), Ok(BatteryChange { level: Some(0.3), charge: None, low_power: Some(true) }));
        assert!("120%".parse::<BatteryChange>().is_err());
        assert!("plugged".parse::<BatteryChange>().is_err());
    }

    #[test]
    fn overrides_parse() {
        let o: Overrides = "time=9:41,battery=100,battery-state=charging,cellular-bars=2,wifi-bars=1,data-network=lte".parse().unwrap();
        assert_eq!(o.time, Some(9 * 60 + 41));
        assert_eq!("time=21:05".parse::<Overrides>().unwrap().time, Some(21 * 60 + 5));
        assert_eq!("time=2026-03-14T09:41:00Z".parse::<Overrides>().unwrap().time, Some(9 * 60 + 41));
        assert!("time=noon".parse::<Overrides>().is_err());
        assert_eq!((o.level, o.charge, o.cellular_bars, o.wifi_bars), (Some(1.0), Some(Charge::Charging), Some(2), Some(1)));
        assert_eq!(o.data_network, Some(Data::Cellular("LTE".into())));
        assert!("wifi-bars=4".parse::<Overrides>().is_err());
        let o: Overrides = "cellular-mode=searching,wifi-mode=failed".parse().unwrap();
        assert_eq!((o.cellular_mode, o.wifi_mode), (Some(Signal::Searching), Some(Data::Wifi(0))));
    }

    #[test]
    fn every_phone_has_a_measured_layout() {
        for d in crate::device::DEVICES.iter().filter(|d| d.idiom == crate::device::Idiom::Phone) {
            let l = layout_for(d).unwrap();
            assert!(l.cap[2] < d.width && l.time.0 > 0.0, "{}", d.id);
            if d.id != "iphone-16e" && d.id != "iphone-17e" && !d.id.starts_with("iphone-12") && !d.id.starts_with("iphone-13") && !d.id.starts_with("iphone-14") {
                assert_ne!(l, NOTCH_390, "{} falls back to the 16e's layout", d.id);
            }
        }
    }
}
