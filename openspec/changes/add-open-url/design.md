# Design: Open URL

## Context

pomme passes a link only at launch (`uikit.rs`: `launch_url`,
`launch_nsurl`, `url_contexts`, and `application:openURL:options:` for apps
without scenes). The window's menu items reach the app as host events:
Device ▸ Relaunch App is `Action::Relaunch` in `pomme-desktop`, then
`Event::Relaunch`, handled in `pomme-hle/src/host.rs`. The window draws
with winit, wgpu and tiny-skia, and has no way to draw text or take typed
text for itself.

hello-expo's `SceneDelegate.swift` implements `scene:openURLContexts:` and
forwards links to Expo's linking (after Firebase Auth's own check), and its
`Info.plist` lists the `pommehello` scheme. So it is a complete test app.

## Goals / Non-Goals

**Goals:**
- Deliver a link to a running app exactly as iOS does.
- A text field in the window that works the same on macOS, Windows and
  Linux, with paste.
- Keep the window's look when the panel is closed.

**Non-Goals:**
- Anything specific to one framework (Expo, React Native, Flutter).
- Universal links and a browser.

## Decisions

### A host event, like Relaunch

`Event::OpenUrl(String)` in `crates/pomme-hle/src/host.rs`. It is handled
on the main thread between run-loop turns, as other events are. The
handler (`crates/pomme-hle/src/uikit.rs`, next to the launch code):

1. Parses the text as an `NSURL` (`URLWithString:`). Nil: refuse.
2. Checks the scheme against `CFBundleURLTypes` in the app's `Info.plist`.
   Not listed: refuse.
3. With scenes: for each connected scene, builds the `UIOpenURLContext`
   (reusing what `url_contexts` builds, with the options measured in task
   1.1) and calls `scene:openURLContexts:` if the delegate implements it.
   Without scenes: calls `application:openURL:options:` on the app
   delegate if it implements it.

A refusal comes back to the host as a reason string, through a new
`Host::url_refused(reason)` call with a default that does nothing. The
window shows it in the panel; the driver turns it into a failed step.

### The driver step

`App::open_url(&str)` in `crates/pomme-hle/src/driver.rs` sends the event
in the test's turn and settles, as `tap` does. It reuses the same handler,
so tests cover what the window does.

### egui for the panel, only its drawing parts

`egui`, `egui-winit` and `egui-wgpu` 0.37 need wgpu `^30.0`, winit
`^0.30.13` and accesskit_winit `^0.34.0`: the versions pomme already uses.
So there is one copy of each, and egui draws into pomme's own wgpu surface.

We do not use `eframe` (egui's own app framework), because it would own the
window. pomme keeps its borderless, see-through window and its presenter
(`present.rs`).

- `crates/pomme-desktop/src/open_url.rs` (new): the panel's state (open or
  not, the text, the last error), `egui::Context`, `egui_winit::State`.
  `fn ui(&mut self, ctx)` lays out the field, the error line and the
  recent list, with egui's default look.
- `present.rs`: after drawing the phone, if the panel is open, run an
  `egui_wgpu::Renderer` pass on the same frame, clipped to the phone's
  screen rectangle. When the panel is closed nothing extra is drawn, so the
  window is as before.
- `lib.rs`: while the panel is open, window events go to `egui_winit` first
  and not to the app (no `Event::Key` or touches). `Action::OpenUrl` opens
  the panel. Return sends `Event::OpenUrl`.
- `menu.rs`: File ▸ Open URL… (⌘L / Ctrl+L) and File ▸ Open Recent URL
  (the ten links and Clear), rebuilt when the list changes. On Windows these
  are in the right-click menu, as the other items are.

egui draws premultiplied colours, the same as the presenter's phone
picture, so the panel blends correctly on a see-through window too.

### Recent links on disk

A plain text file, one link per line, newest first:
`<config dir>/pomme/recent-urls.txt` (`~/Library/Application Support`,
`%APPDATA%`, `$XDG_CONFIG_HOME` or `~/.config`). A link is added only when
the app accepted it. Read at start, written on each change. An unreadable
file is treated as empty.

## Risks / Trade-offs

- **egui and pomme must move together.** A future wgpu or winit upgrade in
  pomme needs an egui release on the same versions. Today they match.
- **Build time and size grow** with egui. It is only in `pomme-desktop`,
  not in the emulator core or the browser build.
- **The panel looks like egui**, not like a native dialog. Accepted: the
  user chose egui's defaults for now.
- **Keyboard focus.** Keys must not leak to the app while the panel is
  open (for example, Return typing a newline into a focused text field in
  the app). The tests in task 3.3 check this.
