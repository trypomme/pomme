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

pub(crate) struct Panel {
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
    state: egui_winit::State,
}

impl Panel {
    pub fn new(window: &Window) -> Panel {
        let ctx = egui::Context::default();
        // Nothing animates while the app's frames don't come: a steady caret.
        ctx.all_styles_mut(|s| s.visuals.text_cursor.blink = false);
        let state = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, window, Some(window.scale_factor() as f32), window.theme(), None);
        Panel { open: false, text: String::new(), error: None, waiting: None, fresh: false, recent: Recent::load(), ctx, state }
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

    /// A window event while the panel is open; true if egui wants the
    /// pointer where it is (the event is the panel's).
    pub fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let _ = self.state.on_window_event(window, event);
        self.ctx.is_pointer_over_egui()
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
    /// in window pixels): what to draw, and what the user asked for.
    pub fn frame(&mut self, window: &Window, screen: [f32; 4]) -> (Overlay, Wants) {
        let input = self.state.take_egui_input(window);
        let ppp = window.scale_factor() as f32;
        let [x, y, w, _] = screen.map(|v| v / ppp);
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
        self.state.handle_platform_output(window, output.platform_output);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        (Overlay { primitives, textures: output.textures_delta, pixels_per_point: output.pixels_per_point }, wants)
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
        let path = config_dir().map(|d| d.join("pomme").join("recent-urls.txt"));
        let items = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| parse(&s)).unwrap_or_default();
        Recent { items, path }
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
    fn the_file_reads_back() {
        assert_eq!(parse("a://1\n\n  b://2  \na://1\n"), ["a://1", "b://2"]);
        // An unreadable or empty file is an empty list.
        assert!(parse("").is_empty());
        assert!(parse("\u{0}\n").len() <= 1);
    }
}
