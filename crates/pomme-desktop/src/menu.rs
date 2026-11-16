//! The Simulator's menus, as far as pomme has them: File ▸ Open URL… /
//! Open Recent URL (pomme's own: a link into the running app), Save Screen /
//! Record Screen, Device ▸ Rotate Left / Rotate Right / Relaunch App (pomme's
//! own: quit the app and start it again, keeping its data), and Features ▸ Face ID
//! ▸ Enrolled / Matching Face / Non-matching Face. On macOS they're the menu bar, with the Simulator's
//! shortcuts. On Windows the window has no frame to hang a menu bar on, so
//! they're a right-click menu on the phone's bezel (the shortcuts work there
//! too, through the window's own key handling). Linux has the shortcuts only:
//! native menus there need GTK, which winit doesn't use.

use muda::accelerator::{Accelerator, CMD_OR_CTRL, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use crate::Action;
use pomme_hle::keychain::Face;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

pub(crate) struct Menus {
    menu: Menu,
    enrolled: CheckMenuItem,
    record: MenuItem,
    /// File ▸ Open Recent URL: the recent links, then Clear.
    recent: Submenu,
}

/// The ids of File ▸ Open Recent URL's items: `recent-<n>` for the n-th link, and Clear.
const RECENT_PREFIX: &str = "recent-";
const RECENT_CLEAR: &str = "recent-clear";

fn key(mods: Modifiers, code: Code) -> Option<Accelerator> {
    Some(Accelerator::new(mods, code))
}

impl Menus {
    /// The menus; Face ID starts enrolled or not, as the run does. Chosen
    /// items arrive as `wrap(action)` events on the window's event loop.
    pub(crate) fn new<T: Send + 'static>(enrolled: bool, proxy: EventLoopProxy<T>, wrap: fn(Action) -> T) -> Self {
        let item = |text: &str, accel| MenuItem::new(text, true, accel);
        let rotate_left = item("Rotate Left", key(CMD_OR_CTRL, Code::ArrowLeft));
        let rotate_right = item("Rotate Right", key(CMD_OR_CTRL, Code::ArrowRight));
        let relaunch = item("Relaunch App", key(CMD_OR_CTRL | Modifiers::SHIFT, Code::KeyR));
        let face = CMD_OR_CTRL | Modifiers::ALT;
        let enrolled = CheckMenuItem::new("Enrolled", true, enrolled, key(face, Code::KeyE));
        let matching = item("Matching Face", key(face, Code::KeyM));
        let non_matching = item("Non-matching Face", key(face, Code::KeyN));
        let close = item("Close Window", key(CMD_OR_CTRL, Code::KeyW));
        let open_url = item("Open URL…", key(CMD_OR_CTRL, Code::KeyL));
        let recent = Submenu::new("Open Recent URL", true);
        let save = item("Save Screen", key(CMD_OR_CTRL, Code::KeyS));
        let record = item("Record Screen", key(CMD_OR_CTRL, Code::KeyR));
        let sep = PredefinedMenuItem::separator();
        let mut actions = vec![
            (rotate_left.id().clone(), Action::Rotate(true)),
            (rotate_right.id().clone(), Action::Rotate(false)),
            (relaunch.id().clone(), Action::Relaunch),
            (enrolled.id().clone(), Action::Face(Face::ToggleEnrolled)),
            (matching.id().clone(), Action::Face(Face::Matching)),
            (non_matching.id().clone(), Action::Face(Face::NonMatching)),
            (close.id().clone(), Action::Close),
            (save.id().clone(), Action::SaveScreen),
            (record.id().clone(), Action::Record),
            (open_url.id().clone(), Action::OpenUrl),
        ];
        let face_id = Submenu::with_items("Face ID", true, &[&enrolled, &sep, &matching, &non_matching]).expect("menu");
        let device = Submenu::with_items("Device", true, &[&rotate_left, &rotate_right, &sep, &relaunch]).expect("menu");
        let features = Submenu::with_items("Features", true, &[&face_id]).expect("menu");

        let menu = Menu::new();
        #[cfg(target_os = "macos")]
        {
            // The application menu (named for the app by macOS), then File as the Simulator has them.
            let quit = item("Quit pomme", key(CMD_OR_CTRL, Code::KeyQ));
            let app = Submenu::with_items(
                "pomme",
                true,
                &[&PredefinedMenuItem::hide(None), &PredefinedMenuItem::hide_others(None), &PredefinedMenuItem::show_all(None), &sep, &quit],
            )
            .expect("menu");
            let file = Submenu::with_items("File", true, &[&open_url, &recent, &sep, &close, &sep, &save, &record]).expect("menu");
            let window = Submenu::with_items("Window", true, &[&PredefinedMenuItem::minimize(None)]).expect("menu");
            menu.append_items(&[&app, &file, &device, &features, &window]).expect("menu");
            window.set_as_windows_menu_for_nsapp();
            actions.push((quit.id().clone(), Action::Close));
            Self::finish(menu, enrolled, record, recent, actions, proxy, wrap)
        }
        #[cfg(not(target_os = "macos"))]
        {
            menu.append_items(&[&open_url, &recent, &sep, &save, &record, &sep, &device, &features, &sep, &close]).expect("menu");
            Self::finish(menu, enrolled, record, recent, actions, proxy, wrap)
        }
    }

    fn finish<T: Send + 'static>(menu: Menu, enrolled: CheckMenuItem, record: MenuItem, recent: Submenu, actions: Vec<(MenuId, Action)>, proxy: EventLoopProxy<T>, wrap: fn(Action) -> T) -> Self {
        let proxy = std::sync::Mutex::new(proxy);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let a = match actions.iter().find(|(id, _)| *id == e.id) {
                Some(&(_, a)) => Some(a),
                None if e.id.0 == RECENT_CLEAR => Some(Action::ClearRecent),
                None => e.id.0.strip_prefix(RECENT_PREFIX).and_then(|n| n.parse().ok()).map(Action::OpenRecent),
            };
            if let Some(a) = a {
                let _ = proxy.lock().unwrap().send_event(wrap(a));
            }
        }));
        let menus = Self { menu, enrolled, record, recent };
        menus.set_recent(&[]);
        menus
    }

    /// File ▸ Open Recent URL lists these links (newest first), then Clear.
    pub(crate) fn set_recent(&self, urls: &[String]) {
        while self.recent.remove_at(0).is_some() {}
        for (i, url) in urls.iter().enumerate() {
            let _ = self.recent.append(&MenuItem::with_id(format!("{RECENT_PREFIX}{i}"), url, true, None));
        }
        if !urls.is_empty() {
            let _ = self.recent.append(&PredefinedMenuItem::separator());
        }
        let _ = self.recent.append(&MenuItem::with_id(RECENT_CLEAR, "Clear", !urls.is_empty(), None));
    }

    /// Put the menus up for the app (macOS: the menu bar).
    pub(crate) fn install(&self) {
        #[cfg(target_os = "macos")]
        self.menu.init_for_nsapp();
    }

    /// Show the menus at the cursor (a right-click on the bezel, where there's no menu bar).
    #[allow(unused_variables)]
    pub(crate) fn pop_up(&self, window: &Window) {
        #[cfg(target_os = "windows")]
        {
            use muda::ContextMenu;
            use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = window.window_handle()
                && let RawWindowHandle::Win32(h) = h.as_raw()
            {
                // SAFETY: the HWND is the live window's.
                unsafe { self.menu.show_context_menu_for_hwnd(h.hwnd.get(), None) };
            }
        }
    }

    /// Record Screen becomes Stop Recording while it records.
    pub(crate) fn recording(&self, on: bool) {
        self.record.set_text(if on { "Stop Recording" } else { "Record Screen" });
    }

    /// Keep the Enrolled check in step when it's toggled from the keyboard.
    pub(crate) fn toggled_enrolled(&self) {
        self.enrolled.set_checked(!self.enrolled.is_checked());
    }
}
