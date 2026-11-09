//! pomme in a desktop window, looking like the iOS Simulator: the app on a
//! phone, live. Clicks and drags on the screen are touches, typing goes to the
//! focused text input (as a hardware keyboard does in the Simulator),
//! ⌘← / ⌘→ (Ctrl on Windows and Linux) turn the phone, and the Simulator's
//! Face ID shortcuts work: ⌥⌘M a matching face, ⌥⌘N one that doesn't match,
//! and ⌥⌘E turns enrolment on or off. They're menus too, as the Simulator's
//! (Device, Features ▸ Face ID): see menu.rs.
//!
//! The emulator runs on its own thread with a [`DesktopHost`]: it paces
//! virtual time against the wall clock while the app idles, and takes the
//! window's input then. The window (winit, on the main thread as macOS
//! requires) is the phone itself, as the Simulator's is: borderless, its
//! rounded corners see-through (wgpu, see present.rs), dragged by its bezel,
//! closed with ⌘W / ⌘Q (Ctrl elsewhere). It runs wherever winit and wgpu
//! do: macOS, Windows, Linux (X11 and Wayland); where windows can't be
//! transparent the phone sits on a dark backdrop.

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), path = "menu_none.rs")]
mod menu;
mod present;

use pomme_hle::device::{Device, Orientation};
use pomme_hle::host::{Event, Frame, Host, Idle};
use pomme_hle::keychain::Face;
use pomme_hle::touch::{BEGAN, ENDED, Input, MOVED};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

/// What the window shows and how.
pub struct Config {
    pub device: &'static Device,
    /// How the phone is held at launch.
    pub orientation: Orientation,
    /// The window's title (the Simulator shows the device and iOS version).
    pub title: String,
    /// Face ID starts enrolled (the Features ▸ Face ID ▸ Enrolled check).
    pub enrolled: bool,
    /// Print frame statistics each second frames arrive (see [`FrameStats`]).
    pub frame_stats: bool,
}

/// How smoothly frames arrive: per second of wall time, how many, how much
/// of the app's (virtual) time they covered, and the longest wall-clock gap
/// between consecutive frames of motion.
#[derive(Default)]
pub struct FrameStats {
    since: Option<Instant>,
    count: u32,
    first_virtual: u64,
    last_virtual: u64,
    last: Option<Instant>,
    longest: Duration,
}

impl FrameStats {
    fn frame(&mut self, virtual_ns: u64) {
        let now = Instant::now();
        let since = *self.since.get_or_insert(now);
        if self.count == 0 {
            self.first_virtual = virtual_ns;
        }
        // A gap counts only between consecutive frames of motion (no more
        // than ~1.5 frames apart in the app's time): idle stretches with
        // nothing to draw aren't stutters.
        if let Some(last) = self.last
            && virtual_ns.saturating_sub(self.last_virtual) <= 50_000_000
        {
            self.longest = self.longest.max(now - last);
        }
        self.last = Some(now);
        self.count += 1;
        self.last_virtual = virtual_ns;
        if now - since >= Duration::from_secs(1) {
            let wall = (now - since).as_secs_f64();
            let app = (self.last_virtual - self.first_virtual) as f64 / 1e9;
            eprintln!(
                "frames: {:.0}/s, {:.2} s of the app's time in {wall:.2} s, longest gap {:.0} ms",
                self.count as f64 / wall,
                app,
                self.longest.as_secs_f64() * 1000.0
            );
            *self = FrameStats::default();
        }
    }
}

/// What a menu item or shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
enum Action {
    /// Turn the phone a quarter: left (true) or right.
    Rotate(bool),
    Face(Face),
    /// Close the window (and stop the run).
    Close,
}

/// The emulator thread's side: a frame for the window, or the run ended.
enum ToWindow {
    Frame,
    Ended,
    /// A menu item was chosen.
    Menu(Action),
    /// A Face ID prompt is waiting (true) or was answered (false).
    FaceId(bool),
}

/// The latest frame from the emulator, waiting for the window.
struct LatestFrame {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    orientation: Orientation,
}

/// How far the app's clock may fall behind the wall clock before the time is
/// dropped rather than caught up (about one frame).
const MAX_BEHIND: Duration = Duration::from_millis(40);

/// The emulator's host: input from the window, frames to it.
pub struct DesktopHost {
    events: Receiver<Event>,
    frame: Arc<Mutex<Option<LatestFrame>>>,
    proxy: EventLoopProxy<ToWindow>,
    /// wall − virtual time, fixed when the app first idles.
    base: Option<Instant>,
    closed: Arc<std::sync::atomic::AtomicBool>,
    stats: Option<FrameStats>,
}

impl Host for DesktopHost {
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> Idle {
        use std::sync::atomic::Ordering;
        let virt = Duration::from_nanos(now_ns);
        let base = *self.base.get_or_insert_with(|| Instant::now() - virt);
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Idle::Quit;
            }
            let wall_virt = Instant::now().saturating_duration_since(base);
            let mut events: Vec<Event> = self.events.try_iter().collect();
            if !events.is_empty() {
                // Input happens now: no earlier than the app's own clock.
                let at = wall_virt.max(virt).as_nanos() as u64;
                for e in &mut events {
                    if let Event::Touch(i) = e {
                        i.at_ns = at;
                    }
                }
                return Idle::Input { at_ns: at, events };
            }
            let wait = match next_ns {
                Some(n) => Duration::from_nanos(n).checked_sub(wall_virt),
                None => Some(Duration::from_millis(250)),
            };
            let Some(wait) = wait.filter(|w| !w.is_zero()) else {
                // Due, or overdue because the app was busy. More than a frame
                // behind, the lost time is dropped rather than raced through
                // (racing plays animations fast): the clock is set so that
                // the moment it advances to is now (setting it to the current
                // moment instead would leave it ahead once it jumps, and stall).
                let next = Duration::from_nanos(next_ns.unwrap_or(now_ns));
                if wall_virt > next + MAX_BEHIND {
                    self.base = Some(Instant::now() - next);
                }
                return Idle::Advance;
            };
            match self.events.recv_timeout(wait.min(Duration::from_millis(250))) {
                Ok(e) => {
                    let at = Instant::now().saturating_duration_since(base).max(virt).as_nanos() as u64;
                    let mut events = vec![e];
                    events.extend(self.events.try_iter());
                    for e in &mut events {
                        if let Event::Touch(i) = e {
                            i.at_ns = at;
                        }
                    }
                    return Idle::Input { at_ns: at, events };
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Idle::Quit,
            }
        }
    }

    fn biometry_prompt(&mut self, reason: &str) {
        let reason = if reason.is_empty() { String::new() } else { format!(" ({reason})") };
        eprintln!("Face ID is asking{reason}: Features ▸ Face ID ▸ Matching Face (⌥⌘M) or Non-matching Face (⌥⌘N)");
        let _ = self.proxy.send_event(ToWindow::FaceId(true));
    }

    fn wall_ns(&mut self) -> Option<u64> {
        self.base.map(|b| Instant::now().saturating_duration_since(b).as_nanos() as u64)
    }

    fn biometry_answered(&mut self) {
        let _ = self.proxy.send_event(ToWindow::FaceId(false));
    }

    fn frame(&mut self, f: Frame<'_>) {
        if let Some(s) = self.stats.as_mut() {
            s.frame(f.at_ns);
        }
        *self.frame.lock().unwrap() = Some(LatestFrame { rgba: f.rgba.to_vec(), width: f.width, height: f.height, orientation: f.orientation });
        let _ = self.proxy.send_event(ToWindow::Frame);
    }
}

/// Open the window and run `app` (which sets up the emulator, installs the
/// host and runs it) on its own thread, with frames rendered at `scale`
/// pixels per point. Returns when the window closes.
pub fn launch(config: Config, app: impl FnOnce(Box<dyn Host>, f32) + Send + 'static) -> Result<(), String> {
    let event_loop = EventLoop::<ToWindow>::with_user_event().build().map_err(|e| e.to_string())?;
    let held = config.orientation;
    let mut window = App {
        held,
        config,
        app: Some(Box::new(app)),
        proxy: event_loop.create_proxy(),
        window: None,
        presenter: None,
        events: None,
        frame: Arc::new(Mutex::new(None)),
        closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        phone: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        touching: false,
        modifiers: ModifiersState::empty(),
        ended: false,
        menus: None,
        shell: None,
        face_id: false,
    };
    event_loop.run_app(&mut window).map_err(|e| e.to_string())
}

type AppFn = Box<dyn FnOnce(Box<dyn Host>, f32) + Send>;

/// The phone as last drawn: the picture, the screen's place in it, and the frame's scale.
struct Phone {
    pix: Pixmap,
    screen: [f32; 4],
    scale: f32,
}

struct App {
    config: Config,
    app: Option<AppFn>,
    proxy: EventLoopProxy<ToWindow>,
    window: Option<Arc<Window>>,
    presenter: Option<present::Presenter>,
    events: Option<Sender<Event>>,
    frame: Arc<Mutex<Option<LatestFrame>>>,
    closed: Arc<std::sync::atomic::AtomicBool>,
    phone: Option<Phone>,
    cursor: PhysicalPosition<f64>,
    touching: bool,
    modifiers: ModifiersState,
    /// The run is over (the app exited or stopped); the last frame stays up.
    ended: bool,
    /// How the phone is held now.
    held: Orientation,
    menus: Option<menu::Menus>,
    /// A Face ID prompt is waiting: the sheet is up.
    face_id: bool,
    /// The phone's body for the last (orientation, scale bits) a frame came in.
    shell: Option<((Orientation, u32), pomme_hle::render::Shell)>,
}

impl App {
    /// The phone at this frame scale with a blank screen: the window's size before the first frame.
    fn blank_phone(&mut self, scale: f32) -> Option<Phone> {
        let shell = self.shell(self.held, scale)?;
        Some(Phone { pix: shell.blank(), screen: shell.screen, scale })
    }

    /// The phone's body for this orientation and scale, drawn once and kept.
    fn shell(&mut self, o: Orientation, scale: f32) -> Option<&pomme_hle::render::Shell> {
        if self.shell.as_ref().is_none_or(|(k, _)| *k != (o, scale.to_bits())) {
            let shell = pomme_hle::render::Shell::new(self.config.device, o, scale).ok()?;
            self.shell = Some(((o, scale.to_bits()), shell));
        }
        self.shell.as_ref().map(|(_, s)| s)
    }

    /// Fit the phone in the window: the transform from phone pixels to window pixels.
    fn fit(&self, phone: &Phone) -> Option<(f32, f32, f32)> {
        let size = self.window.as_ref()?.inner_size();
        let (ww, wh) = (size.width as f32, size.height as f32);
        let (pw, ph) = (phone.pix.width() as f32, phone.pix.height() as f32);
        let k = (ww / pw).min(wh / ph);
        Some((k, (ww - pw * k) / 2.0, (wh - ph * k) / 2.0))
    }

    /// The window position as a point on the app's screen, if it's on the screen.
    fn point_at(&self, p: PhysicalPosition<f64>) -> Option<(f64, f64)> {
        let phone = self.phone.as_ref()?;
        let fit = self.fit(phone)?;
        screen_point(fit, phone.screen, phone.scale, (p.x as f32, p.y as f32), false)
    }

    fn send(&self, e: Event) {
        if let Some(tx) = &self.events {
            let _ = tx.send(e);
        }
    }

    fn touch(&mut self, phase: u64, p: (f64, f64)) {
        self.send(Event::Touch(Input { at_ns: 0, phase, x: p.0, y: p.1 }));
    }

    /// Size the window to the phone (in points) when it turns or first appears.
    fn size_window_to(&self, phone: &Phone) {
        let Some(w) = &self.window else { return };
        let size = LogicalSize::new(phone.pix.width() as f64 / phone.scale as f64, phone.pix.height() as f64 / phone.scale as f64);
        let _ = w.request_inner_size(size);
    }

    fn take_frame(&mut self) {
        let Some(f) = self.frame.lock().unwrap().take() else { return };
        let Some(scale) = self.phone.as_ref().map(|p| p.scale) else { return };
        let Some(screen) = Pixmap::from_vec(f.rgba, tiny_skia::IntSize::from_wh(f.width, f.height).unwrap()) else { return };
        let Some(shell) = self.shell(f.orientation, scale) else { return };
        let (pix, rect) = (shell.compose(&screen), shell.screen);
        let turned = self.phone.as_ref().is_some_and(|p| (p.pix.width(), p.pix.height()) != (pix.width(), pix.height()));
        self.phone = Some(Phone { pix, screen: rect, scale });
        if turned {
            self.size_window_to(self.phone.as_ref().unwrap());
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(phone) = self.phone.as_ref() else { return };
        let Some(fit) = self.fit(phone) else { return };
        // The system's Face ID sheet over the app while a prompt waits, as on iOS.
        let sheet = self.face_id.then(|| {
            let mut pix = phone.pix.clone();
            face_id_sheet(&mut pix, phone.screen, phone.scale);
            pix
        });
        let pix = sheet.as_ref().unwrap_or(&phone.pix);
        let (data, w, h) = (pix.data(), pix.width(), pix.height());
        if let Some(p) = self.presenter.as_mut() {
            p.present(data, w, h, fit);
        }
    }

    /// Whether a window position is on the phone (its body or screen), not the see-through corners.
    fn on_phone(&self, p: PhysicalPosition<f64>) -> bool {
        let Some(phone) = self.phone.as_ref() else { return false };
        let Some((k, dx, dy)) = self.fit(phone) else { return false };
        let (x, y) = (((p.x as f32 - dx) / k) as i64, ((p.y as f32 - dy) / k) as i64);
        if x < 0 || y < 0 {
            return false;
        }
        phone.pix.pixel(x as u32, y as u32).is_some_and(|c| c.alpha() > 0)
    }

    /// ⌘← / ⌘→ (Ctrl elsewhere): turn the phone a quarter, as the Simulator's Rotate Left / Right.
    fn rotate(&mut self, left: bool) {
        self.held = turned(self.held, left);
        self.send(Event::Rotate(self.held));
    }

    fn act(&mut self, event_loop: &ActiveEventLoop, a: Action) {
        match a {
            Action::Rotate(left) => self.rotate(left),
            Action::Face(f) => self.send(Event::Face(f)),
            Action::Close => self.close(event_loop),
        }
    }

    fn close(&mut self, event_loop: &ActiveEventLoop) {
        self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
        self.events = None; // the host sees the channel close
        event_loop.exit();
    }
}

/// iOS's Face ID sheet, centred on the screen (`screen`: x, y, w, h in
/// pixels at `scale` pixels per point): a dark rounded square with the Face
/// ID glyph, shown while the system waits for a face.
fn face_id_sheet(pix: &mut Pixmap, screen: [f32; 4], scale: f32) {
    use tiny_skia::{LineCap, Paint, PathBuilder, Stroke, Transform};
    let [sx, sy, sw, sh] = screen;
    let (cx, cy) = (sx + sw / 2.0, sy + sh / 2.0);
    // Points to pixels, around the centre.
    let t = Transform::from_row(scale, 0.0, 0.0, scale, cx, cy);
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(28, 28, 30, 225);
    // A rounded square (radius 30 pt).
    let mut pb = PathBuilder::new();
    let (r, h) = (30.0, 80.0);
    pb.move_to(-h + r, -h);
    pb.line_to(h - r, -h);
    pb.quad_to(h, -h, h, -h + r);
    pb.line_to(h, h - r);
    pb.quad_to(h, h, h - r, h);
    pb.line_to(-h + r, h);
    pb.quad_to(-h, h, -h, h - r);
    pb.line_to(-h, -h + r);
    pb.quad_to(-h, -h, -h + r, -h);
    pb.close();
    if let Some(path) = pb.finish() {
        pix.fill_path(&path, &paint, tiny_skia::FillRule::Winding, t, None);
    }
    // The glyph: four corner brackets, eyes, nose and smile (white strokes).
    paint.set_color_rgba8(255, 255, 255, 235);
    let stroke = Stroke { width: 4.0, line_cap: LineCap::Round, ..Default::default() };
    let mut g = PathBuilder::new();
    let (e, l) = (32.0f32, 13.0f32); // bracket corner and arm length
    for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        g.move_to(dx * e, dy * (e - l));
        g.quad_to(dx * e, dy * e, dx * (e - l), dy * e);
    }
    for x in [-11.0f32, 11.0] {
        g.move_to(x, -10.0);
        g.line_to(x, -4.0);
    }
    g.move_to(1.0, -9.0);
    g.line_to(1.0, 5.0);
    g.line_to(-3.0, 5.0);
    g.move_to(-11.0, 13.0);
    g.quad_to(0.0, 21.0, 11.0, 13.0);
    if let Some(path) = g.finish() {
        pix.stroke_path(&path, &paint, &stroke, t, None);
    }
}

/// The Simulator's Features ▸ Face ID shortcuts (with ⌥⌘): M a matching face,
/// N a non-matching one; E (not one of the Simulator's) toggles Enrolled.
fn face_shortcut(code: KeyCode) -> Option<Face> {
    match code {
        KeyCode::KeyM => Some(Face::Matching),
        KeyCode::KeyN => Some(Face::NonMatching),
        KeyCode::KeyE => Some(Face::ToggleEnrolled),
        _ => None,
    }
}

/// How the phone is held after a quarter turn to the left (counter-clockwise) or right.
/// Turning left from portrait gives what UIKit calls (interface) LandscapeRight.
fn turned(o: Orientation, left: bool) -> Orientation {
    use Orientation::*;
    const ROUND: [Orientation; 4] = [Portrait, LandscapeRight, PortraitUpsideDown, LandscapeLeft];
    let i = ROUND.iter().position(|&r| r == o).unwrap_or(0);
    ROUND[if left { (i + 1) % 4 } else { (i + 3) % 4 }]
}

/// A window pixel as a point (pt) on the app's screen. `fit` places the phone
/// in the window (scale, x, y); `screen` is the screen's rect in the phone
/// picture; `scale` its pixels per point. Off the screen: None, or with
/// `clamp`, the nearest point on it.
fn screen_point(fit: (f32, f32, f32), screen: [f32; 4], scale: f32, p: (f32, f32), clamp: bool) -> Option<(f64, f64)> {
    let (k, dx, dy) = fit;
    let (mut x, mut y) = ((p.0 - dx) / k, (p.1 - dy) / k);
    let s = screen;
    let inside = x >= s[0] && y >= s[1] && x < s[0] + s[2] && y < s[1] + s[3];
    if !inside {
        if !clamp {
            return None;
        }
        x = x.clamp(s[0], s[0] + s[2] - 1.0);
        y = y.clamp(s[1], s[1] + s[3] - 1.0);
    }
    Some((((x - s[0]) / scale) as f64, ((y - s[1]) / scale) as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_id_shortcuts() {
        assert_eq!(face_shortcut(KeyCode::KeyM), Some(Face::Matching));
        assert_eq!(face_shortcut(KeyCode::KeyN), Some(Face::NonMatching));
        assert_eq!(face_shortcut(KeyCode::KeyE), Some(Face::ToggleEnrolled));
        assert_eq!(face_shortcut(KeyCode::KeyX), None);
    }
    use Orientation::*;

    #[test]
    fn turning_goes_round_both_ways() {
        let mut o = Portrait;
        for want in [LandscapeRight, PortraitUpsideDown, LandscapeLeft, Portrait] {
            o = turned(o, true);
            assert_eq!(o, want);
        }
        assert_eq!(turned(Portrait, false), LandscapeLeft);
        assert_eq!(turned(LandscapeLeft, true), Portrait);
    }

    #[test]
    fn clicks_map_to_screen_points() {
        // A phone at 2 px/pt with its screen at (36, 28) px, shown at half size 10 px in.
        let screen = [36.0, 28.0, 780.0, 1688.0];
        let fit = (0.5, 10.0, 0.0);
        // The screen's top-left in the window: (10 + 18, 14).
        assert_eq!(screen_point(fit, screen, 2.0, (28.0, 14.0), false), Some((0.0, 0.0)));
        // 100 window px right, 50 down: 200 × 100 phone px, 100 × 50 pt.
        assert_eq!(screen_point(fit, screen, 2.0, (128.0, 64.0), false), Some((100.0, 50.0)));
        // On the bezel: nothing, unless clamped to the screen's edge.
        assert_eq!(screen_point(fit, screen, 2.0, (12.0, 64.0), false), None);
        assert_eq!(screen_point(fit, screen, 2.0, (12.0, 64.0), true), Some((0.0, 50.0)));
    }
}

impl ApplicationHandler<ToWindow> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let menus = menu::Menus::new(self.config.enrolled, self.proxy.clone(), ToWindow::Menu);
        menus.install();
        self.menus = Some(menus);
        // The phone is the window: no frame, see-through around it.
        let attrs = Window::default_attributes().with_title(&self.config.title).with_decorations(false).with_transparent(true).with_resizable(false);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("can't open a window: {e}");
                event_loop.exit();
                return;
            }
        };
        let scale = window.scale_factor() as f32;
        self.window = Some(window.clone());
        match present::Presenter::new(window.clone()) {
            Ok(p) => {
                // Can't see through it here: a normal window frame around the phone on its backdrop.
                if !p.transparent() {
                    window.set_decorations(true);
                }
                self.presenter = Some(p);
            }
            Err(e) => {
                eprintln!("can't draw in the window: {e}");
                event_loop.exit();
                return;
            }
        }
        self.phone = self.blank_phone(scale);
        if let Some(p) = &self.phone {
            self.size_window_to(p);
        }
        // Start the emulator.
        let (tx, rx) = channel();
        self.events = Some(tx);
        let stats = self.config.frame_stats.then(FrameStats::default);
        let host = DesktopHost { events: rx, frame: self.frame.clone(), proxy: self.proxy.clone(), base: None, closed: self.closed.clone(), stats };
        let app = self.app.take().unwrap();
        let proxy = self.proxy.clone();
        std::thread::Builder::new()
            .name("pomme".into())
            .stack_size(64 << 20)
            .spawn(move || {
                app(Box::new(host), scale);
                let _ = proxy.send_event(ToWindow::Ended);
            })
            .expect("the emulator thread");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ToWindow) {
        match event {
            ToWindow::Menu(a) => self.act(event_loop, a),
            ToWindow::FaceId(up) => {
                self.face_id = up;
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            ToWindow::Frame => self.take_frame(),
            ToWindow::Ended => {
                self.ended = true;
                if let Some(w) = &self.window {
                    w.set_title(&format!("{} — stopped", self.config.title));
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.close(event_loop),
            // Where there's no menu bar, the menus pop up from the bezel.
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Right, .. } => {
                if self.point_at(self.cursor).is_none()
                    && self.on_phone(self.cursor)
                    && let (Some(m), Some(w)) = (&self.menus, &self.window)
                {
                    m.pop_up(w);
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(size) => {
                if let Some(p) = self.presenter.as_mut() {
                    p.resize(size.width, size.height);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                if self.touching {
                    // Dragging off the screen keeps the finger at the edge it left by.
                    if let Some(p) = self.point_at(position) {
                        self.touch(MOVED, p);
                    }
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => {
                    if let Some(p) = self.point_at(self.cursor) {
                        self.touching = true;
                        self.touch(BEGAN, p);
                    } else if self.on_phone(self.cursor) {
                        // The bezel: move the phone (the window) around, as in the Simulator.
                        if let Some(w) = &self.window {
                            let _ = w.drag_window();
                        }
                    }
                }
                ElementState::Released => {
                    if self.touching {
                        self.touching = false;
                        // Let go off the screen: the finger leaves at the screen's edge.
                        let p = self.phone.as_ref().and_then(|phone| {
                            let fit = self.fit(phone)?;
                            screen_point(fit, phone.screen, phone.scale, (self.cursor.x as f32, self.cursor.y as f32), true)
                        });
                        if let Some(p) = p {
                            self.touch(ENDED, p);
                        }
                    }
                }
            },
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed => {
                let command = if cfg!(target_os = "macos") { self.modifiers.super_key() } else { self.modifiers.control_key() };
                // On macOS the menu bar has these shortcuts too. AppKit gives a key
                // to the menu or the window, never both: whichever gets it acts.
                // ⌥ changes the character (⌥M is µ on a Mac): the Face ID shortcuts go by the key.
                if command && self.modifiers.alt_key() {
                    if let PhysicalKey::Code(code) = event.physical_key
                        && let Some(face) = face_shortcut(code)
                    {
                        if face == Face::ToggleEnrolled
                            && let Some(m) = &self.menus
                        {
                            m.toggled_enrolled();
                        }
                        self.send(Event::Face(face));
                    }
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) if command => self.rotate(true),
                    Key::Named(NamedKey::ArrowRight) if command => self.rotate(false),
                    // No title bar to close it with: ⌘W / ⌘Q.
                    Key::Character(c) if command && (c.as_str() == "w" || c.as_str() == "q") => self.close(event_loop),
                    _ if command => {}
                    Key::Named(NamedKey::Enter) => self.send(Event::Key('\n')),
                    Key::Named(NamedKey::Backspace) => self.send(Event::Key('\u{8}')),
                    _ => {
                        if let Some(text) = &event.text {
                            for ch in text.chars().filter(|c| !c.is_control()) {
                                self.send(Event::Key(ch));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod sheet_tests {
    /// The Face ID sheet lands in the middle of the screen and leaves the rest alone.
    #[test]
    fn sheet_is_centred_on_the_screen() {
        let mut pix = tiny_skia::Pixmap::new(400, 800).unwrap();
        pix.fill(tiny_skia::Color::WHITE);
        super::face_id_sheet(&mut pix, [0.0, 0.0, 400.0, 800.0], 1.0);
        if let Ok(dir) = std::env::var("POMME_SHEET_PNG") {
            pix.save_png(dir).unwrap();
        }
        let dark = |x, y| pix.pixel(x, y).unwrap().red() < 100;
        assert!(dark(200 - 70, 400), "the card's edge");
        assert!(!dark(200 - 90, 400) && !dark(10, 10), "outside the card");
        assert!(!dark(200 - 32, 400 - 25), "the glyph's white bracket");
    }
}
