//! pomme in a desktop window, looking like the iOS Simulator: the app on a
//! phone, live. Clicks and drags on the screen are touches, typing goes to the
//! focused text input (as a hardware keyboard does in the Simulator), and
//! ⌘← / ⌘→ (Ctrl on Windows and Linux) turn the phone.
//!
//! The emulator runs on its own thread with a [`DesktopHost`]: it paces
//! virtual time against the wall clock while the app idles, and takes the
//! window's input then. The window (winit, on the main thread as macOS
//! requires) draws the phone around each frame with softbuffer, so it runs
//! anywhere winit does: macOS, Windows, Linux (X11 and Wayland).

use pomme_hle::device::{Device, Orientation};
use pomme_hle::host::{Event, Frame, Host, Idle};
use pomme_hle::touch::{BEGAN, ENDED, Input, MOVED};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_skia::{Pixmap, PixmapPaint, Transform};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// What the window shows and how.
pub struct Config {
    pub device: &'static Device,
    /// How the phone is held at launch.
    pub orientation: Orientation,
    /// The window's title (the Simulator shows the device and iOS version).
    pub title: String,
}

/// The emulator thread's side: a frame for the window, or the run ended.
enum ToWindow {
    Frame,
    Ended,
}

/// The latest frame from the emulator, waiting for the window.
struct LatestFrame {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    orientation: Orientation,
}

/// The emulator's host: input from the window, frames to it.
pub struct DesktopHost {
    events: Receiver<Event>,
    frame: Arc<Mutex<Option<LatestFrame>>>,
    proxy: EventLoopProxy<ToWindow>,
    /// wall − virtual time, fixed when the app first idles.
    base: Option<Instant>,
    closed: Arc<std::sync::atomic::AtomicBool>,
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
                // Due (or overdue: the app was busy). Far behind: don't race through the backlog.
                if wall_virt > Duration::from_nanos(next_ns.unwrap_or(0)) + Duration::from_millis(250) {
                    self.base = Some(Instant::now() - virt);
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

    fn frame(&mut self, f: Frame<'_>) {
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
        surface: None,
        events: None,
        frame: Arc::new(Mutex::new(None)),
        closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        phone: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        touching: false,
        modifiers: ModifiersState::empty(),
        ended: false,
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
    window: Option<Rc<Window>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
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
}

/// The window's background around the phone (the Simulator's dark backdrop).
const BACKDROP: u32 = 0x1e1e20;

impl App {
    /// The phone at this frame scale with a blank screen: the window's size before the first frame.
    fn blank_phone(&self, scale: f32) -> Option<Phone> {
        let (w, h) = self.config.device.screen(self.held);
        let mut screen = Pixmap::new((w * scale as f64) as u32, (h * scale as f64) as u32)?;
        screen.fill(tiny_skia::Color::BLACK);
        let (pix, rect) = pomme_hle::render::shell(self.config.device, self.held, &screen, scale).ok()?;
        Some(Phone { pix, screen: rect, scale })
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
        let Ok((pix, rect)) = pomme_hle::render::shell(self.config.device, f.orientation, &screen, scale) else { return };
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
        let (Some(window), Some(phone)) = (self.window.clone(), self.phone.as_ref()) else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
        let Some((k, dx, dy)) = self.fit(phone) else { return };
        // The phone, scaled to fit, over the backdrop.
        let Some(mut canvas) = Pixmap::new(size.width, size.height) else { return };
        canvas.fill(tiny_skia::Color::from_rgba8((BACKDROP >> 16) as u8, (BACKDROP >> 8) as u8, BACKDROP as u8, 255));
        let paint = PixmapPaint { quality: if (k - 1.0).abs() < 1e-3 { tiny_skia::FilterQuality::Nearest } else { tiny_skia::FilterQuality::Bilinear }, ..Default::default() };
        canvas.draw_pixmap(0, 0, phone.pix.as_ref(), &paint, Transform::from_row(k, 0.0, 0.0, k, dx, dy), None);
        let Some(surface) = self.surface.as_mut() else { return };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else { return };
        for (dst, src) in buffer.iter_mut().zip(canvas.pixels()) {
            // Opaque over the backdrop: premultiplied equals straight.
            *dst = (src.red() as u32) << 16 | (src.green() as u32) << 8 | src.blue() as u32;
        }
        let _ = buffer.present();
    }

    /// ⌘← / ⌘→ (Ctrl elsewhere): turn the phone a quarter, as the Simulator's Rotate Left / Right.
    fn rotate(&mut self, left: bool) {
        self.held = turned(self.held, left);
        self.send(Event::Rotate(self.held));
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
        let attrs = Window::default_attributes().with_title(&self.config.title);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("can't open a window: {e}");
                event_loop.exit();
                return;
            }
        };
        let scale = window.scale_factor() as f32;
        self.window = Some(window.clone());
        let context = softbuffer::Context::new(window.clone()).expect("a display to draw on");
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).expect("a surface to draw on"));
        self.phone = self.blank_phone(scale);
        if let Some(p) = &self.phone {
            self.size_window_to(p);
        }
        // Start the emulator.
        let (tx, rx) = channel();
        self.events = Some(tx);
        let host = DesktopHost { events: rx, frame: self.frame.clone(), proxy: self.proxy.clone(), base: None, closed: self.closed.clone() };
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

    fn user_event(&mut self, _: &ActiveEventLoop, event: ToWindow) {
        match event {
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
            WindowEvent::CloseRequested => {
                self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
                self.events = None; // the host sees the channel close
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) => {
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
                match &event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) if command => self.rotate(true),
                    Key::Named(NamedKey::ArrowRight) if command => self.rotate(false),
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
