//! The Simulator's menus, as far as pomme has them: Device ▸ Rotate Left /
//! Rotate Right, and Features ▸ Face ID ▸ Enrolled / Matching Face /
//! Non-matching Face. On macOS they're the menu bar, with the Simulator's
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
}

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
        let face = CMD_OR_CTRL | Modifiers::ALT;
        let enrolled = CheckMenuItem::new("Enrolled", true, enrolled, key(face, Code::KeyE));
        let matching = item("Matching Face", key(face, Code::KeyM));
        let non_matching = item("Non-matching Face", key(face, Code::KeyN));
        let close = item("Close Window", key(CMD_OR_CTRL, Code::KeyW));
        let sep = PredefinedMenuItem::separator();
        let face_id = Submenu::with_items("Face ID", true, &[&enrolled, &sep, &matching, &non_matching]).expect("menu");
        let device = Submenu::with_items("Device", true, &[&rotate_left, &rotate_right]).expect("menu");
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
            let file = Submenu::with_items("File", true, &[&close]).expect("menu");
            let window = Submenu::with_items("Window", true, &[&PredefinedMenuItem::minimize(None)]).expect("menu");
            menu.append_items(&[&app, &file, &device, &features, &window]).expect("menu");
            window.set_as_windows_menu_for_nsapp();
            let actions = actions(&rotate_left, &rotate_right, &enrolled, &matching, &non_matching, &close, Some(&quit));
            Self::finish(menu, enrolled, actions, proxy, wrap)
        }
        #[cfg(not(target_os = "macos"))]
        {
            menu.append_items(&[&device, &features, &sep, &close]).expect("menu");
            let actions = actions(&rotate_left, &rotate_right, &enrolled, &matching, &non_matching, &close, None);
            Self::finish(menu, enrolled, actions, proxy, wrap)
        }
    }

    fn finish<T: Send + 'static>(menu: Menu, enrolled: CheckMenuItem, actions: Vec<(MenuId, Action)>, proxy: EventLoopProxy<T>, wrap: fn(Action) -> T) -> Self {
        let proxy = std::sync::Mutex::new(proxy);
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some(&(_, a)) = actions.iter().find(|(id, _)| *id == e.id) {
                let _ = proxy.lock().unwrap().send_event(wrap(a));
            }
        }));
        Self { menu, enrolled }
    }

    /// Put the menus up for the app (macOS: the menu bar).
    pub(crate) fn install(&self) {
        #[cfg(target_os = "macos")]
        self.menu.init_for_nsapp();
    }

    /// Whether the menus handle this key themselves (their shortcuts, on macOS),
    /// so the window mustn't act on it too.
    pub(crate) fn owns_shortcuts(&self) -> bool {
        cfg!(target_os = "macos")
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

    /// Keep the Enrolled check in step when it's toggled from the keyboard.
    pub(crate) fn toggled_enrolled(&self) {
        self.enrolled.set_checked(!self.enrolled.is_checked());
    }
}

fn actions(
    rotate_left: &MenuItem,
    rotate_right: &MenuItem,
    enrolled: &CheckMenuItem,
    matching: &MenuItem,
    non_matching: &MenuItem,
    close: &MenuItem,
    quit: Option<&MenuItem>,
) -> Vec<(MenuId, Action)> {
    let mut a = vec![
        (rotate_left.id().clone(), Action::Rotate(true)),
        (rotate_right.id().clone(), Action::Rotate(false)),
        (enrolled.id().clone(), Action::Face(Face::ToggleEnrolled)),
        (matching.id().clone(), Action::Face(Face::Matching)),
        (non_matching.id().clone(), Action::Face(Face::NonMatching)),
        (close.id().clone(), Action::Close),
    ];
    if let Some(q) = quit {
        a.push((q.id().clone(), Action::Close));
    }
    a
}
