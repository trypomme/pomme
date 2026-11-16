# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 A probe app `crates/pomme-cli/tests/fixtures/openurl/` in two variants (with a scene delegate, without scenes) that prints which callback it got, the URL, and every key and value of the options (`UISceneOpenURLOptions`: `sourceApplication`, `annotation`, `openInPlace`, `eventAttribution`; or the options dictionary). Run it on the iOS 26 Simulator and open a link with `xcrun simctl openurl` while it runs; save the output beside the probe. Check: the output names the callback and all options for both variants.
- [ ] 1.2 On the same Simulator, run hello-expo, open `pommehello:///tabs` with `xcrun simctl openurl`, and note what the screen shows. Check: a line in the probe's notes.

## 2. Delivering links

- [ ] 2.1 `crates/pomme-hle/src/host.rs`: `Event::OpenUrl(String)` and `Host::url_refused`; `crates/pomme-hle/src/uikit.rs`: parse, check the scheme, deliver to scenes or the app delegate with the measured options. Check: `crates/pomme-cli/tests/open_url.rs` runs both probe variants and compares with the saved output.
- [ ] 2.2 Refusals: a scheme the app does not list, and text that is not a URL. Check: cases in `open_url.rs`.
- [ ] 2.3 `crates/pomme-hle/src/driver.rs`: `App::open_url`. Check: a case in `crates/pomme-cli/tests/hello_expo.rs` opens `pommehello:///tabs` and waits for the tabs screen.

## 3. The window

- [ ] 3.1 `crates/pomme-desktop/Cargo.toml`: `egui`, `egui-winit`, `egui-wgpu` 0.37. `present.rs`: an egui pass on the phone's frame only while the panel is open. Check: `cargo build -p pomme-desktop` with one copy of wgpu and winit in `cargo tree -d`.
- [ ] 3.2 `crates/pomme-desktop/src/open_url.rs` and `lib.rs`: the panel (field with the last link selected, Return, Escape, the error line, the recent list); events go to the panel while it is open. Check: by hand with hello-expo on macOS: ⌘L, paste `pommehello:///tabs`, Return shows the tabs; `https://example.com` shows the reason.
- [ ] 3.3 Keys do not reach the app while the panel is open. Check: by hand, with hello-expo's text field focused: ⌘L, type, Escape; the app's field is unchanged.
- [ ] 3.4 `menu.rs`: File ▸ Open URL… (⌘L / Ctrl+L) and File ▸ Open Recent URL with Clear; the recent links file. Check: a unit test for the recent list (ten items, newest first, no repeats, an unreadable file reads as empty); by hand, a link survives a restart.
- [ ] 3.5 The window's look with the panel closed. Check: `cargo test --test capture` (or the window tests that compare frames) still passes, and a screenshot after opening and closing the panel equals one taken before.

## 4. Docs

- [ ] 4.1 README: File ▸ Open URL…, with two examples: testing a deep link, and connecting a development build to Metro with the link `npx expo start` prints. Check: updated.
- [ ] 4.2 `hosts/desktop-window` main spec: add Open URL to "The Simulator's menus" list when this change is archived. Check: updated at archive time.
