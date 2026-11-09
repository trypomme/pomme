//! UIKit: the application, scenes, the screen, and the main run loop.
//!
//! `UIApplicationMain` does what iOS's does for a scene-based app: create the
//! app and its delegate, deliver the launch callbacks, connect one window
//! scene from the Info.plist scene manifest (its delegate creates the window),
//! deliver the foreground/active callbacks, then run the main loop.
//!
//! Views live in `uiview.rs`.

use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result, Stop, foundation, swift};

/// The device we pretend to be: an iPhone with a 390×844 pt screen at 3×.

#[derive(Default)]
pub struct State {
    pub app: u64,
    pub delegate: u64,
    /// Set once `application:didFinishLaunchingWithOptions:` returns.
    pub launched: bool,
    pub screen: u64,
    pub scene: u64,
    pub scene_delegate: u64,
    /// UIStoryboard objects → their compiled directory (guest path).
    pub storyboards: std::collections::HashMap<u64, String>,
    pub session: u64,
    pub key_window: u64,
    pub windows: Vec<u64>,
    pub views: crate::uiview::Views,
    /// CGColorGetComponents buffers, per colour.
    pub color_components: std::collections::HashMap<u64, u64>,
    /// The one RGB colour space.
    pub rgb_space: u64,
    /// `performSelector:withObject:afterDelay:` requests not yet run: {target, sel, arg}.
    pub delayed_performs: Vec<u64>,
    /// The URL the app is launched with (a deep link), as iOS passes it.
    pub launch_url: Option<String>,
    /// How the interface is turned, and how the device is held (see orientation.rs).
    pub orientation: crate::device::Orientation,
    pub device_orientation: crate::device::Orientation,
    /// Scripted turns of the phone: (virtual ns, how it's then held).
    pub rotations: Vec<(u64, crate::device::Orientation)>,
    /// The interface turn in progress, if any.
    pub transition: crate::orientation::Transition,
}

/// The launch URL as an NSURL, if there is one.
fn launch_nsurl(emu: &mut Emu) -> Result<u64> {
    let Some(url) = emu.ui.launch_url.clone() else { return Ok(0) };
    let s = foundation::new_nsstring(emu, &url)?;
    let cls = host_class(emu, "NSURL")?;
    send(emu, cls, "URLWithString:", &[s])
}

/// `connectionOptions.URLContexts`: the launch URL, as a UIOpenURLContext.
fn url_contexts(emu: &mut Emu) -> Result<()> {
    let url = launch_nsurl(emu)?;
    if url == 0 {
        return empty_set(emu);
    }
    let ctx = new_object(emu, "UIOpenURLContext")?;
    let options = new_object(emu, "UISceneOpenURLOptions")?;
    objc::bag_set(emu, ctx, "URL", url);
    objc::bag_set(emu, ctx, "options", options);
    let nsset = host_class(emu, "NSSet")?;
    let set = send(emu, nsset, "setWithObject:", &[ctx])?;
    emu.ret(set);
    Ok(())
}

fn class_named(emu: &mut Emu, name: &str) -> Result<u64> {
    let c = objc::class_by_name(emu, name)?;
    if c == 0 {
        return Err(Stop::Unsupported(format!("no class named {name}")));
    }
    Ok(c)
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

/// The body of `UIApplicationMain`.
fn application_main(emu: &mut Emu, principal: Option<String>, delegate: Option<String>) -> Result<()> {
    let principal = principal.unwrap_or_else(|| "UIApplication".into());
    let app_cls = class_named(emu, &principal)?;
    let app = send(emu, app_cls, "alloc", &[])?;
    let app = send(emu, app, "init", &[])?;
    emu.ui.app = app;
    // The interface starts as the phone is held, if Info.plist allows.
    let held = emu.ui.device_orientation;
    crate::orientation::launch(emu, held);

    if let Some(name) = delegate {
        let cls = class_named(emu, &name)?;
        let d = send(emu, cls, "alloc", &[])?;
        let d = send(emu, d, "init", &[])?;
        send(emu, app, "setDelegate:", &[d])?;
    }
    let d = emu.ui.delegate;
    // A deep link: in the launch options, and (without scenes) application:openURL:options:.
    let url = launch_nsurl(emu)?;
    let launch_options = if url != 0 {
        let k = foundation::new_nsstring(emu, "UIApplicationLaunchOptionsURLKey")?;
        foundation::new_dict_of(emu, &[(k, url)])?
    } else {
        0
    };
    for sel in ["application:willFinishLaunchingWithOptions:", "application:didFinishLaunchingWithOptions:"] {
        call_if_implemented(emu, d, sel, &[app, launch_options])?;
    }
    if url != 0 && scene_delegate_class(emu).is_none() {
        let empty = foundation::new_dict_of(emu, &[])?;
        call_if_implemented(emu, d, "application:openURL:options:", &[app, url, empty])?;
    }
    emu.ui.launched = true;
    foundation::post_named(emu, "UIApplicationDidFinishLaunchingNotification", app)?;

    // A window the app delegate made visible itself (no scenes) appears now.
    let mut roots = Vec::new();
    if scene_delegate_class(emu).is_none() {
        roots.extend(window_roots(emu));
        // The root view controllers have their say on the orientation before the first frame.
        crate::orientation::settle(emu, false)?;
        for &vc in &roots {
            crate::navigation::appearance(emu, vc, "viewWillAppear:", false)?;
        }
    }
    if let Some(delegate_class) = scene_delegate_class(emu) {
        roots.extend(connect_scene(emu, &delegate_class)?);
    }
    call_if_implemented(emu, d, "applicationDidBecomeActive:", &[app])?;
    foundation::post_named(emu, "UIApplicationDidBecomeActiveNotification", app)?;
    // The first frame is laid out and on screen: the root view controllers have appeared.
    crate::uiview::layout_windows(emu)?;
    for vc in roots {
        crate::navigation::appearance(emu, vc, "viewDidAppear:", false)?;
    }
    run_loop(emu)
}

fn call_if_implemented(emu: &mut Emu, obj: u64, sel: &str, args: &[u64]) -> Result<Option<u64>> {
    if obj == 0 {
        return Ok(None);
    }
    let s = objc::intern(emu, sel)?;
    if objc::responds(emu, obj, s)? { send(emu, obj, sel, args).map(Some) } else { Ok(None) }
}

/// `UIApplicationSceneManifest` → the application-role scene's delegate class name.
fn scene_delegate_class(emu: &Emu) -> Option<String> {
    let info = crate::bundle::host_info_plist(emu)?;
    let configs = info.as_dictionary()?.get("UIApplicationSceneManifest")?.as_dictionary()?.get("UISceneConfigurations")?.as_dictionary()?;
    let app_role = configs.get("UIWindowSceneSessionRoleApplication")?.as_array()?;
    let first = app_role.first()?.as_dictionary()?;
    first.get("UISceneDelegateClassName")?.as_string().map(str::to_string)
}

/// The root view controllers of the visible windows.
pub(crate) fn window_roots(emu: &mut Emu) -> Vec<u64> {
    let windows = emu.ui.windows.clone();
    let mut roots = Vec::new();
    for w in windows {
        if !crate::uiview::view(emu, w).hidden
            && let Some(&vc) = emu.ui.views.root_vc.get(&w)
            && vc != 0
        {
            roots.push(vc);
        }
    }
    roots
}

/// Connect the app's one window scene: what UIKit does after launch. Returns
/// the root view controllers that will appear.
fn connect_scene(emu: &mut Emu, delegate_class: &str) -> Result<Vec<u64>> {
    let session = new_object(emu, "UISceneSession")?;
    let scene = new_object(emu, "UIWindowScene")?;
    let options = new_object(emu, "UISceneConnectionOptions")?;
    emu.ui.session = session;
    emu.ui.scene = scene;

    let cls = class_named(emu, delegate_class)?;
    let d = send(emu, cls, "alloc", &[])?;
    let d = send(emu, d, "init", &[])?;
    emu.ui.scene_delegate = d;

    // A scene configuration with a storyboard: UIKit makes the window, with the
    // storyboard's initial view controller, and gives it to the delegate first.
    let mut window = 0;
    if let Some(dir) = crate::storyboard::scene_storyboard(emu) {
        let vc = crate::storyboard::instantiate(emu, &dir, None)?;
        if vc != 0 {
            let wcls = host_class(emu, "UIWindow")?;
            let w = send(emu, wcls, "alloc", &[])?;
            window = send(emu, w, "initWithWindowScene:", &[scene])?;
            send(emu, window, "setRootViewController:", &[vc])?;
            call_if_implemented(emu, d, "setWindow:", &[window])?;
        }
    }
    foundation::post_named(emu, "UISceneWillConnectNotification", scene)?;
    call_if_implemented(emu, d, "scene:willConnectToSession:options:", &[scene, session, options])?;
    if window != 0 {
        send(emu, window, "makeKeyAndVisible", &[])?;
    }
    let roots = window_roots(emu);
    crate::orientation::settle(emu, false)?;
    for &vc in &roots {
        crate::navigation::appearance(emu, vc, "viewWillAppear:", false)?;
    }
    call_if_implemented(emu, d, "sceneWillEnterForeground:", &[scene])?;
    foundation::post_named(emu, "UISceneWillEnterForegroundNotification", scene)?;
    call_if_implemented(emu, d, "sceneDidBecomeActive:", &[scene])?;
    foundation::post_named(emu, "UISceneDidActivateNotification", scene)?;
    Ok(roots)
}

/// The main run loop: CoreFoundation's own, run until nothing can happen.
/// While the main thread waits in it, the other threads run (see threads.rs).
fn run_loop(emu: &mut Emu) -> Result<()> {
    crate::libsystem::drain_queue(emu)?;
    let mode = foundation::cf_data_pub(emu, "_kCFRunLoopDefaultMode")?;
    // Lay out dirty views whenever the loop is about to sleep, like CoreAnimation's commit observer.
    const BEFORE_WAITING: u64 = 1 << 5;
    const EXIT: u64 = 1 << 7;
    // A layout pass can queue work for the loop's other observers (React Native
    // sends events from layoutSubviews and delivers them on its own
    // before-waiting observer, which runs earlier). UIKit's display-driven update
    // cycle comes round again after a frame with layout; here the loop wakes once
    // more so those observers run.
    let callout = emu.host_thunk(|e| {
        if crate::uiview::layout_windows(e)? {
            let main = foundation::cf_call(e, "_CFRunLoopGetMain", &[])?;
            foundation::cf_call(e, "_CFRunLoopWakeUp", &[main])?;
        }
        crate::host::maybe_frame(e)
    })?;
    let observer = foundation::cf_call(emu, "_CFRunLoopObserverCreate", &[0, BEFORE_WAITING | EXIT, 1, 2_000_000, callout, 0])?;
    let rl = foundation::cf_call(emu, "_CFRunLoopGetMain", &[])?;
    let common = foundation::cf_data_pub(emu, "_kCFRunLoopCommonModes")?;
    foundation::cf_call(emu, "_CFRunLoopAddObserver", &[rl, observer, common])?;
    crate::touch::schedule_script(emu)?;
    crate::textinput::schedule_script(emu)?;
    crate::clipboard::schedule_script(emu)?;
    crate::orientation::schedule_script(emu)?;
    crate::host::install(emu)?;
    loop {
        emu.cpu.q[0] = 1.0e10f64.to_bits() as u128; // seconds
        let r = foundation::cf_call(emu, "_CFRunLoopRunInMode", &[mode, 0])?;
        if r == 1 {
            // kCFRunLoopRunFinished: no sources or timers left.
            let windows = emu.ui.windows.len();
            let views = emu.ui.views.count();
            return Err(Stop::Idle(format!("main run loop has no sources or timers left ({windows} window(s), {views} view(s))")));
        }
    }
}

/// Swift overlay: `UIApplicationMain(_ argc: Int32, _ argv: …, _ principalClassName: String?, _ delegateClassName: String?) -> Int32`
fn swift_application_main(emu: &mut Emu) -> Result<()> {
    let principal = swift::read_optional_string(emu, emu.cpu.x[2], emu.cpu.x[3])?;
    let delegate = swift::read_optional_string(emu, emu.cpu.x[4], emu.cpu.x[5])?;
    application_main(emu, principal, delegate)
}

/// `int UIApplicationMain(int argc, char **argv, NSString *principal, NSString *delegate)`
fn c_application_main(emu: &mut Emu) -> Result<()> {
    let (p, d) = (emu.arg(2), emu.arg(3));
    let principal = if p == 0 { None } else { Some(foundation::nsstring(emu, p)?) };
    let delegate = if d == 0 { None } else { Some(foundation::nsstring(emu, d)?) };
    application_main(emu, principal, delegate)
}

/// `CACurrentMediaTime()`: seconds since boot, on the virtual clock.
fn ca_current_media_time(emu: &mut Emu) -> Result<()> {
    let t = crate::threads::now_ns(emu) as f64 / 1e9;
    ret_f64(emu, t);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CACurrentMediaTime", ca_current_media_time),
    // CAFrameRateRange is three floats in, the same three out (s0-s2).
    ("_CAFrameRateRangeMake", |_| Ok(())),
    // Swift's AccessibilitySettings (default settings): animated images on, no
    // preference for non-blinking cursors; change notifications are their names.
    ("_$sSo21AccessibilitySettingsV0A0E21animatedImagesEnabledSbvgZ", |e| {
        e.ret(1);
        Ok(())
    }),
    ("_$sSo21AccessibilitySettingsV0A0E40prefersNonBlinkingTextInsertionIndicatorSbvgZ", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_$sSo21AccessibilitySettingsV0A0E42animatedImagesEnabledDidChangeNotificationSo18NSNotificationNameavgZ", |e| {
        let n = foundation::new_nsstring(e, "AXAnimatedImagesEnabledDidChangeNotification")?;
        e.ret(n);
        Ok(())
    }),
    // Swift's CAFrameRateRange(minimum:maximum:preferred: Float?): min/max in s0/s1;
    // the optional's payload in w0 and its nil tag in w1. Returns the three floats.
    ("_$sSo16CAFrameRateRangeV10QuartzCoreE7minimum7maximum9preferredABSf_S2fSgtcfC", |e| {
        let preferred = if e.cpu.x[1] & 1 == 0 { e.cpu.x[0] as u32 } else { 0 };
        e.cpu.q[2] = preferred as u128;
        Ok(())
    }),
    ("_$s5UIKit17UIApplicationMainys5Int32VAD_SpySpys4Int8VGGSgSSSgAJtF", swift_application_main),
    ("_UIApplicationMain", c_application_main),
];

// ---- UIApplication ----

fn shared_application(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.app);
    Ok(())
}

fn app_delegate(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.delegate);
    Ok(())
}

fn set_app_delegate(emu: &mut Emu) -> Result<()> {
    emu.ui.delegate = emu.arg(2);
    Ok(())
}

fn ret_zero(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn key_window(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.key_window);
    Ok(())
}

fn windows(emu: &mut Emu) -> Result<()> {
    let w = emu.ui.windows.clone();
    let a = foundation::new_array_of(emu, &w)?;
    emu.ret(a);
    Ok(())
}

fn connected_scenes(emu: &mut Emu) -> Result<()> {
    let s = emu.ui.scene;
    let arr = if s == 0 { Vec::new() } else { vec![s] };
    let nsset = host_class(emu, "NSSet")?;
    let set = send(emu, nsset, "set", &[])?;
    for o in arr {
        send(emu, set, "addObject:", &[o])?;
    }
    emu.ret(set);
    Ok(())
}

/// `UIApplicationState`: active (0).
fn application_state(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

// ---- UIScene / UIWindowScene ----

fn scene_session(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.session);
    Ok(())
}

fn scene_delegate(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.scene_delegate);
    Ok(())
}

fn set_scene_delegate(emu: &mut Emu) -> Result<()> {
    emu.ui.scene_delegate = emu.arg(2);
    Ok(())
}

/// `UISceneActivationState`: foreground active (0).
fn activation_state(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn scene_screen(emu: &mut Emu) -> Result<()> {
    let s = main_screen_obj(emu)?;
    emu.ret(s);
    Ok(())
}

fn scene_windows(emu: &mut Emu) -> Result<()> {
    windows(emu)
}

fn session_role(emu: &mut Emu) -> Result<()> {
    let s = foundation::new_nsstring(emu, "UIWindowSceneSessionRoleApplication")?;
    emu.ret(s);
    Ok(())
}

fn empty_set(emu: &mut Emu) -> Result<()> {
    let nsset = host_class(emu, "NSSet")?;
    let set = send(emu, nsset, "set", &[])?;
    emu.ret(set);
    Ok(())
}

fn interface_orientation(emu: &mut Emu) -> Result<()> {
    emu.ret(crate::orientation::current(emu) as u64);
    Ok(())
}

// ---- UIScreen ----

pub fn main_screen_obj(emu: &mut Emu) -> Result<u64> {
    if emu.ui.screen == 0 {
        emu.ui.screen = new_object(emu, "UIScreen")?;
    }
    Ok(emu.ui.screen)
}

fn main_screen(emu: &mut Emu) -> Result<()> {
    let s = main_screen_obj(emu)?;
    emu.ret(s);
    Ok(())
}

/// Return a CGRect in d0–d3.
pub fn ret_rect(emu: &mut Emu, r: [f64; 4]) {
    for (i, v) in r.iter().enumerate() {
        emu.cpu.q[i] = v.to_bits() as u128;
    }
}

pub fn ret_f64(emu: &mut Emu, v: f64) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

fn screen_bounds(emu: &mut Emu) -> Result<()> {
    let (w, h) = crate::orientation::screen(emu);
    ret_rect(emu, [0.0, 0.0, w, h]);
    Ok(())
}

fn screen_native_bounds(emu: &mut Emu) -> Result<()> {
    let d = emu.device;
    ret_rect(emu, [0.0, 0.0, d.width * d.scale, d.height * d.scale]);
    Ok(())
}

fn screen_scale(emu: &mut Emu) -> Result<()> {
    ret_f64(emu, emu.device.scale);
    Ok(())
}

fn screen_fps(emu: &mut Emu) -> Result<()> {
    emu.ret(60);
    Ok(())
}

// ---- UIDevice ----

pub(crate) fn current_device_obj(emu: &mut Emu) -> Result<u64> {
    if emu.ui.views.device == 0 {
        emu.ui.views.device = new_object(emu, "UIDevice")?;
    }
    Ok(emu.ui.views.device)
}

fn current_device(emu: &mut Emu) -> Result<()> {
    let d = current_device_obj(emu)?;
    emu.ret(d);
    Ok(())
}

fn device_string(emu: &mut Emu, s: &str) -> Result<()> {
    let r = foundation::new_nsstring(emu, s)?;
    emu.ret(r);
    Ok(())
}

// ---- CADisplayLink: {isa, target, selector, paused, CFRunLoopTimer, timestamp} ----

const FRAME: f64 = 1.0 / 60.0;

fn display_link_new(emu: &mut Emu) -> Result<()> {
    let (target, sel) = (emu.arg(2), emu.arg(3));
    let link = new_object(emu, "CADisplayLink")?;
    emu.write_u64(link + 8, target)?;
    emu.write_u64(link + 16, sel)?;
    emu.ret(link);
    Ok(())
}

/// The timer callout `void (*)(CFRunLoopTimerRef, void *info)`: one frame.
fn display_link_fire(emu: &mut Emu) -> Result<()> {
    let link = emu.arg(1);
    if emu.read_u64(link + 24)? != 0 {
        return Ok(()); // paused
    }
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    emu.write_u64(link + 40, now.to_bits())?;
    let (target, sel) = (emu.read_u64(link + 8)?, emu.read_u64(link + 16)?);
    let name = objc::sel_name(emu, sel);
    send(emu, target, &name, &[link])?;
    Ok(())
}

/// Run `callout(timer, info)` once on the main run loop, `delay_s` of virtual time from now.
pub fn after_on_main(emu: &mut Emu, delay_s: f64, callout: HostFn, info: u64) -> Result<()> {
    let callout = emu.host_thunk(callout)?;
    let rl = foundation::cf_call(emu, "_CFRunLoopGetMain", &[])?;
    let common = foundation::cf_data_pub(emu, "_kCFRunLoopCommonModes")?;
    foundation::cf_call(emu, "_CFAbsoluteTimeGetCurrent", &[])?;
    let fire = f64::from_bits(emu.last_q0 as u64) + delay_s;
    // CFRunLoopTimerContext {version, info, retain, release, copyDescription}
    let ctx = emu.alloc(40)?;
    emu.write_u64(ctx + 8, info)?;
    emu.cpu.q[0] = fire.to_bits() as u128;
    emu.cpu.q[1] = 0f64.to_bits() as u128;
    let timer = foundation::cf_call(emu, "_CFRunLoopTimerCreate", &[0, 0, 0, callout, ctx])?;
    foundation::cf_call(emu, "_CFRunLoopAddTimer", &[rl, timer, common])?;
    foundation::cf_call(emu, "_CFRelease", &[timer])?;
    Ok(())
}

// ---- -performSelector:withObject:afterDelay: (on the current run loop: the main one) ----

fn perform_after_delay(emu: &mut Emu) -> Result<()> {
    let (target, sel, arg) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let delay = f64::from_bits(emu.cpu.q[0] as u64).max(0.0);
    let req = emu.alloc(24)?;
    emu.write_u64(req, target)?;
    emu.write_u64(req + 8, sel)?;
    emu.write_u64(req + 16, arg)?;
    // The target and argument are retained until the request runs.
    send(emu, target, "retain", &[])?;
    if arg != 0 {
        send(emu, arg, "retain", &[])?;
    }
    emu.ui.delayed_performs.push(req);
    after_on_main(emu, delay, perform_fire, req)
}

fn perform_fire(emu: &mut Emu) -> Result<()> {
    let req = emu.arg(1);
    let Some(i) = emu.ui.delayed_performs.iter().position(|&r| r == req) else { return Ok(()) }; // cancelled
    emu.ui.delayed_performs.remove(i);
    let (target, sel, arg) = (emu.read_u64(req)?, emu.read_u64(req + 8)?, emu.read_u64(req + 16)?);
    let name = objc::sel_name(emu, sel);
    send(emu, target, &name, &[arg])?;
    send(emu, target, "release", &[])?;
    if arg != 0 {
        send(emu, arg, "release", &[])?;
    }
    Ok(())
}

/// `+cancelPreviousPerformRequestsWithTarget:[selector:object:]`
fn cancel_performs(emu: &mut Emu) -> Result<()> {
    let target = emu.arg(2);
    let only = objc::sel_name(emu, emu.arg(1)).contains("selector:").then(|| (emu.arg(3), emu.arg(4)));
    let reqs = emu.ui.delayed_performs.clone();
    for req in reqs {
        let (t, sel, arg) = (emu.read_u64(req)?, emu.read_u64(req + 8)?, emu.read_u64(req + 16)?);
        let matches = t == target && only.is_none_or(|(s, a)| s == sel && a == arg);
        if matches {
            emu.ui.delayed_performs.retain(|&r| r != req);
            send(emu, t, "release", &[])?;
            if arg != 0 {
                send(emu, arg, "release", &[])?;
            }
        }
    }
    Ok(())
}

/// `-addToRunLoop:forMode:` — a repeating CFRunLoopTimer at 60 Hz.
fn display_link_add(emu: &mut Emu) -> Result<()> {
    let (link, run_loop, mode) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let rl = send(emu, run_loop, "getCFRunLoop", &[])?;
    let callout = emu.host_thunk(display_link_fire)?;
    // CFRunLoopTimerContext {version, info, retain, release, copyDescription}
    let ctx = emu.alloc(40)?;
    emu.write_u64(ctx + 8, link)?;
    let now = foundation::cf_call(emu, "_CFAbsoluteTimeGetCurrent", &[])?;
    let _ = now;
    let first = f64::from_bits(emu.last_q0 as u64) + FRAME;
    emu.cpu.q[0] = first.to_bits() as u128;
    emu.cpu.q[1] = FRAME.to_bits() as u128;
    let timer = foundation::cf_call(emu, "_CFRunLoopTimerCreate", &[0, 0, 0, callout, ctx])?;
    foundation::cf_call(emu, "_CFRunLoopAddTimer", &[rl, timer, mode])?;
    emu.write_u64(link + 32, timer)?;
    Ok(())
}

fn display_link_invalidate(emu: &mut Emu) -> Result<()> {
    let timer = emu.read_u64(emu.arg(0) + 32)?;
    if timer != 0 {
        foundation::cf_call(emu, "_CFRunLoopTimerInvalidate", &[timer])?;
    }
    Ok(())
}

fn display_link_set_paused(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.arg(0) + 24, emu.arg(2) & 0xff)
}

fn display_link_paused(emu: &mut Emu) -> Result<()> {
    let p = emu.read_u64(emu.arg(0) + 24)?;
    emu.ret(p);
    Ok(())
}

fn display_link_timestamp(emu: &mut Emu) -> Result<()> {
    let t = f64::from_bits(emu.read_u64(emu.arg(0) + 40)?);
    ret_f64(emu, t);
    Ok(())
}

fn display_link_target_timestamp(emu: &mut Emu) -> Result<()> {
    let t = f64::from_bits(emu.read_u64(emu.arg(0) + 40)?) + FRAME;
    ret_f64(emu, t);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSObject", false, "performSelector:withObject:afterDelay:", perform_after_delay),
    ("NSObject", false, "performSelector:withObject:afterDelay:inModes:", perform_after_delay),
    ("NSObject", true, "cancelPreviousPerformRequestsWithTarget:", cancel_performs),
    ("NSObject", true, "cancelPreviousPerformRequestsWithTarget:selector:object:", cancel_performs),
    ("CADisplayLink", true, "displayLinkWithTarget:selector:", display_link_new),
    ("CADisplayLink", false, "addToRunLoop:forMode:", display_link_add),
    ("CADisplayLink", false, "removeFromRunLoop:forMode:", display_link_invalidate),
    ("CADisplayLink", false, "invalidate", display_link_invalidate),
    ("CADisplayLink", false, "setPaused:", display_link_set_paused),
    ("CADisplayLink", false, "isPaused", display_link_paused),
    ("CADisplayLink", false, "timestamp", display_link_timestamp),
    ("CADisplayLink", false, "targetTimestamp", display_link_target_timestamp),
    ("CADisplayLink", false, "duration", |e| {
        ret_f64(e, FRAME);
        Ok(())
    }),
    ("CADisplayLink", false, "setPreferredFramesPerSecond:", noop),
    // 0: "the display's maximum", what an untouched link reports.
    ("CADisplayLink", false, "preferredFramesPerSecond", |e| {
        e.ret(0);
        Ok(())
    }),
    ("CADisplayLink", false, "preferredFrameRateRange", |e| {
        for i in 0..3 {
            e.cpu.q[i] = 0;
        }
        Ok(())
    }),
    ("CADisplayLink", false, "setPreferredFrameRateRange:", noop),
    ("UIApplication", true, "sharedApplication", shared_application),
    // Opening a URL in another app: there are no other apps. Report it
    // (`--trace`) and answer "not opened".
    ("UIApplication", false, "openURL:options:completionHandler:", |e| {
        let (url, done) = (e.arg(2), e.arg(4));
        if e.trace {
            let s = crate::objc::send(e, url, "absoluteString", &[])?;
            println!("[uikit] openURL {} (not opened)", if s == 0 { String::new() } else { foundation::nsstring(e, s)? });
        }
        if done != 0 {
            crate::libsystem::call_block(e, done, &[0])?;
        }
        Ok(())
    }),
    ("UIApplication", false, "openURL:", |e| {
        e.ret(0);
        Ok(())
    }),
    ("UIApplication", false, "delegate", app_delegate),
    ("UIApplication", false, "setDelegate:", set_app_delegate),
    ("UIApplication", false, "keyWindow", key_window),
    ("UIApplication", false, "windows", windows),
    ("UIApplication", false, "connectedScenes", connected_scenes),
    ("UIApplication", false, "applicationState", application_state),
    ("UIApplication", false, "statusBarFrame", |e| {
        let f = crate::orientation::status_bar_frame(e);
        ret_rect(e, f);
        Ok(())
    }),
    ("UIApplication", false, "statusBarStyle", ret_zero),
    ("UIApplication", false, "setStatusBarStyle:", noop),
    ("UIApplication", false, "setStatusBarStyle:animated:", noop),
    ("UIApplication", false, "setStatusBarHidden:", noop),
    ("UIApplication", false, "setStatusBarHidden:withAnimation:", noop),
    ("UIApplication", false, "preferredContentSizeCategory", |e| {
        // The same string pomme binds to UIContentSizeCategoryLarge (see constants.rs).
        let s = foundation::new_nsstring(e, "UIContentSizeCategoryLarge")?;
        e.ret(s);
        Ok(())
    }),
    ("UIApplication", false, "isIdleTimerDisabled", ret_zero),
    ("UIApplication", false, "setIdleTimerDisabled:", noop),
    ("UIApplication", false, "applicationIconBadgeNumber", ret_zero),
    ("UIApplication", false, "isRegisteredForRemoteNotifications", ret_zero),
    ("UIApplication", false, "supportsMultipleScenes", ret_zero),
    ("UIApplication", false, "userInterfaceLayoutDirection", ret_zero),
    ("UIApplication", false, "statusBarOrientation", interface_orientation),
    ("UIApplication", false, "beginIgnoringInteractionEvents", noop),
    ("UIApplication", false, "endIgnoringInteractionEvents", noop),
    ("UIScene", false, "session", scene_session),
    ("UIScene", false, "delegate", scene_delegate),
    ("UIScene", false, "setDelegate:", set_scene_delegate),
    ("UIScene", false, "activationState", activation_state),
    ("UIWindowScene", false, "screen", scene_screen),
    ("UIWindowScene", false, "statusBarManager", |e| {
        let m = new_object(e, "UIStatusBarManager")?;
        e.ret(m);
        Ok(())
    }),
    ("UIStatusBarManager", false, "statusBarFrame", |e| {
        let f = crate::orientation::status_bar_frame(e);
        ret_rect(e, f);
        Ok(())
    }),
    ("UIStatusBarManager", false, "statusBarStyle", ret_zero),
    ("UIWindowScene", false, "windows", scene_windows),
    ("UIWindowScene", false, "interfaceOrientation", interface_orientation),
    ("UIWindowScene", false, "keyWindow", key_window),
    ("UISceneSession", false, "role", session_role),
    ("UISceneSession", false, "scene", |e| {
        e.ret(e.ui.scene);
        Ok(())
    }),
    ("UISceneConnectionOptions", false, "URLContexts", url_contexts),
    ("UISceneConnectionOptions", false, "userActivities", empty_set),
    ("UISceneConnectionOptions", false, "shortcutItem", ret_zero),
    ("UISceneConnectionOptions", false, "notificationResponse", ret_zero),
    ("UISceneConnectionOptions", false, "sourceApplication", ret_zero),
    ("UIScreen", true, "mainScreen", main_screen),
    ("UIScreen", true, "screens", |e| {
        let s = main_screen_obj(e)?;
        let a = foundation::new_array_of(e, &[s])?;
        e.ret(a);
        Ok(())
    }),
    ("UIScreen", false, "traitCollection", |e| {
        let tc = host_class(e, "UITraitCollection")?;
        let t = send(e, tc, "currentTraitCollection", &[])?;
        e.ret(t);
        Ok(())
    }),
    ("UIScreen", false, "bounds", screen_bounds),
    ("UIScreen", false, "applicationFrame", screen_bounds),
    ("UIScreen", false, "nativeBounds", screen_native_bounds),
    ("UIScreen", false, "scale", screen_scale),
    ("UIScreen", false, "nativeScale", screen_scale),
    ("UIScreen", false, "maximumFramesPerSecond", screen_fps),
    ("UIDevice", true, "currentDevice", current_device),
    ("UIDevice", false, "systemName", |e| device_string(e, "iOS")),
    ("UIDevice", false, "systemVersion", |e| {
        let v = e.system.os.short();
        device_string(e, &v)
    }),
    ("UIDevice", false, "model", |e| device_string(e, "iPhone")),
    ("UIDevice", false, "name", |e| device_string(e, "pomme")),
    ("UIDevice", false, "userInterfaceIdiom", ret_zero), // phone
    ("UIDevice", false, "isBatteryMonitoringEnabled", ret_zero),
    ("UIDevice", false, "identifierForVendor", ret_zero),
    ("UIDevice", false, "self", ret_self),
];
