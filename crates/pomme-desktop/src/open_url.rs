//! File ▸ Open URL: a panel over the phone's screen with a text field, drawn
//! with egui's own parts in pomme's window (no eframe: the window stays
//! pomme's, borderless and see-through). Return opens the link in the
//! running app (`Event::OpenUrl`); the app's answer comes back through the
//! host: accepted, the panel closes and the link goes on the recent list;
//! refused, the panel stays and says why. Escape closes it. While it's open
//! the keyboard is the panel's, not the app's.
//!
//! Recent links (the last ten, newest first, no repeats) are kept in
//! `recent-urls.txt` in pomme's config folder, one per line.

use std::path::PathBuf;
use winit::event::WindowEvent;
use winit::window::Window;

/// How many recent links are kept.
pub(crate) const RECENT_MAX: usize = 10;

/// What the panel wants done after a frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Wants {
    Nothing,
    /// Open this link in the app.
    Open(String),
    /// Close the panel (Escape, Cancel).
    Close,
}

/// A frame of the panel, ready for the presenter.
pub(crate) struct Overlay {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

/// The panel itself, apart from the window: what it shows and what the
/// user asked for, from egui's input. Tests drive it with egui's events.
pub(crate) struct Form {
    pub open: bool,
    text: String,
    /// Why the last link wasn't taken.
    error: Option<String>,
    /// A link sent to the app, waiting for its answer.
    waiting: Option<String>,
    /// Focus the field and select its text on the next frame.
    fresh: bool,
    pub recent: Recent,
    ctx: egui::Context,
}

/// The panel in pomme's window: the [`Form`], and egui's link to winit.
pub(crate) struct Panel {
    pub form: Form,
    state: egui_winit::State,
}

impl Panel {
    pub fn new(window: &Window) -> Panel {
        let form = Form::new(Recent::load());
        let state = egui_winit::State::new(form.ctx.clone(), egui::ViewportId::ROOT, window, Some(window.scale_factor() as f32), window.theme(), None);
        Panel { form, state }
    }

    /// A window event while the panel is open; true if egui wants the
    /// pointer where it is (the event is the panel's).
    pub fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let _ = self.state.on_window_event(window, event);
        self.form.ctx.is_pointer_over_egui()
    }

    /// One frame of the panel, laid out over `screen` (the phone's screen,
    /// in window pixels): what to draw, and what the user asked for.
    pub fn frame(&mut self, window: &Window, screen: [f32; 4]) -> (Overlay, Wants) {
        let input = self.state.take_egui_input(window);
        let ppp = window.scale_factor() as f32;
        let (output, wants) = self.form.run(input, screen.map(|v| v / ppp));
        self.state.handle_platform_output(window, output.platform_output);
        let primitives = self.form.ctx.tessellate(output.shapes, output.pixels_per_point);
        (Overlay { primitives, textures: output.textures_delta, pixels_per_point: output.pixels_per_point }, wants)
    }
}

impl std::ops::Deref for Panel {
    type Target = Form;
    fn deref(&self) -> &Form {
        &self.form
    }
}

impl std::ops::DerefMut for Panel {
    fn deref_mut(&mut self) -> &mut Form {
        &mut self.form
    }
}

impl Form {
    pub fn new(recent: Recent) -> Form {
        let ctx = egui::Context::default();
        // Nothing animates while the app's frames don't come: a steady caret.
        ctx.all_styles_mut(|s| s.visuals.text_cursor.blink = false);
        Form { open: false, text: String::new(), error: None, waiting: None, fresh: false, recent, ctx }
    }

    /// Show the panel, with the last link (selected) in the field.
    pub fn show(&mut self) {
        if !self.open {
            self.open = true;
            self.text = self.recent.items.first().cloned().unwrap_or_default();
            self.error = None;
            self.waiting = None;
            self.fresh = true;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.waiting = None;
        self.error = None;
    }

    /// The app's answer to the link sent: accepted closes the panel and
    /// remembers the link; refused shows why.
    pub fn answered(&mut self, url: &str, result: Result<(), String>) {
        if self.waiting.as_deref() != Some(url) {
            return;
        }
        self.waiting = None;
        match result {
            Ok(()) => {
                self.recent.add(url);
                self.close();
            }
            Err(why) => self.error = Some(why),
        }
    }

    /// A link sent from elsewhere (File ▸ Open Recent URL): wait for its answer here.
    pub fn sent(&mut self, url: &str) {
        self.text = url.to_string();
        self.error = None;
        self.waiting = Some(url.to_string());
    }

    /// The link was sent, but nothing will answer (the app has stopped).
    pub fn unanswerable(&mut self, why: &str) {
        self.waiting = None;
        self.error = Some(why.to_string());
    }

    /// One frame of the panel, laid out over `screen` (the phone's screen,
    /// in egui's points): egui's output, and what the user asked for.
    pub fn run(&mut self, input: egui::RawInput, screen: [f32; 4]) -> (egui::FullOutput, Wants) {
        let [x, y, w, _] = screen;
        let mut wants = Wants::Nothing;
        let fresh = std::mem::take(&mut self.fresh);
        let (text, error, waiting, recent) = (&mut self.text, &self.error, &self.waiting, &self.recent.items);
        let output = self.ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            let margin = 12.0;
            egui::Area::new(egui::Id::unique("open-url")).fixed_pos(egui::pos2(x + margin, y + 60.0)).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(w - 2.0 * margin - 16.0);
                    ui.strong("Open URL");
                    let field = egui::TextEdit::singleline(text).hint_text("myapp://path, or the link `npx expo start` prints").desired_width(f32::INFINITY);
                    let mut out = field.show(ui);
                    if fresh {
                        out.response.response.request_focus();
                        let end = egui::text::CCursor::new(out.galley.text().chars().count());
                        out.state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
                        out.state.store(&ctx, out.response.response.id);
                    }
                    let enter = out.response.response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if let Some(why) = error {
                        ui.colored_label(ui.visuals().error_fg_color, why);
                    }
                    if waiting.is_some() {
                        ui.label("Opening…");
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            wants = Wants::Close;
                        }
                        if (ui.button("Open").clicked() || enter) && !text.trim().is_empty() {
                            wants = Wants::Open(text.trim().to_string());
                        }
                    });
                    if !recent.is_empty() {
                        ui.separator();
                        ui.weak("Recent");
                        for url in recent {
                            if ui.selectable_label(false, url).clicked() {
                                *text = url.clone();
                                wants = Wants::Open(url.clone());
                            }
                        }
                    }
                });
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                wants = Wants::Close;
            }
        });
        if let Wants::Open(url) = &wants {
            self.error = None;
            self.waiting = Some(url.clone());
        }
        (output, wants)
    }
}

/// The recent links: newest first, no repeats, at most [`RECENT_MAX`].
pub(crate) struct Recent {
    pub items: Vec<String>,
    path: Option<PathBuf>,
}

impl Recent {
    /// From the config folder; an unreadable file is an empty list.
    pub fn load() -> Recent {
        Recent::load_from(config_dir().map(|d| d.join("pomme").join("recent-urls.txt")))
    }

    fn load_from(path: Option<PathBuf>) -> Recent {
        let items = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| parse(&s)).unwrap_or_default();
        Recent { items, path }
    }

    /// A list that isn't saved anywhere.
    #[cfg(test)]
    pub fn in_memory(items: &[&str]) -> Recent {
        Recent { items: items.iter().map(|s| s.to_string()).collect(), path: None }
    }

    pub fn add(&mut self, url: &str) {
        self.items = pushed(&self.items, url);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.items.join("\n") + "\n");
    }
}

/// The list read from its file: one link per line, blank lines skipped.
fn parse(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in s.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if !out.iter().any(|u| u == l) && out.len() < RECENT_MAX {
            out.push(l.to_string());
        }
    }
    out
}

/// `url` first, then the rest without it, at most [`RECENT_MAX`].
fn pushed(items: &[String], url: &str) -> Vec<String> {
    std::iter::once(url.to_string()).chain(items.iter().filter(|u| *u != url).cloned()).take(RECENT_MAX).collect()
}

/// The platform's config folder: `~/Library/Application Support`, `%APPDATA%`,
/// `$XDG_CONFIG_HOME` or `~/.config`.
fn config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The phone's screen in egui's points, as the window lays the panel over it.
    const SCREEN: [f32; 4] = [0.0, 0.0, 390.0, 844.0];

    /// One frame with these events.
    fn frame(form: &mut Form, events: Vec<egui::Event>) -> Wants {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(390.0, 844.0))),
            focused: true,
            events,
            ..Default::default()
        };
        let (mut out, wants) = form.run(input, SCREEN);
        out.textures_delta.clear();
        wants
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    fn text(t: &str) -> egui::Event {
        egui::Event::Text(t.into())
    }

    /// Open the panel and draw its first frame (the field takes the focus).
    fn opened(recent: &[&str]) -> Form {
        let mut form = Form::new(Recent::in_memory(recent));
        form.show();
        assert_eq!(frame(&mut form, vec![]), Wants::Nothing);
        form
    }

    #[test]
    fn return_opens_the_typed_link() {
        let mut form = opened(&[]);
        assert_eq!(frame(&mut form, vec![text("pommehello:///tabs")]), Wants::Nothing);
        assert_eq!(frame(&mut form, vec![key(egui::Key::Enter)]), Wants::Open("pommehello:///tabs".into()));
        // Accepted: the panel closes and the link is the newest recent one.
        form.answered("pommehello:///tabs", Ok(()));
        assert!(!form.open);
        assert_eq!(form.recent.items, ["pommehello:///tabs"]);
    }

    #[test]
    fn the_last_link_is_selected_so_typing_replaces_it() {
        let mut form = opened(&["old://link"]);
        assert_eq!(form.text, "old://link");
        frame(&mut form, vec![text("new://link")]);
        assert_eq!(frame(&mut form, vec![key(egui::Key::Enter)]), Wants::Open("new://link".into()));
    }

    #[test]
    fn a_refused_link_keeps_the_panel_open_with_the_reason() {
        let mut form = opened(&[]);
        frame(&mut form, vec![text("https://example.com")]);
        assert_eq!(frame(&mut form, vec![key(egui::Key::Enter)]), Wants::Open("https://example.com".into()));
        form.answered("https://example.com", Err("The app doesn't open https links.".into()));
        assert!(form.open);
        assert_eq!(form.error.as_deref(), Some("The app doesn't open https links."));
        assert!(form.recent.items.is_empty());
        // The reason is drawn.
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(390.0, 844.0))), ..Default::default() };
        let mut out = form.run(input, SCREEN).0;
        out.textures_delta.clear();
        assert!(format!("{:?}", out.shapes).contains("open https links"));
    }

    #[test]
    fn escape_closes() {
        let mut form = opened(&["a://1"]);
        assert_eq!(frame(&mut form, vec![key(egui::Key::Escape)]), Wants::Close);
    }

    #[test]
    fn an_empty_field_opens_nothing() {
        let mut form = opened(&[]);
        assert_eq!(frame(&mut form, vec![key(egui::Key::Enter)]), Wants::Nothing);
    }

    #[test]
    fn newest_first_without_repeats() {
        let l = pushed(&[], "a://1");
        let l = pushed(&l, "b://2");
        let l = pushed(&l, "a://1");
        assert_eq!(l, ["a://1", "b://2"]);
    }

    #[test]
    fn at_most_ten() {
        let mut l = Vec::new();
        for i in 0..15 {
            l = pushed(&l, &format!("x://{i}"));
        }
        assert_eq!(l.len(), RECENT_MAX);
        assert_eq!(l[0], "x://14");
        assert_eq!(l[9], "x://5");
    }

    #[test]
    fn a_link_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("pomme-recent-{}", std::process::id()));
        let path = dir.join("pomme").join("recent-urls.txt");
        let mut before = Recent::load_from(Some(path.clone()));
        assert!(before.items.is_empty());
        before.add("pommehello:///tabs");
        before.add("myapp://settings");
        // The next run reads the same list back.
        assert_eq!(Recent::load_from(Some(path.clone())).items, ["myapp://settings", "pommehello:///tabs"]);
        before.clear();
        assert!(Recent::load_from(Some(path)).items.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_file_reads_back() {
        assert_eq!(parse("a://1\n\n  b://2  \na://1\n"), ["a://1", "b://2"]);
        // An unreadable or empty file is an empty list.
        assert!(parse("").is_empty());
        assert!(parse("\u{0}\n").len() <= 1);
    }
}
