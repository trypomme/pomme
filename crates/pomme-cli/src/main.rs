use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pomme_macho::{DylibKind, Image, LibOrdinal};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "pomme", about = "An emulator for running iOS apps elsewhere")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inspect a .app bundle or .ipa (or bare Mach-O) and report what it needs from iOS.
    Inspect {
        path: PathBuf,
        /// List every imported symbol, not just per-library counts.
        #[arg(long)]
        symbols: bool,
    },
    /// Load a .app (or .ipa) and run it: main() on the arm64 interpreter, system libraries in HLE.
    Run(RunArgs),
    /// How much of iOS the app needs and how much of it pomme provides: every system
    /// symbol it binds (implemented, data, or missing), then a run (taking `run`'s
    /// options) that records every missing function and method the app calls, and
    /// the messages pomme answers with a generic stand-in instead of iOS's behaviour.
    Coverage {
        #[command(flatten)]
        coverage: CoverageArgs,
        #[command(flatten)]
        run: RunArgs,
    },
    /// Pack a .app (or .ipa) and the sysroot files it loads into one bundle, for the browser host.
    Bundle {
        path: PathBuf,
        /// Where to write the bundle.
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value_os_t = default_sysroot())]
        sysroot: PathBuf,
        /// Languages whose fonts to include besides the app's own localizations'
        /// (the Chinese, Japanese and Korean fonts are large, so a bundle has
        /// them only when needed): `--fonts ja,zh-Hans`.
        #[arg(long, value_delimiter = ',')]
        fonts: Vec<String>,
    },
    /// Load and link a .app (or .ipa) into guest memory, and report the result.
    Load {
        path: PathBuf,
        /// List every HLE stub that was bound.
        #[arg(long)]
        stubs: bool,
    },
}

#[derive(clap::Args)]
struct RunArgs {
    path: PathBuf,
    /// Stop after this many instructions.
    #[arg(long, default_value_t = 50_000_000)]
    steps: u64,
    /// Stop once the app is idle past this many seconds of virtual time.
    #[arg(long, default_value_t = 10.0)]
    seconds: f64,
    /// Print the view hierarchy when the run stops.
    #[arg(long)]
    views: bool,
    /// Print what accessibility sees when the run stops: identifiers
    /// (React Native's testID, SwiftUI's and Flutter's), labels, values
    /// and frames, as UI tests find elements by them.
    #[arg(long)]
    elements: bool,
    /// Tap at a point (in points) at a virtual time: `--tap 195,438@2` (repeatable).
    #[arg(long, value_parser = parse_tap)]
    tap: Vec<(f64, f64, f64)>,
    /// Swipe from one point to another at a virtual time: `--swipe 195,700>195,200@2`
    /// (0.3 s; `~0.1` after the time for a faster fling). Scroll views scroll and keep
    /// going after a fast swipe (repeatable).
    #[arg(long, value_parser = parse_swipe)]
    swipe: Vec<Swipe>,
    /// Write a PNG of the screen (at the device's scale) when the run stops.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// The iPhone to present: its screen, safe area, notch or Dynamic Island and keyboard
    /// (`iphone-17-pro-max`, `"iPhone SE (3rd generation)"`; `--device list` lists them).
    #[arg(long, default_value = pomme_hle::device::DEFAULT, value_parser = parse_device)]
    device: &'static pomme_hle::device::Device,
    /// Draw the screenshot inside a phone body (rounded screen corners, bezel, buttons).
    #[arg(long)]
    shell: bool,
    /// Print every system call and Objective-C message as it happens.
    #[arg(long)]
    trace: bool,
    /// Directory of pomme-built system libraries (see scripts/build-sysroot.sh).
    /// Defaults to `vendor/sysroot` in a checkout, else the `sysroot` folder
    /// next to the executable (as the release downloads ship it).
    #[arg(long, default_value_os_t = default_sysroot())]
    sysroot: PathBuf,
    /// Run from a bundle made by `pomme bundle` instead of the disk (PATH is then ignored).
    #[arg(long)]
    bundle: Option<PathBuf>,
    /// No network: every request fails as if the device were offline.
    #[arg(long)]
    offline: bool,
    /// Answer requests for URL (or URL prefix ending in `*`) with FILE: `--fixture 'https://api.x/*=x.json'`.
    #[arg(long, value_parser = parse_fixture)]
    fixture: Vec<(String, PathBuf)>,
    /// The iOS version to present (`18.6`, `26`, ...). The app's own SDK and Info.plist
    /// decide the rest: e.g. Liquid Glass needs iOS 26 and an app linked against the 26 SDK.
    #[arg(long, default_value_t = pomme_hle::system::LATEST)]
    ios: pomme_hle::system::Version,
    /// List the system functions the app binds that pomme doesn't implement yet, and exit.
    #[arg(long)]
    missing: bool,
    /// Launch the app with this URL (a deep link), as iOS does: `--url 'myapp:///settings'`.
    #[arg(long)]
    url: Option<String>,
    /// Type on the software keyboard at a virtual time: `--type 'Ada\n@4'` (`\n` is
    /// return, `\b` delete). Keys go to the first responder (tap a text field first).
    #[arg(long = "type", value_parser = parse_typing)]
    typing: Vec<(String, f64)>,
    #[command(flatten)]
    clipboard: ClipboardArgs,
    #[command(flatten)]
    state: DeviceStateArgs,
    #[command(flatten)]
    profile: ProfileArgs,
    /// Run live in a window that looks like the iOS Simulator: click and drag to
    /// touch, type into the focused field, ⌘← / ⌘→ (Ctrl on Windows and Linux)
    /// to turn the phone. Runs until the window closes (no step or time limit).
    #[arg(long)]
    window: bool,
    /// With --window: print, for each second frames arrive, how many there
    /// were (in wall time and in the app's own time) and the longest wait
    /// between consecutive frames of motion, to see where it stutters.
    #[arg(long, requires = "window")]
    frame_stats: bool,
}

#[derive(clap::Args, Clone)]
struct CoverageArgs {
    /// Only what's bound: go as far as dyld does (the initializers), not into main.
    #[arg(long)]
    no_run: bool,
    /// Stop at the first missing function or method, as `run` does, instead of
    /// answering it with 0 (nil) and carrying on to find the rest.
    #[arg(long)]
    stop_at_first: bool,
    /// List every missing symbol, not just the first few of each library.
    #[arg(long)]
    all: bool,
    /// Also write the report as JSON (always complete).
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
}

#[derive(clap::Args)]
/// How the phone is held, connected and set up.
struct DeviceStateArgs {
    /// How the phone is held at launch: `portrait` (the default), `landscape` (turned
    /// to the left: the island on the left; = `landscape-right`, as UIKit names the
    /// interface then), `landscape-left` or `portrait-upside-down`. The interface
    /// follows if the app supports it.
    #[arg(long = "orientation", default_value = "portrait")]
    held: pomme_hle::device::Orientation,
    /// Turn the phone at a virtual time: `--rotate landscape@3` (repeatable). Apps
    /// hear it as on a real iPhone: device-orientation notifications, and the
    /// interface turns if they support the new orientation.
    #[arg(long, value_parser = parse_rotate)]
    rotate: Vec<(pomme_hle::device::Orientation, f64)>,
    /// How the phone is connected: `wifi` (the default), `cellular`, `wired` or `offline`,
    /// and changes while it runs: `--network cellular --network offline@3`. Apps see it
    /// through the Network framework (NWPathMonitor, expo-network …); offline, requests fail.
    #[arg(long, value_parser = parse_network)]
    network: Vec<(pomme_hle::network::Path, Option<f64>)>,
    /// The battery: a level, `charging`, `full` or `unplugged` (the default: full,
    /// unplugged), `low-power` (Low Power Mode; `low-power-off`), or several
    /// (`--battery 20%,charging`), and changes while it runs:
    /// `--battery 15%@3`. The status bar shows it; apps read it through UIDevice
    /// (batteryLevel, batteryState and their notifications, once they monitor it).
    /// Light (the default) or dark, as Settings ▸ Display sets it, and changes while
    /// it runs: `--appearance dark@3`. System colours follow, apps hear
    /// `traitCollectionDidChange:`, and the status bar's default style turns white.
    #[arg(long, value_parser = parse_appearance)]
    appearance: Vec<(bool, Option<f64>)>,
    #[arg(long, value_parser = parse_battery)]
    battery: Vec<(pomme_hle::statusbar::BatteryChange, Option<f64>)>,
    /// Pin what the status bar shows, as `simctl status_bar override` does, for
    /// screenshots: `--status-bar time=9:41,battery=100,cellular-bars=4,wifi-bars=3`
    /// (also `battery-state=charging|charged|discharging` and
    /// `data-network=wifi|hide|lte|5g|…`). Apps don't see it.
    #[arg(long = "status-bar")]
    status_bar: Option<pomme_hle::statusbar::Overrides>,
    /// Face ID / Touch ID, as the Simulator's Features menu sets it: `off` (not enrolled,
    /// the default) or `enrolled` (each prompt waits for a face: from `--face`, or ⌥⌘M /
    /// ⌥⌘N in the window). `match` and `no-match` are enrolled with every prompt
    /// answered at once by a matching face, or one that doesn't match.
    #[arg(long, default_value = "off")]
    biometry: pomme_hle::keychain::Biometry,
    /// Present a face at a virtual time: `--face match@3`, `--face no-match@5`, or
    /// `--face toggle-enrolled@2` (repeatable). A face answers the prompts waiting then.
    #[arg(long, value_parser = parse_face)]
    face: Vec<(pomme_hle::keychain::Face, f64)>,
    /// A preferred language, as Settings ▸ General ▸ Language & Region sets it:
    /// `--language ja-JP` (repeatable: the first is the one the app shows, the
    /// others its fallbacks). The default is `en-US`.
    #[arg(long = "language", value_parser = parse_language)]
    languages: Vec<String>,
    /// The region, which sets the formats of dates, numbers, money and units:
    /// `--region AU`. The default is `US`.
    #[arg(long, value_parser = parse_region)]
    region: Option<String>,
    /// The phone's time zone, an IANA name: `--time-zone Australia/Sydney`. The default is GMT.
    #[arg(long = "time-zone", value_parser = parse_time_zone)]
    time_zone: Option<String>,
    /// Settings' 24-Hour Time switch, moved away from the region's choice:
    /// `--24-hour on` (24 hours everywhere) or `--24-hour off` (12 hours everywhere).
    #[arg(long = "24-hour", value_parser = parse_on_off)]
    force_24_hour: Option<bool>,
    /// The phone's clock at launch: `now`, a date (`2026-03-14` or
    /// `2026-03-14T09:30:00Z`, UTC) or seconds since 1970. By default it's `now` when
    /// the network is live (TLS certificates and sign-in tokens are checked against
    /// it), and 2026-01-01T00:00:00Z with `--offline`, the same every run.
    #[arg(long, value_parser = parse_clock)]
    clock: Option<Clock>,
    /// An environment variable for the app, as an Xcode scheme sets them:
    /// `--env GRPC_VERBOSITY=debug` (repeatable).
    #[arg(long = "env", value_parser = parse_env)]
    env: Vec<(String, String)>,
    /// Print each HTTP request the app makes, with its status and size.
    #[arg(long)]
    log_requests: bool,
    /// Print the app's debug-level log messages too (os_log's debug type), as
    /// Xcode's console does with "Include Debug Messages".
    #[arg(long)]
    debug_logs: bool,
    /// Launch arguments, after `--`: `pomme run App.app -- -FIRDebugEnabled -AppleLanguages '(fr)'`.
    /// The app sees them in `argv` and `ProcessInfo.arguments`, and `-key value` pairs
    /// set user defaults (NSArgumentDomain), as an Xcode scheme's arguments do.
    #[arg(last = true)]
    args: Vec<String>,
}

/// A BCP 47 language tag: `ja`, `en-AU`, `zh-Hans-CN`.
fn parse_language(s: &str) -> std::result::Result<String, String> {
    let ok = !s.is_empty() && s.split(['-', '_']).all(|p| !p.is_empty() && p.len() <= 8 && p.chars().all(|c| c.is_ascii_alphanumeric()));
    if ok { Ok(s.replace('_', "-")) } else { Err(format!("{s}: expected a language like ja-JP or en-AU")) }
}

/// A region code: two letters (`AU`) or three digits (`419`).
fn parse_region(s: &str) -> std::result::Result<String, String> {
    let ok = (s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic())) || (s.len() == 3 && s.chars().all(|c| c.is_ascii_digit()));
    if ok { Ok(s.to_uppercase()) } else { Err(format!("{s}: expected a region like AU or JP")) }
}

fn parse_time_zone(s: &str) -> std::result::Result<String, String> {
    if pomme_hle::locale::is_time_zone(s) { Ok(s.to_string()) } else { Err(format!("{s}: not a time zone (expected an IANA name like Australia/Sydney)")) }
}

fn parse_on_off(s: &str) -> std::result::Result<bool, String> {
    match s {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("{s}: expected on or off")),
    }
}

fn parse_env(s: &str) -> std::result::Result<(String, String), String> {
    let (k, v) = s.split_once('=').ok_or_else(|| format!("{s}: expected NAME=VALUE"))?;
    Ok((k.into(), v.into()))
}

#[derive(Clone, Copy, Debug)]
enum Clock {
    Now,
    At(u64),
}

/// `now`, seconds since 1970, or a UTC date: `YYYY-MM-DD` with an optional `THH:MM[:SS]` and `Z`.
fn parse_clock(s: &str) -> std::result::Result<Clock, String> {
    if s == "now" {
        return Ok(Clock::Now);
    }
    if let Ok(secs) = s.parse::<u64>() {
        return Ok(Clock::At(secs));
    }
    let bad = || format!("{s}: expected now, seconds since 1970, or YYYY-MM-DD[THH:MM[:SS]][Z]");
    let s2 = s.strip_suffix('Z').unwrap_or(s);
    let (date, time) = s2.split_once('T').unwrap_or((s2, "00:00"));
    let d: Vec<i64> = date.split('-').map(|x| x.parse().map_err(|_| bad())).collect::<std::result::Result<_, _>>()?;
    let t: Vec<i64> = time.split(':').map(|x| x.parse().map_err(|_| bad())).collect::<std::result::Result<_, _>>()?;
    let [y, m, day] = d[..] else { return Err(bad()) };
    let [h, min, sec] = match t[..] {
        [h, m] => [h, m, 0],
        [h, m, s] => [h, m, s],
        _ => return Err(bad()),
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || !(0..24).contains(&h) || !(0..60).contains(&min) || !(0..61).contains(&sec) || y < 1970 {
        return Err(bad());
    }
    // Days since 1970 (Howard Hinnant's days_from_civil).
    let yy = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (yy.div_euclid(400), yy.rem_euclid(400));
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Ok(Clock::At((days * 86_400 + h * 3600 + min * 60 + sec) as u64))
}

impl DeviceStateArgs {
    fn apply(&self, emu: &mut pomme_hle::Emu) {
        emu.ui.device_orientation = self.held;
        emu.ui.rotations = self.rotate.iter().map(|&(o, t)| ((t * 1e9) as u64, o)).collect();
        emu.keychain.biometry = self.biometry;
        emu.keychain.faces = self.face.iter().map(|&(f, t)| ((t * 1e9) as u64, f)).collect();
        emu.keychain.faces.sort_by_key(|f| f.0);
        match self.clock {
            Some(Clock::Now) => {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                pomme_hle::libsystem::set_boot_time(emu, now);
            }
            Some(Clock::At(secs)) => pomme_hle::libsystem::set_boot_time(emu, secs),
            None => {}
        }
        for (k, v) in &self.env {
            pomme_hle::libsystem::set_env(emu, k, v);
        }
        let settings = &mut emu.locales.settings;
        if !self.languages.is_empty() {
            settings.languages = self.languages.clone();
        }
        if let Some(r) = &self.region {
            settings.region = r.clone();
        }
        settings.time_zone = self.time_zone.clone().or(settings.time_zone.take());
        settings.force_24_hour = self.force_24_hour.or(settings.force_24_hour);
        emu.launch_args = self.args.clone();
        emu.os_log_debug = self.debug_logs;
        emu.net.log_requests = self.log_requests;
        for &(p, at) in &self.network {
            match at {
                None => emu.network.path = p,
                Some(t) => emu.network.changes.push(((t * 1e9) as u64, p)),
            }
        }
        for &(c, at) in &self.battery {
            match at {
                None => {
                    emu.status_bar.battery = emu.status_bar.battery.with(c);
                }
                Some(t) => emu.status_bar.battery_changes.push(((t * 1e9) as u64, c)),
            }
        }
        for &(dark, at) in &self.appearance {
            match at {
                None => emu.appearance.dark = dark,
                Some(t) => emu.appearance.changes.push(((t * 1e9) as u64, dark)),
            }
        }
        if let Some(o) = &self.status_bar {
            emu.status_bar.overrides = o.clone();
        }
    }
}

/// `cellular` (at launch) or `offline@3` (a change at 3 s).
fn parse_network(s: &str) -> std::result::Result<(pomme_hle::network::Path, Option<f64>), String> {
    match s.rsplit_once('@') {
        Some((p, t)) => Ok((p.parse()?, Some(t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?))),
        None => Ok((s.parse()?, None)),
    }
}

fn parse_appearance(s: &str) -> std::result::Result<(bool, Option<f64>), String> {
    let (a, at) = match s.rsplit_once('@') {
        Some((a, t)) => (a, Some(t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?)),
        None => (s, None),
    };
    match a.trim() {
        "light" => Ok((false, at)),
        "dark" => Ok((true, at)),
        o => Err(format!("{o}: expected light or dark")),
    }
}

fn parse_battery(s: &str) -> std::result::Result<(pomme_hle::statusbar::BatteryChange, Option<f64>), String> {
    match s.rsplit_once('@') {
        Some((c, t)) => Ok((c.parse()?, Some(t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?))),
        None => Ok((s.parse()?, None)),
    }
}

fn parse_face(s: &str) -> std::result::Result<(pomme_hle::keychain::Face, f64), String> {
    let (f, t) = s.rsplit_once('@').ok_or("expected face@seconds")?;
    Ok((f.parse()?, t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?))
}

fn parse_rotate(s: &str) -> std::result::Result<(pomme_hle::device::Orientation, f64), String> {
    let (o, t) = s.rsplit_once('@').ok_or("expected orientation@seconds")?;
    Ok((o.parse()?, t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?))
}

#[derive(clap::Args)]
/// Where the app's time goes.
struct ProfileArgs {
    /// Profile the app's code and write the profile here when the run stops, for
    /// https://www.speedscope.app (flame chart, left-heavy and sandwich views; one
    /// profile per thread). Time is in instructions, so the same run profiles the
    /// same way every time. The functions with the most instructions are printed too.
    #[arg(long, value_name = "FILE")]
    profile: Option<PathBuf>,
    /// Instructions between profile samples.
    #[arg(long, default_value_t = 10_000, value_name = "N")]
    profile_every: u64,
}

#[derive(clap::Args)]
struct ClipboardArgs {
    /// What the clipboard starts with: `empty` (the default), `host` (this machine's
    /// clipboard, live both ways: the app pastes what you copied and its copies land
    /// here), or some text (`text:…` for text that would read as one of those words).
    #[arg(long = "clipboard", default_value = "empty")]
    source: String,
    /// Paste into whatever has focus at a virtual time, as if chosen from the edit menu:
    /// `--paste 'Ada@4'` copies "Ada" (as another app would) and pastes it; `--paste '@4'`
    /// pastes what's on the clipboard. `\n` is a newline.
    #[arg(long, value_parser = parse_paste)]
    paste: Vec<(Option<String>, f64)>,
    /// How iOS's "Allow Paste" prompt is answered when the app reads another app's text
    /// without the user pasting: `allow` (the default) or `deny`.
    #[arg(long = "paste-permission", default_value = "allow")]
    permission: pomme_hle::clipboard::Permission,
}

/// This machine's clipboard.
struct HostClipboard(arboard::Clipboard);

impl pomme_hle::clipboard::Backend for HostClipboard {
    fn read(&mut self) -> Option<String> {
        self.0.get_text().ok()
    }
    fn write(&mut self, text: Option<&str>) {
        let _ = match text {
            Some(t) => self.0.set_text(t),
            None => self.0.clear(),
        };
    }
    /// The same clipboard: it's this machine's, not the phone's.
    fn duplicate(&self) -> Box<dyn pomme_hle::clipboard::Backend> {
        match arboard::Clipboard::new() {
            Ok(c) => Box::new(HostClipboard(c)),
            Err(_) => Box::new(pomme_hle::clipboard::Memory(None)),
        }
    }
}

impl ClipboardArgs {
    fn apply(&self, emu: &mut pomme_hle::Emu) -> Result<()> {
        use pomme_hle::clipboard::{Memory, PasteInput};
        let c = &mut emu.clipboard;
        c.backend = match self.source.as_str() {
            "empty" => Box::new(Memory(None)),
            "host" => Box::new(HostClipboard(arboard::Clipboard::new().context("opening this machine's clipboard")?)),
            t => Box::new(Memory(Some(pomme_hle::textinput::unescape(t.strip_prefix("text:").unwrap_or(t))))),
        };
        c.permission = self.permission;
        c.script = self.paste.iter().map(|(text, t)| PasteInput { at_ns: (t * 1e9) as u64, text: text.clone() }).collect();
        Ok(())
    }

    /// After the run: what the app left on the clipboard (not echoed for the host's).
    fn report(&self, emu: &mut pomme_hle::Emu) {
        let prompts = emu.clipboard.prompts;
        if self.source == "host" {
            if prompts > 0 {
                println!("
clipboard: {prompts} \"Allow Paste\" prompt(s), answered {:?}", self.permission);
            }
            return;
        }
        let text = emu.clipboard.text();
        if text.is_some() || prompts > 0 || !self.paste.is_empty() {
            println!("
clipboard: {text:?}; {prompts} \"Allow Paste\" prompt(s)");
        }
    }
}

fn parse_paste(s: &str) -> std::result::Result<(Option<String>, f64), String> {
    let (text, t) = s.rsplit_once('@').ok_or("expected text@seconds or @seconds")?;
    let t = t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?;
    Ok(((!text.is_empty()).then(|| pomme_hle::textinput::unescape(text)), t))
}

fn main() -> Result<()> {
    // An .ipa becomes the .app inside it (unpacked once into a cache folder).
    let app = |path: PathBuf| pomme_loader::ipa::open_app(&path).map_err(anyhow::Error::from);
    match Cli::parse().cmd {
        Cmd::Inspect { path, symbols } => inspect(&app(path)?, symbols),
        Cmd::Load { path, stubs } => load(&app(path)?, stubs),
        Cmd::Bundle { path, out, sysroot, fonts } => make_bundle(&app(path)?, &sysroot, &out, &fonts),
        Cmd::Run(mut args) => {
            args.path = app(args.path)?;
            run_command(args, None)
        }
        Cmd::Coverage { coverage, mut run } => {
            run.path = app(run.path)?;
            run_command(run, Some(coverage))
        }
    }
}

/// The sysroot to use when `--sysroot` isn't given: `vendor/sysroot` when run
/// from a checkout (`cargo run`), else the `sysroot` folder that the release
/// downloads put next to the executable (or in `../Resources` in a macOS app).
fn default_sysroot() -> PathBuf {
    let checkout = PathBuf::from("vendor/sysroot");
    if checkout.is_dir() {
        return checkout;
    }
    let beside_exe = std::env::current_exe().ok().and_then(|exe| {
        let dir = exe.parent()?;
        [dir.join("sysroot"), dir.join("../Resources/sysroot")].into_iter().find(|d| d.is_dir())
    });
    beside_exe.unwrap_or(checkout)
}

/// `run` and `coverage`: set up the network and the window (if any), then [`run`].
fn run_command(args: RunArgs, coverage: Option<CoverageArgs>) -> Result<()> {
    let RunArgs { path, steps, seconds, views, elements, tap, swipe, screenshot, device, shell, trace, sysroot, bundle, offline, fixture, ios, missing, url, typing, clipboard, mut state, profile, window, frame_stats } = args;
    // A live network talks to servers that check the date: use the real one.
    if state.clock.is_none() && !offline {
        state.clock = Some(Clock::Now);
    }
    let (path, sysroot) = match bundle {
        Some(b) => {
            let bytes = std::fs::read(&b).with_context(|| format!("reading {}", b.display()))?;
            pomme_loader::vfs::mount(&bytes).map_err(anyhow::Error::msg)?;
            let app = bundle_app_path(&bytes).context("the bundle has no .app")?;
            (PathBuf::from(app), PathBuf::from(BUNDLE_SYSROOT))
        }
        None => (path, sysroot),
    };
    if window {
        // The window shows the screen the app sees: an iPad's compatibility
        // screen for an iPhone-only app (drawn in iPadOS's window: not yet).
        let info = pomme_loader::vfs::read(&path.join("Info.plist")).ok().and_then(|b| plist::Value::from_reader(std::io::Cursor::new(b)).ok());
        let device = device.for_app(&pomme_hle::device::app_families(info.as_ref()));
        let config = pomme_desktop::Config { device, orientation: state.held, title: format!("{} — iOS {ios}", device.name), enrolled: state.biometry != pomme_hle::keychain::Biometry::NotEnrolled, frame_stats };
        let result = std::sync::Arc::new(std::sync::Mutex::new(Ok(())));
        let out = result.clone();
        pomme_desktop::launch(config, move |host, scale| {
            let r = make_net(offline, &fixture).and_then(|net| {
                run(&path, steps, seconds, views, elements, &tap, &swipe, screenshot.as_deref(), device, shell, trace, &sysroot, net, ios, missing, url, &typing, &clipboard, &state, &profile, coverage.as_ref(), Some((host, scale)))
            });
            *out.lock().unwrap() = r;
        })
        .map_err(anyhow::Error::msg)?;
        return std::mem::replace(&mut *result.lock().unwrap(), Ok(()));
    }
    let net = make_net(offline, &fixture)?;
    run(&path, steps, seconds, views, elements, &tap, &swipe, screenshot.as_deref(), device, shell, trace, &sysroot, net, ios, missing, url, &typing, &clipboard, &state, &profile, coverage.as_ref(), None)
}

/// `--device`: an id or name from pomme's device table; `list` prints the table.
fn parse_device(s: &str) -> Result<&'static pomme_hle::device::Device, String> {
    use pomme_hle::device::{find, DEVICES};
    let table = || DEVICES.iter().map(|d| format!("  {:<20} {:<28} {}×{} pt, @{}x", d.id, d.name, d.width, d.height, d.scale)).collect::<Vec<_>>().join("\n");
    if s == "list" {
        println!("{}", table());
        std::process::exit(0);
    }
    find(s).ok_or_else(|| format!("no device {s:?}; one of:\n{}", table()))
}

/// Where a bundle puts things.
const BUNDLE_APP_DIR: &str = "/app";
const BUNDLE_SYSROOT: &str = "/sysroot";

/// The `.app` directory inside a bundle (first path under /app/).
fn bundle_app_path(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let start = s.find("/app/")?;
    let rest = &s[start..];
    let end = rest.find(".app")? + 4;
    Some(rest[..end].to_string())
}

/// Pack the app and the sysroot files it loads (plus fonts) into a vfs bundle.
fn make_bundle(path: &Path, sysroot: &Path, out: &Path, languages: &[String]) -> Result<()> {
    use pomme_loader::vfs::{Entry, write_bundle};
    let sysroot = sysroot.canonicalize().context("no sysroot (run scripts/build-sysroot.sh)")?;
    let (_, _, app_dir) = pomme_loader::locate_executable(path)?;
    let app_dir = app_dir.canonicalize()?;
    let app_name = app_dir.file_name().context("app path has no name")?.to_string_lossy().into_owned();
    let app_root = format!("{BUNDLE_APP_DIR}/{app_name}");

    // Which sysroot files the app actually loads.
    let p = pomme_loader::load_with(&app_dir, &pomme_loader::LoadOptions { sysroot: Some(sysroot.clone()) })?;
    let loaded: BTreeSet<PathBuf> = p.images.iter().filter(|i| i.path.starts_with(&sysroot)).map(|i| i.path.clone()).collect();

    let map = |host: &Path| -> Option<String> {
        if let Ok(r) = host.strip_prefix(&app_dir) {
            return Some(format!("{app_root}/{}", r.display()));
        }
        host.strip_prefix(&sysroot).ok().map(|r| format!("{BUNDLE_SYSROOT}/{}", r.display()))
    };
    let mut entries: Vec<(String, Entry)> = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        let mut items: Vec<_> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
        items.sort();
        for p in items {
            if p.symlink_metadata()?.is_dir() {
                walk(&p, out)?;
            } else {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut app_files = Vec::new();
    walk(&app_dir, &mut app_files)?;
    for f in app_files {
        let v = map(&f).unwrap();
        if f.symlink_metadata()?.file_type().is_symlink() {
            match f.canonicalize().ok().and_then(|t| map(&t)) {
                Some(t) => entries.push((v, Entry::Symlink(t))),
                None => entries.push((v, Entry::File(std::fs::read(&f)?))),
            }
        } else {
            entries.push((v, Entry::File(std::fs::read(&f)?)));
        }
    }
    let mut sys_files = Vec::new();
    walk(&sysroot, &mut sys_files)?;
    let fonts = sysroot.join("System/Library/Fonts");
    // A Flutter app's engine reads its shaders' MSL from the sysroot (made by
    // scripts/flutter/impeller-msl.sh for the engine revision it was built with).
    let impeller = flutter_engine_revision(&app_dir).map(|rev| sysroot.join("pomme/impeller").join(rev));
    if let Some(dir) = impeller.as_ref().filter(|d| !d.is_dir()) {
        eprintln!("warning: no Impeller MSL at {} (run scripts/flutter/impeller-msl.sh): Flutter won't draw", dir.display());
    }
    // The CJK fonts: only for the app's own localizations and the languages asked for.
    let mut languages = languages.to_vec();
    if let Ok(dir) = std::fs::read_dir(&app_dir) {
        languages.extend(dir.flatten().filter_map(|e| e.file_name().to_str()?.strip_suffix(".lproj").map(str::to_string)));
    }
    let cjk_wanted = |file: &str| -> bool {
        let wants = |prefixes: &[&str]| languages.iter().any(|l| prefixes.iter().any(|p| l.to_lowercase().starts_with(p)));
        match file.split(['.', '-']).next().unwrap_or("") {
            "NotoSansJP" => wants(&["ja"]),
            "NotoSansSC" => wants(&["zh"]),
            "NotoSansTC" => wants(&["zh-hant", "zh-tw", "zh-hk", "zh-mo"]),
            "NotoSansKR" => wants(&["ko"]),
            _ => true,
        }
    };
    for f in sys_files {
        let real = f.canonicalize()?;
        let font_wanted = f.starts_with(&fonts) && f.file_name().and_then(|n| n.to_str()).is_some_and(cjk_wanted);
        let wanted = loaded.contains(&real) || font_wanted || impeller.as_ref().is_some_and(|d| f.starts_with(d));
        if !wanted {
            continue;
        }
        let v = map(&f).unwrap();
        if f.symlink_metadata()?.file_type().is_symlink() {
            entries.push((v, Entry::Symlink(map(&real).unwrap())));
        } else {
            entries.push((v, Entry::File(std::fs::read(&f)?)));
        }
    }
    let bytes = write_bundle(&entries);
    std::fs::write(out, &bytes)?;
    println!("{}: {} files, {:.1} MB ({} as {app_root})", out.display(), entries.len(), bytes.len() as f64 / 1e6, app_dir.display());
    Ok(())
}

/// The engine revision of the app's Flutter.framework (its Info.plist's
/// `FlutterEngine`), if it has one: as pomme's Metal finds it at run time.
fn flutter_engine_revision(app_dir: &Path) -> Option<String> {
    let plist = plist::Value::from_file(app_dir.join("Frameworks/Flutter.framework/Info.plist")).ok()?;
    plist.as_dictionary()?.get("FlutterEngine")?.as_string().map(str::to_string)
}

fn bundle_images(path: &Path) -> Result<(Option<String>, Vec<PathBuf>)> {
    let (id, exe, app_dir) = pomme_loader::locate_executable(path)?;
    let mut images = vec![exe];
    let mut frameworks = vec![];
    if let Ok(rd) = std::fs::read_dir(app_dir.join("Frameworks")) {
        for p in rd.flatten().map(|e| e.path()) {
            match p.extension().and_then(|x| x.to_str()) {
                Some("framework") => frameworks.push(p.join(p.file_stem().unwrap())),
                Some("dylib") => frameworks.push(p),
                _ => {}
            }
        }
    }
    frameworks.sort();
    images.extend(frameworks);
    Ok((id, images))
}

fn inspect(path: &Path, show_symbols: bool) -> Result<()> {
    let (id, images) = bundle_images(path)?;
    if let Some(id) = id {
        println!("bundle: {id}");
    }

    // Symbols each system library must provide, across every image in the app.
    let mut surface: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for path in &images {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let img = Image::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
        print_image(path, &img, show_symbols);
        for d in img.dylibs.iter().filter(|d| d.kind == DylibKind::System) {
            surface.entry(d.path.clone()).or_default();
        }
        for imp in &img.imports {
            if let LibOrdinal::Dylib(d) = imp.ordinal
                && img.dylibs[d].kind == DylibKind::System
            {
                surface.entry(img.dylibs[d].path.clone()).or_default().insert(imp.name.clone());
            }
        }
    }

    let total: usize = surface.values().map(|s| s.len()).sum();
    println!("\n== HLE surface: system libraries pomme must provide ==");
    println!("{} libraries, {} distinct symbols", surface.len(), total);
    let mut libs: Vec<_> = surface.iter().collect();
    libs.sort_by_key(|(_, syms)| std::cmp::Reverse(syms.len()));
    for (lib, syms) in libs {
        println!("  {:>5}  {lib}", syms.len());
    }
    Ok(())
}

fn print_image(path: &Path, img: &Image<'_>, show_symbols: bool) {
    println!("\n== {} ==", path.file_name().unwrap().to_string_lossy());
    print!("  {} {:?}", img.cpu, img.file_type);
    if let Some(b) = &img.build {
        print!(", {:?} min {} sdk {}", b.platform, b.min_os, b.sdk);
    }
    println!(
        ", {} fixups ({}), {} exports",
        img.fixups.len(),
        if img.uses_chained_fixups { "chained" } else { "dyld_info" },
        img.exports.len()
    );
    for s in &img.segments {
        println!("  seg {:<16} {:#012x} +{:#x} {}", s.name, s.vmaddr, s.vmsize, s.prot);
    }
    let by_lib = img.imports_by_library();
    for d in &img.dylibs {
        let n = by_lib.get(d.path.as_str()).map_or(0, |v| v.len());
        let tag = match d.kind {
            DylibKind::System => "sys",
            DylibKind::Bundled => "app",
        };
        let flags = match (d.weak, d.reexport) {
            (true, _) => " (weak)",
            (_, true) => " (re-export)",
            _ => "",
        };
        println!("  [{tag}] {n:>5} imports  {}{flags}", d.path);
    }
    if show_symbols {
        for (lib, imps) in &by_lib {
            println!("  -- {lib}");
            for i in imps {
                println!("       {}{}", i.name, if i.weak { " (weak)" } else { "" });
            }
        }
    }
}

fn load(path: &Path, show_stubs: bool) -> Result<()> {
    let start = std::time::Instant::now();
    let sysroot = default_sysroot().canonicalize().ok();
    let p = pomme_loader::load_with(path, &pomme_loader::LoadOptions { sysroot })?;
    let elapsed = start.elapsed();

    if let Some(id) = &p.bundle_id {
        println!("bundle: {id}");
    }
    println!("loaded {} images in {:.1?}:", p.images.len(), elapsed);
    for img in &p.images {
        println!("  {:#012x}-{:#012x}  {}", img.start, img.end, img.name);
    }
    if let Some(entry) = p.entry() {
        let insn = p.memory.read_u32(entry)?;
        println!("entry (main): {entry:#x}  first insn {insn:#010x}");
    }
    println!("mapped {:.1} MiB in {} regions", p.memory.mapped_bytes() as f64 / (1 << 20) as f64, p.memory.regions().len());

    let s = &p.stats;
    println!(
        "fixups: {} rebases ({} outside mapped memory), {} binds to guest code, {} binds to {} HLE stubs, {} missing weak",
        s.rebases, s.rebases_unmapped, s.binds_to_guest, s.binds_to_stub, p.stubs.len(), s.weak_missing
    );

    if !p.unresolved.is_empty() {
        println!("\n{} unresolved imports from bundled libraries:", p.unresolved.len());
        for u in p.unresolved.iter().take(20) {
            println!("  {} -> {} ({})", u.image, u.symbol, u.library);
        }
    }
    if show_stubs {
        println!("\nstubs:");
        for (addr, stub) in p.stubs.iter() {
            println!("  {addr:#x}  {}  {}", stub.symbol, stub.library);
        }
    }
    Ok(())
}

/// A hook for POMME_BREAK: print the call, then run the function normally.
fn print_call(emu: &mut pomme_hle::Emu) -> pomme_hle::Result<()> {
    let c = &emu.cpu;
    // An argument that's an Objective-C object shows its class: x2=0x… <NSArray>.
    let arg = |i: usize| match pomme_hle::objc::object_class_name(emu, c.x[i]) {
        Some(n) => format!("{:#x} <{n}>", c.x[i]),
        None => format!("{:#x}", c.x[i]),
    };
    println!(
        "[break] {} from {}  x0={} x1={} x2={} x3={} d0={} d1={} d2={} d3={}",
        emu.symbolize(emu.hook_target()),
        emu.symbolize(c.x[30].wrapping_sub(4)),
        arg(0),
        arg(1),
        arg(2),
        arg(3),
        f64::from_bits(c.q[0] as u64),
        f64::from_bits(c.q[1] as u64),
        f64::from_bits(c.q[2] as u64),
        f64::from_bits(c.q[3] as u64)
    );
    // POMME_BREAK_VREGS=1: also print the vector registers that hold anything.
    if std::env::var_os("POMME_BREAK_VREGS").is_some() {
        let xs: Vec<String> = (4..29).map(|i| format!("x{i}={:#x}", c.x[i])).collect();
        println!("        {}", xs.join(" "));
        for (i, v) in c.q.iter().enumerate() {
            if *v != 0 {
                let lanes: Vec<String> = (0..4).map(|k| format!("{}", f32::from_bits((*v >> (k * 32)) as u32))).collect();
                println!("        v{i:<2} {v:#034x}  f32 [{}]", lanes.join(", "));
            }
        }
    }
    // POMME_BREAK_MEM=x1:8 (or just 8, for x0): also dump that many 8-byte words at the register.
    if let Ok(spec) = std::env::var("POMME_BREAK_MEM") {
        let (reg, n) = spec.split_once(':').map_or(("x0", spec.as_str()), |(r, n)| (r, n));
        let r = reg.trim_start_matches('x').parse::<usize>().unwrap_or(0).min(30);
        let base = c.x[r];
        for i in 0..n.parse::<u64>().unwrap_or(0) {
            if let Ok(w) = emu.p.memory.read_u64(base + i * 8) {
                // A word that points into an image: what it is (type metadata, a function …).
                let what = if emu.p.image_at(w).is_some() { emu.symbolize(w) } else { f64::from_bits(w).to_string() };
                println!("        [x{r}+{:#04x}] {w:#018x}  {what}", i * 8);
            }
        }
    }
    emu.continue_original();
    Ok(())
}

/// `URL=FILE`
fn parse_fixture(s: &str) -> std::result::Result<(String, PathBuf), String> {
    let (url, file) = s.rsplit_once('=').ok_or("expected URL=FILE")?;
    Ok((url.to_string(), PathBuf::from(file)))
}

fn make_net(offline: bool, fixtures: &[(String, PathBuf)]) -> Result<Option<Box<dyn pomme_hle::net::Net>>> {
    let mut f = pomme_hle::net::Fixtures::default();
    for (url, file) in fixtures {
        let body = std::fs::read(file).with_context(|| format!("reading fixture {}", file.display()))?;
        let ct = if file.extension().is_some_and(|e| e == "json") { "application/json; charset=utf-8" } else { "application/octet-stream" };
        f.0.push((url.clone(), pomme_hle::net::Response { status: 200, headers: vec![("Content-Type".into(), ct.into())], body }));
    }
    Ok(if offline {
        (!fixtures.is_empty()).then(|| Box::new(f) as Box<dyn pomme_hle::net::Net>)
    } else {
        Some(Box::new(pomme_net::LiveNet::new(f)))
    })
}

/// `x,y@seconds`
fn parse_typing(s: &str) -> std::result::Result<(String, f64), String> {
    let (text, t) = s.rsplit_once('@').ok_or("expected text@seconds")?;
    let t = t.trim().parse::<f64>().map_err(|e| format!("{t}: {e}"))?;
    Ok((pomme_hle::textinput::unescape(text), t))
}

#[derive(Clone, Debug)]
struct Swipe {
    from: (f64, f64),
    to: (f64, f64),
    at: f64,
    duration: f64,
}

/// `x1,y1>x2,y2@secs` or `…@secs~duration`.
fn parse_swipe(s: &str) -> std::result::Result<Swipe, String> {
    const USAGE: &str = "expected x1,y1>x2,y2@seconds (or …@seconds~duration)";
    let (path, when) = s.split_once('@').ok_or(USAGE)?;
    let (at, duration) = when.split_once('~').unwrap_or((when, "0.3"));
    let (a, b) = path.split_once('>').ok_or(USAGE)?;
    let num = |v: &str| v.trim().parse::<f64>().map_err(|e| format!("{v}: {e}"));
    let point = |p: &str| -> std::result::Result<(f64, f64), String> {
        let (x, y) = p.split_once(',').ok_or(USAGE)?;
        Ok((num(x)?, num(y)?))
    };
    Ok(Swipe { from: point(a)?, to: point(b)?, at: num(at)?, duration: num(duration)? })
}

fn parse_tap(s: &str) -> std::result::Result<(f64, f64, f64), String> {
    let (xy, t) = s.split_once('@').ok_or("expected x,y@seconds")?;
    let (x, y) = xy.split_once(',').ok_or("expected x,y@seconds")?;
    let num = |v: &str| v.trim().parse::<f64>().map_err(|e| format!("{v}: {e}"));
    Ok((num(x)?, num(y)?, num(t)?))
}

#[allow(clippy::too_many_arguments)]
fn run(path: &Path, max_steps: u64, seconds: f64, views: bool, elements: bool, taps: &[(f64, f64, f64)], swipes: &[Swipe], screenshot: Option<&Path>, device: &'static pomme_hle::device::Device, shell: bool, trace: bool, sysroot: &Path, net: Option<Box<dyn pomme_hle::net::Net>>, ios: pomme_hle::system::Version, missing: bool, url: Option<String>, typing: &[(String, f64)], clipboard: &ClipboardArgs, state: &DeviceStateArgs, profile: &ProfileArgs, coverage: Option<&CoverageArgs>, live: Option<(Box<dyn pomme_hle::host::Host>, f32)>) -> Result<()> {
    use pomme_hle::Emu;

    let sysroot = &pomme_loader::vfs::canonicalize(sysroot);
    let opts = pomme_loader::LoadOptions { sysroot: pomme_loader::vfs::is_dir(sysroot).then(|| sysroot.to_path_buf()) };
    let p = pomme_loader::load_with(path, &opts)?;
    let system: Vec<_> = p.images.iter().filter(|i| i.path.starts_with(sysroot)).map(|i| i.name.as_str()).collect();
    if !system.is_empty() {
        println!("system libraries from {}: {}", sysroot.display(), system.join(", "));
    }
    let mut emu = Emu::new(p).map_err(|e| anyhow::anyhow!("setting up the runtime: {e}"))?;
    emu.present(device);
    clipboard.apply(&mut emu)?;
    state.apply(&mut emu);
    emu.ui.launch_url = url;
    for (text, t) in typing {
        emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: (t * 1e9) as u64, text: text.clone() });
    }
    if missing {
        let list = emu.missing_functions();
        println!("{} system functions bound without an implementation:", list.len());
        for (lib, sym) in list {
            println!("  {sym}  ({lib})");
        }
        return Ok(());
    }
    if let Some(c) = coverage {
        *emu.coverage = Some(pomme_hle::coverage::Recorder::new(!c.stop_at_first));
        if c.no_run {
            // As far as dyld goes: CoreFoundation's initializer has to run before HLE backs the data constants.
            emu.max_steps = max_steps;
            let ended = match emu.run_to_main() {
                Ok(()) => "reached main, and stopped there (--no-run)".to_string(),
                Err(stop) => format!("stopped before main: {stop}"),
            };
            return write_coverage(&emu, Some(ended), c);
        }
    }
    emu.max_steps = max_steps;
    emu.system.os = ios;
    let s = &emu.system;
    println!(
        "presenting iOS {}; the app is linked against the iOS {} SDK{}: {} design",
        s.os,
        s.sdk,
        if s.requires_compatibility { " and asks for compatibility" } else { "" },
        if s.liquid_glass() { "Liquid Glass" } else { "pre-iOS 26" }
    );
    emu.time_limit_ns = Some((seconds * 1e9) as u64);
    // Live in a window: until it closes (Device ▸ Relaunch App starts the app again).
    let relaunchable = live.is_some();
    if let Some((host, scale)) = live {
        *emu.live.host = Some(host);
        emu.live.scale = scale;
        emu.time_limit_ns = None;
        emu.max_steps = u64::MAX;
    }
    *emu.net.client = net;
    // Web views' pages: in the host's web view (the pomme-webview helper beside pomme).
    if cfg!(target_os = "macos")
        && let Some(helper) = pomme_webview::WebViewEngine::find_helper()
    {
        match pomme_webview::WebViewEngine::start(&helper) {
            Ok(engine) => *emu.webkit.engine = Some(Box::new(engine)),
            Err(e) => eprintln!("web views: couldn't start {}: {e}", helper.display()),
        }
    }
    for &(x, y, t) in taps {
        pomme_hle::touch::tap(&mut emu.touch.script, (t * 1e9) as u64, x, y);
    }
    for s in swipes {
        pomme_hle::touch::swipe(&mut emu.touch.script, (s.at * 1e9) as u64, s.from, s.to, (s.duration * 1e9) as u64);
    }
    emu.trace = trace;
    if profile.profile.is_some() {
        emu.start_profiling(profile.profile_every);
    }
    // Debugging aid: POMME_BREAK=_sym1,Image:0xaddr,... prints the arguments of every call to those guest functions.
    if let Ok(list) = std::env::var("POMME_BREAK") {
        for sym in list.split(',') {
            // `Image:0xADDR` (an unslid address as `nm` prints it) reaches local functions too.
            let at = sym.split_once(":0x").and_then(|(img, a)| {
                let i = emu.p.images.iter().find(|i| i.name == img)?;
                u64::from_str_radix(a, 16).ok().map(|a| a + i.slide)
            });
            let named = || emu.p.images.iter().find_map(|i| i.symbol(sym).or_else(|| i.local_symbol(sym)));
            // `-[Class selector]` without a symbol (a stripped binary): through the runtime.
            let method = |emu: &mut pomme_hle::Emu| pomme_hle::objc::method_address(emu, sym);
            let found = at.or_else(named);
            let found = found.or_else(|| method(&mut emu));
            match found {
                Some(addr) => {
                    if let Err(e) = emu.hook(addr, print_call) {
                        println!("POMME_BREAK: {e}");
                    }
                }
                None => println!("POMME_BREAK: no guest symbol {sym}"),
            }
        }
    }
    println!(
        "running {} ({} classes, {} host functions bound)",
        emu.p.bundle_id.as_deref().unwrap_or(&emu.p.main().name),
        emu.objc.classes.len() / 2,
        emu.bound_functions()
    );

    let start = std::time::Instant::now();
    let result = if relaunchable { emu.run_main_relaunching() } else { emu.run_main() };
    let elapsed = start.elapsed();
    let steps = emu.cpu.steps;
    let mips = steps as f64 / elapsed.as_secs_f64() / 1e6;
    if let (Some(out), Some(p)) = (&profile.profile, emu.profiler.take()) {
        let names = |tid: usize| emu.th.threads.get(tid).map_or(String::new(), |t| t.name.clone());
        let title = emu.p.bundle_id.clone().unwrap_or_else(|| emu.p.main().name.clone());
        std::fs::write(out, p.speedscope(&title, names)).with_context(|| format!("writing {}", out.display()))?;
        println!("\nprofile: {} samples, every {} instructions, written to {} (open it at https://www.speedscope.app)", p.samples(), p.every, out.display());
        print!("{}", p.summary(15));
    }
    #[cfg(feature = "cpu-time")]
    print_cpu_stats(steps, elapsed, emu.p.memory.code_generation());
    let ended = match &result {
        Ok(code) => format!("exit({code})"),
        Err(stop) => stop.to_string(),
    };
    match result {
        Ok(code) => println!("\nexit({code}) after {steps} instructions in {elapsed:.2?}"),
        Err(stop) => {
            println!("\nstopped after {steps} instructions in {elapsed:.2?} ({mips:.1} MIPS)");
            println!("reason: {stop}");
            print!("{}", emu.describe_stop(&stop));
            let c = &emu.cpu;
            let shown = if trace || std::env::var_os("POMME_THREADS").is_some() { 28 } else { 8 };
            for r in (0..shown).step_by(4) {
                println!(
                    "  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}  x{:<2} {:#018x}",
                    r, c.x[r], r + 1, c.x[r + 1], r + 2, c.x[r + 2], r + 3, c.x[r + 3]
                );
            }
            if trace || std::env::var_os("POMME_THREADS").is_some() {
                print!("other threads:\n{}", pomme_hle::threads::report(&emu));
                let queues = pomme_hle::libsystem::queue_report(&emu);
                if !queues.is_empty() {
                    print!("dispatch queues:\n{queues}");
                }
            }
        }
    }
    let mut calls: Vec<_> = emu.calls.iter().collect();
    calls.sort_by_key(|(name, n)| (std::cmp::Reverse(**n), (*name).clone()));
    println!("\n{} distinct system functions called (top 15):", calls.len());
    for (name, n) in calls.iter().take(15) {
        println!("  {n:>7}  {name}");
    }
    // Lift the limits: describing and drawing the views runs guest code.
    emu.max_steps = u64::MAX;
    emu.time_limit_ns = None;
    if views {
        println!("\nviews:\n{}", emu.describe_views());
    }
    if elements {
        println!("\nelements:\n{}", emu.describe_elements());
    }
    clipboard.report(&mut emu);
    if let Some(out) = screenshot {
        let scale = device.scale as f32;
        match if shell { emu.shell_png(scale) } else { emu.screenshot_png(scale) } {
            Ok(png) => {
                std::fs::write(out, png)?;
                println!("\nscreenshot: {}", out.display());
            }
            Err(e) => println!("\nscreenshot failed: {e}"),
        }
    }
    if let Some(c) = coverage {
        println!();
        write_coverage(&emu, Some(ended), c)?;
    }
    Ok(())
}

/// Print the coverage report (and write it as JSON with `--json`).
fn write_coverage(emu: &pomme_hle::Emu, ended: Option<String>, args: &CoverageArgs) -> Result<()> {
    let report = pomme_hle::coverage::report(emu, ended);
    print!("{}", report.text(if args.all { usize::MAX } else { 8 }));
    if let Some(out) = &args.json {
        std::fs::write(out, serde_json::to_string_pretty(&report)?).with_context(|| format!("writing {}", out.display()))?;
        println!("\ncoverage: {}", out.display());
    }
    Ok(())
}

/// Where the time went (the `cpu-time` and `cpu-stats` features; see openspec/changes/add-jit, task 0).
/// `POMME_CPU_STATS=file` also writes every slow-path encoding with its count.
#[cfg(feature = "cpu-time")]
fn print_cpu_stats(steps: u64, elapsed: std::time::Duration, code_writes: u64) {
    let h = pomme_hle::cpu_stats::snapshot();
    let pct = |a: f64, b: f64| if b == 0.0 { 0.0 } else { 100.0 * a / b };
    let cpu = h.in_cpu.as_secs_f64();
    let all = elapsed.as_secs_f64();
    println!("\ncpu stats:");
    println!("  host time: {all:.2}s total, {cpu:.2}s in the CPU ({:.1}%), {:.2}s in pomme's Rust code ({:.1}%)", pct(cpu, all), all - cpu, pct(all - cpu, all));
    println!("  CPU speed while in the CPU: {:.1} MIPS", steps as f64 / cpu / 1e6);
    let exits = h.stub_exits + h.brk_exits + h.svc_exits + h.other_exits;
    println!(
        "  exits: {exits} ({:.0} per million instructions): {} stub calls, {} other BRK, {} SVC, {} other",
        exits as f64 * 1e6 / steps.max(1) as f64, h.stub_exits, h.brk_exits, h.svc_exits, h.other_exits
    );
    #[cfg(not(feature = "cpu-stats"))]
    let _ = code_writes;
    #[cfg(feature = "cpu-stats")]
    {
        let c = pomme_cpu::stats::snapshot();
        let in_blocks = c.block_insns as f64;
        println!(
            "  blocks: {} run, {:.2} instructions on average; {} distinct blocks; {} fetched; {:.1}% of instructions ran in whole blocks",
            c.blocks_run, in_blocks / c.blocks_run.max(1) as f64, c.block_runs.len(), c.blocks_fetched, pct(in_blocks, steps as f64)
        );
        let hot = |n: u64| c.block_runs.values().filter(|&&r| r >= n).count();
        let hot_insns = |n: u64| -> f64 { c.block_runs.values().filter(|&&r| r >= n).map(|&r| r as f64).sum() };
        for n in [2u64, 10, 50, 1000] {
            println!("  blocks run at least {n} times: {} ({:.1}% of block runs)", hot(n), pct(hot_insns(n), c.blocks_run as f64));
        }
        let lens: Vec<String> = (1..=32).map(|i| format!("{i}:{:.1}", pct(c.len_hist.get(i).copied().unwrap_or(0) as f64, c.blocks_run as f64))).collect();
        println!("  block length, % of block runs: {}", lens.join(" "));
        println!("  slow-path (generic) instructions: {} ({:.1}% of instructions in blocks)", c.generic_total, pct(c.generic_total as f64, in_blocks));
        println!("  code writes (code generation): {code_writes}; block cache dropped {} times; {} single steps", c.cache_flushes, c.single_steps);
        let mut g: Vec<_> = c.generic.iter().collect();
        g.sort_by(|a, b| b.1.cmp(a.1));
        if let Some(path) = std::env::var_os("POMME_CPU_STATS") {
            let text: String = g.iter().map(|(k, v)| format!("{k:08x} {v}\n")).collect();
            let _ = std::fs::write(path, text);
        }
    }
}
