//! pomme in a desktop window, looking like the iOS Simulator: the app on a
//! phone, live. Clicks and drags on the screen are touches, typing goes to the
//! focused text input (as a hardware keyboard does in the Simulator),
//! ⌘← / ⌘→ (Ctrl on Windows and Linux) turn the phone, and the Simulator's
//! Face ID shortcuts work: ⌥⌘M a matching face, ⌥⌘N one that doesn't match,
//! and ⌥⌘E turns enrolment on or off. ⌘S saves the screen and ⌘R records it
//! (to the Desktop, see capture.rs). They're menus too, as the Simulator's
//! (File, Device, Features ▸ Face ID): see menu.rs.
//!
//! The emulator runs on its own thread with a [`DesktopHost`]: it paces
//! virtual time against the wall clock while the app idles, and takes the
//! window's input then. The window (winit, on the main thread as macOS
//! requires) is the phone itself, as the Simulator's is: borderless, its
//! rounded corners see-through (wgpu, see present.rs), dragged by its bezel,
//! closed with ⌘W / ⌘Q (Ctrl elsewhere). It runs wherever winit and wgpu
//! do: macOS, Windows, Linux (X11 and Wayland); where windows can't be
//! transparent the phone sits on a dark backdrop.
//!
//! The system's screen reader sees the app through the window
//! (accessibility.rs): VoiceOver on a Mac reads its elements and activates them.

mod accessibility;
mod open_url;
mod capture;
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), path = "menu_none.rs")]
mod menu;
mod present;

use pomme_hle::device::{Device, Orientation};
use pomme_hle::host::{Event, Frame, Host, Idle};
use pomme_hle::keychain::Face;
use pomme_hle::touch::{BEGAN, ENDED, Input, MOVED};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Print frame statistics each second frames arrive (see [`pomme_hle::host::FrameStats`]).
    pub frame_stats: bool,
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
    /// File ▸ Save Screen.
    SaveScreen,
    /// File ▸ Record Screen / Stop Recording.
    Record,
    /// Device ▸ Relaunch App: quit the app and start it again, keeping what it saved.
    Relaunch,
    /// File ▸ Open URL…: the panel to type or paste a link into.
    OpenUrl,
    /// Device ▸ Shake: a shake motion event, as the Simulator's (⌃⌘Z).
    Shake,
    /// File ▸ Open Recent URL ▸ one of the recent links (its place in the list).
    OpenRecent(usize),
    /// File ▸ Open Recent URL ▸ Clear.
    ClearRecent,
}

/// The emulator thread's side: a frame for the window, or the run ended.
enum ToWindow {
    Frame,
    Ended,
    /// A menu item was chosen.
    Menu(Action),
    /// A Face ID prompt is waiting (true) or was answered (false).
    FaceId(bool),
    /// The app's accessibility tree changed.
    Tree(Vec<pomme_hle::accessibility::Node>),
    /// The system's screen reader wants something (AccessKit).
    ScreenReader(accesskit_winit::Event),
    /// What became of a link sent with File ▸ Open URL.
    UrlOpened(String, Result<(), String>),
}

impl From<accesskit_winit::Event> for ToWindow {
    fn from(e: accesskit_winit::Event) -> Self {
        ToWindow::ScreenReader(e)
    }
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
    /// Frame statistics, and when they started (`--frame-stats`).
    stats: Option<(pomme_hle::host::FrameStats, Instant)>,
    /// The phone's name, for capture files.
    device: &'static str,
    recording: Option<capture::Recording>,
    /// A screen reader is listening on the window (AccessKit asked for the tree).
    screen_reader: Arc<AtomicBool>,
    /// Whether the emulator has heard so.
    screen_reader_seen: bool,
}

impl Host for DesktopHost {
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> Idle {
        let virt = Duration::from_nanos(now_ns);
        let base = *self.base.get_or_insert_with(|| Instant::now() - virt);
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return Idle::Quit;
            }
            let wall_virt = Instant::now().saturating_duration_since(base);
            // A screen reader came or went: look again (it wants the tree now).
            let listening = self.screen_reader.load(Ordering::Relaxed);
            if listening != self.screen_reader_seen {
                self.screen_reader_seen = listening;
                return Idle::Input { at_ns: wall_virt.max(virt).as_nanos() as u64, events: Vec::new() };
            }
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

    fn url_opened(&mut self, url: &str, result: Result<(), &str>) {
        let _ = self.proxy.send_event(ToWindow::UrlOpened(url.to_string(), result.map_err(str::to_string)));
    }

    fn screenshot(&mut self, shot: Frame<'_>) {
        capture::save_screenshot(shot, self.device);
    }

    fn recorded(&mut self, f: Frame<'_>) {
        self.recording.get_or_insert_with(|| capture::Recording::start(self.device)).frame(f);
    }

    fn recording_stopped(&mut self, at_ns: u64) {
        if let Some(r) = self.recording.take() {
            r.stop(at_ns);
        }
    }

    fn wants_accessibility(&mut self) -> bool {
        self.screen_reader.load(Ordering::Relaxed)
    }

    fn clock_ms(&mut self) -> Option<f64> {
        // Only while it's reporting: the clock costs a little on every idle call.
        self.stats.as_ref().map(|(_, start)| start.elapsed().as_secs_f64() * 1000.0)
    }

    fn accessibility(&mut self, tree: &[pomme_hle::accessibility::Node]) {
        let _ = self.proxy.send_event(ToWindow::Tree(tree.to_vec()));
    }

    fn frame(&mut self, f: Frame<'_>) {
        if let Some((s, start)) = self.stats.as_mut()
            && let Some(line) = s.frame(&f, start.elapsed().as_secs_f64() * 1000.0)
        {
            eprintln!("{line}");
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
        recording: false,
        adapter: None,
        screen_reader: Arc::new(AtomicBool::new(false)),
        ax_nodes: Vec::new(),
        ax_focus: None,
        panel: None,
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
    /// Recording the screen (File ▸ Record Screen).
    recording: bool,
    /// The phone's body for the last (orientation, scale bits) a frame came in.
    shell: Option<((Orientation, u32), pomme_hle::render::Shell)>,
    /// The window's accessibility, for the system's screen reader.
    adapter: Option<accesskit_winit::Adapter>,
    screen_reader: Arc<AtomicBool>,
    /// The app's accessibility tree as last given, and the element the screen reader is on.
    ax_nodes: Vec<pomme_hle::accessibility::Node>,
    ax_focus: Option<u64>,
    /// File ▸ Open URL's panel (made with the window).
    panel: Option<open_url::Panel>,
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
            self.update_accessibility();
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
        // The Open URL panel over the phone's screen, while it's open.
        let mut wants = open_url::Wants::Nothing;
        let overlay = match (self.panel.as_mut(), self.window.as_ref()) {
            (Some(panel), Some(window)) if panel.open => {
                let (k, dx, dy) = fit;
                let [sx, sy, sw, sh] = phone.screen;
                let (overlay, w) = panel.frame(window, [dx + sx * k, dy + sy * k, sw * k, sh * k]);
                wants = w;
                Some(overlay)
            }
            _ => None,
        };
        let pix = sheet.as_ref().unwrap_or(&phone.pix);
        let (data, w, h) = (pix.data(), pix.width(), pix.height());
        if let Some(p) = self.presenter.as_mut() {
            p.present(data, w, h, fit, overlay);
        }
        self.panel_wants(wants);
    }

    /// Do what the panel asked for in its last frame.
    fn panel_wants(&mut self, wants: open_url::Wants) {
        match wants {
            open_url::Wants::Nothing => return,
            open_url::Wants::Close => {
                if let Some(p) = self.panel.as_mut() {
                    p.close();
                }
            }
            open_url::Wants::Open(url) => self.open_url(url),
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// Send a link to the app; its answer comes back as `ToWindow::UrlOpened`.
    fn open_url(&mut self, url: String) {
        if self.ended || self.events.is_none() {
            if let Some(p) = self.panel.as_mut() {
                p.unanswerable("The app has stopped.");
            }
            return;
        }
        self.send(Event::OpenUrl(url));
    }

    /// The recent links changed: the menu lists them.
    fn recent_changed(&self) {
        if let (Some(m), Some(p)) = (&self.menus, &self.panel) {
            m.set_recent(&p.recent.items);
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
            Action::SaveScreen => self.send(Event::SaveScreen),
            Action::Relaunch => self.send(Event::Relaunch),
            Action::Shake => self.send(Event::Shake),
            Action::OpenUrl => {
                if let Some(p) = self.panel.as_mut() {
                    p.show();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            Action::OpenRecent(i) => {
                let url = self.panel.as_ref().and_then(|p| p.recent.items.get(i).cloned());
                if let Some(url) = url {
                    if let Some(p) = self.panel.as_mut() {
                        p.show();
                        p.sent(&url);
                    }
                    self.open_url(url);
                }
            }
            Action::ClearRecent => {
                if let Some(p) = self.panel.as_mut() {
                    p.recent.clear();
                }
                self.recent_changed();
            }
            Action::Record => {
                self.recording = !self.recording;
                if let Some(m) = &self.menus {
                    m.recording(self.recording);
                }
                self.send(Event::Record(self.recording));
            }
        }
    }

    /// Give the screen reader the tree, placed where the screen is in the window now.
    fn update_accessibility(&mut self) {
        let Some(phone) = self.phone.as_ref() else { return };
        let Some((k, dx, dy)) = self.fit(phone) else { return };
        let [sx, sy, ..] = phone.screen;
        let place = accessibility::Place { x: (dx + sx * k) as f64, y: (dy + sy * k) as f64, k: (phone.scale * k) as f64 };
        let (title, nodes, focus) = (&self.config.title, &self.ax_nodes, self.ax_focus);
        if let Some(a) = self.adapter.as_mut() {
            a.update_if_active(|| accessibility::update(title, nodes, place, focus));
        }
    }

    fn screen_reader(&mut self, e: accesskit_winit::WindowEvent) {
        use accesskit_winit::WindowEvent as W;
        match e {
            W::InitialTreeRequested => {
                // The emulator reads the tree for it; until then, the window alone.
                self.screen_reader.store(true, Ordering::Relaxed);
                self.update_accessibility();
            }
            W::ActionRequested(req) => {
                if let Some((id, action)) = accessibility::action(&req) {
                    if action == pomme_hle::accessibility::Action::Focus {
                        self.ax_focus = Some(id);
                        self.update_accessibility();
                    }
                    self.send(Event::Accessibility { id, action });
                }
            }
            W::AccessibilityDeactivated => self.screen_reader.store(false, Ordering::Relaxed),
        }
    }

    /// A window event while the Open URL panel is open: the keyboard is the
    /// panel's (⌘W / ⌘Q still close the window); a click on the panel is its
    /// own; a click elsewhere on the screen closes it; the bezel and the rest
    /// work as usual. True when the event is used up.
    fn panel_event(&mut self, event_loop: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let Some(window) = self.window.clone() else { return false };
        let Some(panel) = self.panel.as_mut() else { return false };
        let over_panel = panel.on_event(&window, event);
        if let WindowEvent::CursorMoved { position, .. } = event {
            self.cursor = *position;
        }
        let command = if cfg!(target_os = "macos") { self.modifiers.super_key() } else { self.modifiers.control_key() };
        let used = match event {
            WindowEvent::KeyboardInput { event: key, .. } => {
                if command && key.state == ElementState::Pressed && matches!(&key.logical_key, Key::Character(c) if c.as_str() == "w" || c.as_str() == "q") {
                    self.close(event_loop);
                }
                true
            }
            WindowEvent::Ime(_) => true,
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if !over_panel => {
                let on_screen = self.point_at(self.cursor).is_some();
                if on_screen && let Some(p) = self.panel.as_mut() {
                    p.close();
                }
                on_screen
            }
            WindowEvent::MouseInput { .. } | WindowEvent::MouseWheel { .. } | WindowEvent::CursorMoved { .. } => over_panel,
            _ => false,
        };
        // (A moved cursor over the panel still redraws it: buttons light up.)
        if used || over_panel {
            window.request_redraw();
        }
        used
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
        // Hidden until the screen reader's adapter is on it (AccessKit needs that).
        let attrs = Window::default_attributes().with_title(&self.config.title).with_decorations(false).with_transparent(true).with_resizable(false).with_visible(false);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("can't open a window: {e}");
                event_loop.exit();
                return;
            }
        };
        self.adapter = Some(accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, self.proxy.clone()));
        self.panel = Some(open_url::Panel::new(&window));
        self.recent_changed();
        window.set_visible(true);
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
        let stats = self.config.frame_stats.then(|| (pomme_hle::host::FrameStats::default(), Instant::now()));
        let host = DesktopHost { events: rx, frame: self.frame.clone(), proxy: self.proxy.clone(), base: None, closed: self.closed.clone(), stats, device: self.config.device.name, recording: None, screen_reader: self.screen_reader.clone(), screen_reader_seen: false };
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
            ToWindow::Tree(nodes) => {
                self.ax_nodes = nodes;
                self.update_accessibility();
            }
            ToWindow::ScreenReader(e) => self.screen_reader(e.window_event),
            ToWindow::UrlOpened(url, result) => {
                if let Some(p) = self.panel.as_mut() {
                    p.answered(&url, result);
                }
                self.recent_changed();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            ToWindow::Ended => {
                self.ended = true;
                if let Some(w) = &self.window {
                    w.set_title(&format!("{} — stopped", self.config.title));
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let (Some(a), Some(w)) = (self.adapter.as_mut(), self.window.as_ref()) {
            a.process_event(w, &event);
        }
        if self.panel.as_ref().is_some_and(|p| p.open) && self.panel_event(event_loop, &event) {
            return;
        }
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
                // The screen moved in the window: so did its elements.
                self.update_accessibility();
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
                    if !cfg!(target_os = "macos") && event.physical_key == PhysicalKey::Code(KeyCode::KeyZ) {
                        self.send(Event::Shake);
                        return;
                    }
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
                    Key::Character(c) if command && c.as_str() == "s" => self.act(event_loop, Action::SaveScreen),
                    Key::Character(c) if command && c.as_str() == "l" => self.act(event_loop, Action::OpenUrl),
                    // ⌃⌘Z: Device ▸ Shake (macOS; elsewhere Ctrl+Alt+Z, above).
                    Key::Character(c) if command && self.modifiers.control_key() && cfg!(target_os = "macos") && c.eq_ignore_ascii_case("z") => self.act(event_loop, Action::Shake),
                    Key::Character(c) if command && self.modifiers.shift_key() && c.eq_ignore_ascii_case("r") => self.act(event_loop, Action::Relaunch),
                    Key::Character(c) if command && c.as_str() == "r" => self.act(event_loop, Action::Record),
                    _ if command => {}
                    Key::Named(NamedKey::Enter) => self.send(Event::Key('\n')),
                    Key::Named(NamedKey::Backspace) => self.send(Event::Key('\u{8}')),
                    Key::Named(NamedKey::ArrowLeft) => self.send(Event::Key(pomme_hle::textinput::KEY_LEFT)),
                    Key::Named(NamedKey::ArrowRight) => self.send(Event::Key(pomme_hle::textinput::KEY_RIGHT)),
                    Key::Named(NamedKey::ArrowUp) => self.send(Event::Key(pomme_hle::textinput::KEY_UP)),
                    Key::Named(NamedKey::ArrowDown) => self.send(Event::Key(pomme_hle::textinput::KEY_DOWN)),
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
