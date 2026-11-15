//! Orientation: how the phone is held (the device orientation) and how the
//! interface is turned (the interface orientation), and the geometry that
//! follows from it. Everything that depends on orientation asks here: the
//! screen's and windows' size, the safe area, the status bar, the keyboard,
//! the size classes.
//!
//! The two orientations differ when the app doesn't support the way the
//! device is held: the interface stays where it was. They share numbers
//! (UIDeviceOrientationLandscapeLeft = UIInterfaceOrientationLandscapeRight
//! = 3), so one enum serves both.
//!
//! Turning (see `turn_interface`) follows UIKit's sequence, measured in the
//! iOS 26 Simulator; see the comments there.

use crate::device::Orientation;
use crate::objc::{self, send, send_fp};
use crate::{foundation, Emu, HostFn, Result};

/// How long the interface takes to turn (UIViewControllerTransitionCoordinator.transitionDuration, measured).
pub const TRANSITION_SECONDS: f64 = 0.3;

/// UIInterfaceOrientationMaskAllButUpsideDown: iPhone's default.
const ALL_BUT_UPSIDE_DOWN: u64 = (1 << 1) | (1 << 3) | (1 << 4);

/// The orientations the interface may take, in UIKit's preference order
/// when it must choose one itself.
const PREFERENCE: [Orientation; 4] = [Orientation::Portrait, Orientation::LandscapeRight, Orientation::LandscapeLeft, Orientation::PortraitUpsideDown];

/// The interface orientation.
pub fn current(emu: &Emu) -> Orientation {
    emu.ui.orientation
}

/// The screen's size in points as the interface is turned (UIScreen.bounds,
/// the windows' and scene's bounds).
pub fn screen(emu: &Emu) -> (f64, f64) {
    emu.device.screen(current(emu))
}

/// The screen's safe-area insets (top, left, bottom, right) as the interface is turned.
pub fn safe_area(emu: &Emu) -> [f64; 4] {
    emu.device.safe_area(current(emu))
}

/// The status bar's frame (0-high when hidden, as in landscape).
pub fn status_bar_frame(emu: &Emu) -> [f64; 4] {
    let (w, _) = screen(emu);
    let h = emu.device.status_bar(current(emu));
    [0.0, 0.0, if h > 0.0 { w } else { 0.0 }, h]
}

/// The keyboard's height for the interface as it's turned.
pub fn keyboard_height(emu: &Emu, predictive: bool) -> f64 {
    emu.device.keyboard_height(current(emu), predictive)
}

/// (horizontal, vertical) size classes.
pub fn size_classes(emu: &Emu) -> (u64, u64) {
    emu.device.size_classes(current(emu))
}

/// The map from the device's native portrait frame (points) into the
/// interface's: for drawing hardware (the sensor housing) where it is.
pub fn from_portrait(emu: &Emu) -> [f64; 6] {
    current(emu).from_portrait(emu.device.width, emu.device.height)
}

// ---- which orientations are allowed ----

/// Info.plist's `UISupportedInterfaceOrientations` (the `~iphone` variant
/// first), as a mask; all but upside down when it doesn't say.
pub fn info_plist_mask(emu: &Emu) -> u64 {
    let info = crate::bundle::host_info_plist(emu);
    let dict = info.as_ref().and_then(|v| v.as_dictionary());
    let list = dict.and_then(|d| d.get("UISupportedInterfaceOrientations~iphone").or_else(|| d.get("UISupportedInterfaceOrientations")));
    let mask = list.and_then(|v| v.as_array()).map_or(0, |a| {
        a.iter().filter_map(|v| v.as_string()).filter_map(by_name).fold(0, |m, o| m | o.mask())
    });
    if mask == 0 { ALL_BUT_UPSIDE_DOWN } else { mask }
}

fn by_name(s: &str) -> Option<Orientation> {
    Some(match s {
        "UIInterfaceOrientationPortrait" => Orientation::Portrait,
        "UIInterfaceOrientationPortraitUpsideDown" => Orientation::PortraitUpsideDown,
        "UIInterfaceOrientationLandscapeLeft" => Orientation::LandscapeLeft,
        "UIInterfaceOrientationLandscapeRight" => Orientation::LandscapeRight,
        _ => return None,
    })
}

/// What the interface may turn to now: the app's orientations (its
/// delegate's, else Info.plist's) ∩ the root view controller's ∩ the
/// hardware's.
pub fn supported(emu: &mut Emu) -> Result<u64> {
    let window = emu.ui.key_window;
    let app = send(emu, emu.ui.app, "supportedInterfaceOrientationsForWindow:", &[window])?;
    let app = if emu.ui.app == 0 { info_plist_mask(emu) } else { app };
    let root = emu.ui.views.root_vc.get(&window).copied().unwrap_or(0);
    let vc = if root != 0 { send(emu, root, "supportedInterfaceOrientations", &[])? } else { u64::MAX };
    Ok(app & vc & emu.device.orientations())
}

/// Where the interface should be given `allowed`: where the app asked for
/// (`requestGeometryUpdate`, until the phone is turned) if allowed, else the
/// device's orientation if allowed (it was turned there), else where it is if
/// still allowed, else the first allowed in UIKit's order.
fn choose(emu: &Emu, allowed: u64) -> Option<Orientation> {
    if let Some(r) = emu.ui.requested_orientation
        && allowed & r.mask() != 0
    {
        return Some(r);
    }
    let device = emu.ui.device_orientation;
    if allowed & device.mask() != 0 {
        return Some(device);
    }
    if allowed & current(emu).mask() != 0 {
        return Some(current(emu));
    }
    PREFERENCE.into_iter().find(|o| allowed & o.mask() != 0)
}

// ---- launching and turning ----

/// At launch, before the app runs: the device is held `device`-wise; the
/// interface starts there if Info.plist allows it (the view controllers
/// have their say once they exist: see `settle`).
pub fn launch(emu: &mut Emu, device: Orientation) {
    emu.ui.device_orientation = device;
    let allowed = info_plist_mask(emu) & emu.device.orientations();
    emu.ui.orientation = choose(emu, allowed).unwrap_or(Orientation::Portrait);
}

/// Bring the interface to where the supported orientations say it should
/// be (after launch, `attemptRotationToDeviceOrientation`,
/// `setNeedsUpdateOfSupportedInterfaceOrientations`).
pub fn settle(emu: &mut Emu, animated: bool) -> Result<()> {
    let allowed = supported(emu)?;
    if let Some(o) = choose(emu, allowed) {
        turn_interface(emu, o, animated)?;
    }
    Ok(())
}

/// The user turns the phone: the device orientation changes, and the
/// interface follows if the app allows it.
pub fn turn_device(emu: &mut Emu, to: Orientation) -> Result<()> {
    if emu.ui.device_orientation == to {
        return Ok(());
    }
    emu.ui.device_orientation = to;
    // Turning the phone ends what the app asked for.
    emu.ui.requested_orientation = None;
    let device = crate::uikit::current_device_obj(emu)?;
    let yes = foundation::new_number_i64(emu, 1)?;
    let key = foundation::new_nsstring(emu, "UIDeviceOrientationRotateAnimatedUserInfoKey")?;
    let info = foundation::new_dict_of(emu, &[(key, yes)])?;
    foundation::post_named_info(emu, "UIDeviceOrientationDidChangeNotification", device, info)?;
    if supported(emu)? & to.mask() != 0 {
        turn_interface(emu, to, true)?;
    }
    Ok(())
}

/// Put the scripted turns (`--rotate`) on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.ui.rotations.len() {
        let delay = emu.ui.rotations[i].0.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, |e| {
            let to = e.ui.rotations[e.arg(1) as usize].1;
            turn_device(e, to)
        }, i as u64)?;
    }
    Ok(())
}

/// The state of a turn in progress: its coordinator's blocks.
#[derive(Clone, Default)]
pub struct Transition {
    coordinator: u64,
    alongside: Vec<u64>,
    completions: Vec<u64>,
}

/// Turn the interface, as UIKit does (order measured in the iOS 26 Simulator):
///  1. `willTransitionToTraitCollection:withTransitionCoordinator:` (if the
///     size classes change) and `viewWillTransitionToSize:…` to the root
///     view controllers, which pass them to their children; views are still
///     the old size;
///  2. UIApplicationWillChangeStatusBarOrientationNotification;
///  3. the screen, windows and root views take the new size; layout margins
///     and safe areas change (their callbacks);
///  4. UIApplicationDidChangeStatusBarOrientation/FrameNotification;
///  5. `traitCollectionDidChange:` (if they changed);
///  6. layout at the new size;
///  7. the coordinator's alongside-animation blocks; the scene delegate's
///     `windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:`;
///  8. the coordinator's completions, TRANSITION_SECONDS later.
pub fn turn_interface(emu: &mut Emu, to: Orientation, animated: bool) -> Result<()> {
    let from = current(emu);
    if from == to {
        return Ok(());
    }
    let (old_classes, new_classes) = (emu.device.size_classes(from), emu.device.size_classes(to));
    let traits_changed = old_classes != new_classes;
    let old_traits = crate::uiview::new_traits(emu, old_classes)?;
    let new_traits = crate::uiview::new_traits(emu, new_classes)?;
    let old_screen = screen(emu);
    let new_screen = emu.device.screen(to);
    let old_status_bar = status_bar_frame(emu);
    let old_space = fixed_space(emu, old_screen)?;

    let coordinator = new_coordinator(emu, from, animated)?;
    let roots = crate::uikit::window_roots(emu);
    if traits_changed {
        for &vc in &roots {
            send(emu, vc, "willTransitionToTraitCollection:withTransitionCoordinator:", &[new_traits, coordinator])?;
        }
    }
    for &vc in &roots {
        send_fp(emu, vc, "viewWillTransitionToSize:withTransitionCoordinator:", &[coordinator], &[new_screen.0, new_screen.1])?;
    }
    let app = emu.ui.app;
    let to_number = foundation::new_number_i64(emu, to as i64)?;
    post_with(emu, "UIApplicationWillChangeStatusBarOrientationNotification", app, "UIApplicationStatusBarOrientationUserInfoKey", to_number)?;

    emu.ui.orientation = to;
    for w in emu.ui.windows.clone() {
        let f = crate::uiview::view(emu, w).bounds;
        // Full-screen windows (all of an iPhone app's) turn with the screen.
        if (f[2], f[3]) == old_screen {
            crate::uiview::set_frame_of(emu, w, [0.0, 0.0, new_screen.0, new_screen.1]);
        }
        if let Some(&vc) = emu.ui.views.root_vc.get(&w)
            && let Some(&v) = emu.ui.views.vc_view.get(&vc)
        {
            crate::uiview::set_frame_of(emu, v, [0.0, 0.0, new_screen.0, new_screen.1]);
        }
    }
    crate::autolayout::mark_dirty(emu);
    crate::autolayout::safe_area_changes(emu)?;

    let from_number = foundation::new_number_i64(emu, from as i64)?;
    post_with(emu, "UIApplicationDidChangeStatusBarOrientationNotification", app, "UIApplicationStatusBarOrientationUserInfoKey", from_number)?;
    let frame = crate::nsvalue::new_rect_value(emu, old_status_bar)?;
    post_with(emu, "UIApplicationDidChangeStatusBarFrameNotification", app, "UIApplicationStatusBarFrameUserInfoKey", frame)?;
    if traits_changed {
        traits_did_change(emu, old_traits)?;
    }
    crate::textinput::keyboard_reoriented(emu, old_screen)?;
    crate::uiview::layout_windows(emu)?;

    let blocks = std::mem::take(&mut emu.ui.transition.alongside);
    for b in blocks {
        crate::libsystem::call_block(emu, b, &[coordinator])?;
    }
    let scene_delegate = emu.ui.scene_delegate;
    let sel = objc::intern(emu, "windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:")?;
    if scene_delegate != 0 && objc::responds(emu, scene_delegate, sel)? {
        let scene = emu.ui.scene;
        send(emu, scene_delegate, "windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:", &[scene, old_space, from as u64, old_traits])?;
    }
    crate::host::frame_changed(emu);
    let delay = if animated { TRANSITION_SECONDS } else { 0.0 };
    crate::uikit::after_on_main(emu, delay, finish_transition, coordinator)
}

/// The coordinator's completions, once the turn is over.
fn finish_transition(emu: &mut Emu) -> Result<()> {
    let coordinator = emu.arg(1);
    if emu.ui.transition.coordinator != coordinator {
        return Ok(());
    }
    let t = std::mem::take(&mut emu.ui.transition);
    for b in t.completions {
        crate::libsystem::call_block(emu, b, &[coordinator])?;
    }
    Ok(())
}

fn post_with(emu: &mut Emu, name: &str, object: u64, key: &str, value: u64) -> Result<()> {
    let k = foundation::new_nsstring(emu, key)?;
    let info = foundation::new_dict_of(emu, &[(k, value)])?;
    foundation::post_named_info(emu, name, object, info)
}

/// `traitCollectionDidChange:` to every view controller and view that
/// implements it, parents first.
pub(crate) fn traits_did_change(emu: &mut Emu, previous: u64) -> Result<()> {
    let sel = objc::intern(emu, "traitCollectionDidChange:")?;
    let mut todo: Vec<u64> = emu.ui.windows.iter().rev().copied().collect();
    while let Some(v) = todo.pop() {
        let vc = emu.ui.views.vc_view.iter().find(|&(_, &x)| x == v).map(|(&vc, _)| vc);
        if let Some(vc) = vc
            && overrides(emu, vc, sel)?
        {
            send(emu, vc, "traitCollectionDidChange:", &[previous])?;
        }
        if overrides(emu, v, sel)? {
            send(emu, v, "traitCollectionDidChange:", &[previous])?;
        }
        todo.extend(crate::uiview::view(emu, v).subviews.iter().rev());
    }
    notify_trait_registrations(emu, previous)
}

/// iOS 17's trait change registrations: a view's or controller's handler
/// (or target and action) for the traits it named, called with it and the
/// traits before when one of them changes: the appearance
/// (UITraitUserInterfaceStyle) or a size class (the phone turned). A
/// registration's token is what `unregisterForTraitChanges:` takes.
fn register_for_trait_changes(emu: &mut Emu, handler: u64, target: u64, action: u64) -> Result<()> {
    let (env, traits) = (emu.arg(0), emu.arg(2));
    let object = objc::host_class(emu, "NSObject")?;
    let token = objc::create_instance(emu, object, 0)?;
    objc::bag_set(emu, token, "pommeEnvironment", env);
    let traits = if traits == 0 { 0 } else { send(emu, traits, "copy", &[])? };
    objc::bag_set(emu, token, "pommeTraits", traits);
    let handler = if handler == 0 { 0 } else { send(emu, handler, "copy", &[])? };
    objc::bag_set(emu, token, "pommeHandler", handler);
    objc::bag_set(emu, token, "pommeTarget", target);
    objc::bag_set(emu, token, "pommeAction", action);
    emu.ui.trait_registrations.push(token);
    emu.ret(token);
    Ok(())
}

/// Call the registrations for traits that differ between `previous` and now.
fn notify_trait_registrations(emu: &mut Emu, previous: u64) -> Result<()> {
    let now = crate::uiview::current_traits(emu)?;
    let get = |emu: &Emu, t: u64, k: &str| objc::bag_get(emu, t, k);
    let mut changed = Vec::new();
    for (key, trait_class) in [("userInterfaceStyle", "UITraitUserInterfaceStyle"), ("horizontalSizeClass", "UITraitHorizontalSizeClass"), ("verticalSizeClass", "UITraitVerticalSizeClass")] {
        if get(emu, previous, key) != get(emu, now, key) {
            changed.push(trait_class);
        }
    }
    if changed.is_empty() {
        return Ok(());
    }
    for token in emu.ui.trait_registrations.clone() {
        let traits = objc::bag_get(emu, token, "pommeTraits").unwrap_or(0);
        let named = if traits == 0 { Vec::new() } else { foundation::array_values(emu, traits)? };
        if !named.iter().any(|&c| changed.contains(&emu.objc.class_name(c).as_str())) {
            continue;
        }
        let env = objc::bag_get(emu, token, "pommeEnvironment").unwrap_or(0);
        let handler = objc::bag_get(emu, token, "pommeHandler").unwrap_or(0);
        if handler != 0 {
            crate::libsystem::call_block(emu, handler, &[env, previous])?;
        } else if let (Some(target), Some(action)) = (objc::bag_get(emu, token, "pommeTarget").filter(|&t| t != 0), objc::bag_get(emu, token, "pommeAction").filter(|&a| a != 0)) {
            let name = objc::sel_name(emu, action);
            send(emu, target, &name, &[env, previous])?;
        }
    }
    Ok(())
}

/// Whether the app's class implements `sel` itself (pomme's defaults do nothing).
fn overrides(emu: &mut Emu, obj: u64, sel: u64) -> Result<bool> {
    let cls = objc::class_of(emu, obj)?;
    Ok(matches!(objc::lookup(emu, cls, sel)?, Some(objc::Imp::Guest(_))))
}

// ---- the transition coordinator ----

const COORDINATOR: &str = "_UIViewControllerTransitionCoordinator";

fn new_coordinator(emu: &mut Emu, from: Orientation, animated: bool) -> Result<u64> {
    let cls = objc::host_class(emu, COORDINATOR)?;
    let c = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, c, "animated", animated as u64);
    objc::bag_set(emu, c, "from", from as u64);
    emu.ui.transition = Transition { coordinator: c, ..Default::default() };
    Ok(c)
}

fn is_animated(emu: &Emu, c: u64) -> bool {
    objc::bag_get(emu, c, "animated").unwrap_or(0) != 0
}

/// `animateAlongsideTransition:completion:` (and the …InView: form, block args at `first`).
fn animate_alongside(emu: &mut Emu, first: usize) -> Result<()> {
    let (animation, completion) = (emu.arg(first), emu.arg(first + 1));
    let current = emu.ui.transition.coordinator == emu.arg(0);
    if current {
        if animation != 0 {
            let b = crate::libc::copy_block(emu, animation)?;
            emu.ui.transition.alongside.push(b);
        }
        if completion != 0 {
            let b = crate::libc::copy_block(emu, completion)?;
            emu.ui.transition.completions.push(b);
        }
    }
    emu.ret(current as u64);
    Ok(())
}

/// The rotation the coordinator's `targetTransform` reports: the turn of the
/// interface relative to where it was.
fn target_transform(emu: &mut Emu) -> Result<()> {
    // Not measured: the angle between the old and new frames' maps from the
    // hardware, in UIKit's y-down sense.
    let angle = |o: Orientation| {
        let m = o.from_portrait(1.0, 1.0);
        m[1].atan2(m[0])
    };
    let (from, to) = (objc::bag_get(emu, emu.arg(0), "from").map(|v| v as u64), current(emu));
    let from = from.and_then(Orientation::from_raw).unwrap_or(to);
    let a = angle(from) - angle(to);
    let (c, s) = (a.cos().round(), a.sin().round());
    // CGAffineTransform is returned indirectly (x8).
    let out = emu.cpu.x[8];
    for (i, v) in [c, s, -s, c, 0.0, 0.0].into_iter().enumerate() {
        emu.write_u64(out + 8 * i as u64, v.to_bits())?;
    }
    Ok(())
}

/// A coordinate space fixed at `size` (the scene's, before it turned).
fn fixed_space(emu: &mut Emu, size: (f64, f64)) -> Result<u64> {
    let cls = objc::host_class(emu, "_UIFixedCoordinateSpace")?;
    let s = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, s, "width", size.0.to_bits());
    objc::bag_set(emu, s, "height", size.1.to_bits());
    Ok(s)
}

fn ret_f64(emu: &mut Emu, v: f64) -> Result<()> {
    crate::uikit::ret_f64(emu, v);
    Ok(())
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// Pass a transition message on to the receiver's child view controllers (UIViewController's default).
fn to_children(emu: &mut Emu, sel: &str, fargs: &[f64]) -> Result<()> {
    let vc = emu.arg(0);
    let args = [emu.arg(2), emu.arg(3)];
    let children = emu.nav.children.get(&vc).cloned().unwrap_or_default();
    for c in children {
        if fargs.is_empty() {
            send(emu, c, sel, &args)?;
        } else {
            send_fp(emu, c, sel, &args[..1], fargs)?;
        }
    }
    Ok(())
}

/// `requestGeometryUpdateWithPreferences:errorHandler:`: turn to one of the
/// preferred orientations the app supports (the device stays as it's held);
/// none supported → the error handler.
fn request_geometry_update(emu: &mut Emu) -> Result<()> {
    let (prefs, handler) = (emu.arg(2), emu.arg(3));
    let wanted = objc::bag_get(emu, prefs, "interfaceOrientations").unwrap_or(0);
    let allowed = supported(emu)? & wanted;
    let target = if allowed & current(emu).mask() != 0 { Some(current(emu)) } else { PREFERENCE.into_iter().find(|o| allowed & o.mask() != 0) };
    match target {
        Some(o) => {
            emu.ui.requested_orientation = Some(o);
            turn_interface(emu, o, true)
        }
        None if handler != 0 => {
            // UISceneErrorDomain, UISceneErrorCodeGeometryRequestUnsupported (101).
            let domain = foundation::new_nsstring(emu, "UISceneErrorDomain")?;
            let cls = objc::host_class(emu, "NSError")?;
            let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, 101, 0])?;
            crate::libsystem::call_block(emu, handler, &[err]).map(|_| ())
        }
        None => Ok(()),
    }
}

const VC: &str = "UIViewController";
const T: &str = COORDINATOR;

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIView", false, "registerForTraitChanges:withHandler:", |e| {
        let h = e.arg(3);
        register_for_trait_changes(e, h, 0, 0)
    }),
    ("UIView", false, "registerForTraitChanges:withTarget:action:", |e| {
        let (t, a) = (e.arg(3), e.arg(4));
        register_for_trait_changes(e, 0, t, a)
    }),
    ("UIView", false, "registerForTraitChanges:withAction:", |e| {
        let (env, a) = (e.arg(0), e.arg(3));
        register_for_trait_changes(e, 0, env, a)
    }),
    ("UIView", false, "unregisterForTraitChanges:", |e| {
        let token = e.arg(2);
        e.ui.trait_registrations.retain(|&t| t != token);
        Ok(())
    }),
    (VC, false, "registerForTraitChanges:withHandler:", |e| {
        let h = e.arg(3);
        register_for_trait_changes(e, h, 0, 0)
    }),
    (VC, false, "registerForTraitChanges:withTarget:action:", |e| {
        let (t, a) = (e.arg(3), e.arg(4));
        register_for_trait_changes(e, 0, t, a)
    }),
    (VC, false, "registerForTraitChanges:withAction:", |e| {
        let (env, a) = (e.arg(0), e.arg(3));
        register_for_trait_changes(e, 0, env, a)
    }),
    (VC, false, "unregisterForTraitChanges:", |e| {
        let token = e.arg(2);
        e.ui.trait_registrations.retain(|&t| t != token);
        Ok(())
    }),
    ("UIApplication", false, "supportedInterfaceOrientationsForWindow:", |e| {
        let (delegate, window) = (e.ui.delegate, e.arg(2));
        let sel = objc::intern(e, "application:supportedInterfaceOrientationsForWindow:")?;
        let m = if delegate != 0 && objc::responds(e, delegate, sel)? {
            let app = e.arg(0);
            send(e, delegate, "application:supportedInterfaceOrientationsForWindow:", &[app, window])?
        } else {
            info_plist_mask(e)
        };
        ret(e, m)
    }),
    ("UIApplication", false, "isStatusBarHidden", |e| ret(e, (status_bar_frame(e)[3] == 0.0) as u64)),
    ("UIStatusBarManager", false, "isStatusBarHidden", |e| ret(e, (status_bar_frame(e)[3] == 0.0) as u64)),
    ("UIDevice", false, "orientation", |e| ret(e, e.ui.device_orientation as u64)),
    ("UIDevice", false, "isGeneratingDeviceOrientationNotifications", |e| ret(e, 1)),
    ("UIDevice", false, "beginGeneratingDeviceOrientationNotifications", |_| Ok(())),
    ("UIDevice", false, "endGeneratingDeviceOrientationNotifications", |_| Ok(())),
    (VC, false, "supportedInterfaceOrientations", |e| ret(e, ALL_BUT_UPSIDE_DOWN)),
    (VC, false, "preferredInterfaceOrientationForPresentation", |e| ret(e, current(e) as u64)),
    (VC, false, "shouldAutorotate", |e| ret(e, 1)),
    (VC, false, "interfaceOrientation", |e| ret(e, current(e) as u64)),
    (VC, false, "setNeedsUpdateOfSupportedInterfaceOrientations", |e| settle(e, true)),
    (VC, true, "attemptRotationToDeviceOrientation", |e| settle(e, true)),
    (VC, false, "viewWillTransitionToSize:withTransitionCoordinator:", |e| {
        let size = [f64::from_bits(e.cpu.q[0] as u64), f64::from_bits(e.cpu.q[1] as u64)];
        to_children(e, "viewWillTransitionToSize:withTransitionCoordinator:", &size)
    }),
    (VC, false, "willTransitionToTraitCollection:withTransitionCoordinator:", |e| to_children(e, "willTransitionToTraitCollection:withTransitionCoordinator:", &[])),
    (VC, false, "traitCollectionDidChange:", |_| Ok(())),
    ("UIView", false, "traitCollectionDidChange:", |_| Ok(())),
    ("UIWindowScene", false, "requestGeometryUpdateWithPreferences:errorHandler:", request_geometry_update),
    ("UIWindowScene", false, "effectiveGeometry", |e| {
        let cls = objc::host_class(e, "UIWindowSceneGeometry")?;
        let g = objc::create_instance(e, cls, 0)?;
        objc::bag_set(e, g, "interfaceOrientation", current(e) as u64);
        ret(e, g)
    }),
    ("UIWindowScene", false, "coordinateSpace", |e| {
        let s = crate::uikit::main_screen_obj(e)?;
        ret(e, s)
    }),
    ("UIWindowSceneGeometry", false, "interfaceOrientation", |e| {
        let o = objc::bag_get(e, e.arg(0), "interfaceOrientation").unwrap_or(current(e) as u64);
        ret(e, o)
    }),
    ("UIWindowSceneGeometry", false, "coordinateSpace", |e| {
        let s = crate::uikit::main_screen_obj(e)?;
        ret(e, s)
    }),
    ("UIWindowSceneGeometryPreferencesIOS", false, "initWithInterfaceOrientations:", |e| {
        let (p, m) = (e.arg(0), e.arg(2));
        objc::bag_set(e, p, "interfaceOrientations", m);
        ret(e, p)
    }),
    ("UIWindowSceneGeometryPreferencesIOS", false, "interfaceOrientations", |e| {
        let m = objc::bag_get(e, e.arg(0), "interfaceOrientations").unwrap_or(0);
        ret(e, m)
    }),
    ("UIWindowSceneGeometryPreferencesIOS", false, "setInterfaceOrientations:", |e| {
        let (p, m) = (e.arg(0), e.arg(2));
        objc::bag_set(e, p, "interfaceOrientations", m);
        Ok(())
    }),
    ("_UIFixedCoordinateSpace", false, "bounds", |e| {
        let o = e.arg(0);
        let (w, h) = (objc::bag_get(e, o, "width").unwrap_or(0), objc::bag_get(e, o, "height").unwrap_or(0));
        crate::uikit::ret_rect(e, [0.0, 0.0, f64::from_bits(w), f64::from_bits(h)]);
        Ok(())
    }),
    (T, false, "animateAlongsideTransition:completion:", |e| animate_alongside(e, 2)),
    (T, false, "animateAlongsideTransitionInView:animation:completion:", |e| animate_alongside(e, 3)),
    (T, false, "notifyWhenInteractionChangesUsingBlock:", |_| Ok(())),
    (T, false, "notifyWhenInteractionEndsUsingBlock:", |_| Ok(())),
    (T, false, "transitionDuration", |e| {
        let animated = is_animated(e, e.arg(0));
        ret_f64(e, if animated { TRANSITION_SECONDS } else { 0.0 })
    }),
    (T, false, "isAnimated", |e| ret(e, is_animated(e, e.arg(0)) as u64)),
    (T, false, "isInteractive", |e| ret(e, 0)),
    (T, false, "initiallyInteractive", |e| ret(e, 0)),
    (T, false, "isInterruptible", |e| ret(e, 0)),
    (T, false, "isCancelled", |e| ret(e, 0)),
    (T, false, "percentComplete", |e| ret_f64(e, 0.0)),
    (T, false, "completionVelocity", |e| ret_f64(e, 1.0)),
    (T, false, "completionCurve", |e| ret(e, 0)), // UIViewAnimationCurveEaseInOut
    (T, false, "presentationStyle", |e| ret(e, u64::MAX)), // UIModalPresentationNone (-1)
    (T, false, "targetTransform", target_transform),
    (T, false, "containerView", |e| ret(e, e.ui.key_window)),
    (T, false, "viewControllerForKey:", |e| ret(e, 0)),
    (T, false, "viewForKey:", |e| ret(e, 0)),
];
