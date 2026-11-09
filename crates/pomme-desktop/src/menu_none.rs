//! No native menus here (on Linux they'd need GTK, which winit doesn't use):
//! the window's keyboard shortcuts do what the Simulator's menus do.

use crate::Action;
use winit::event_loop::EventLoopProxy;
use winit::window::Window;

pub(crate) struct Menus;

impl Menus {
    pub(crate) fn new<T: Send + 'static>(_enrolled: bool, _proxy: EventLoopProxy<T>, _wrap: fn(Action) -> T) -> Self {
        Menus
    }
    pub(crate) fn install(&self) {}
    pub(crate) fn owns_shortcuts(&self) -> bool {
        false
    }
    pub(crate) fn pop_up(&self, _window: &Window) {}
    pub(crate) fn toggled_enrolled(&self) {}
}
