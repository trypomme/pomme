# Tasks

## 1. Saved data

- [x] 1.1 Defaults domains written to `Library/Preferences/<domain>.plist` before a relaunch, and read from there on first use. Check: the `relaunch` probe's default survives. Found on the Simulator: the file appears soon after a change, at no fixed time; pomme writes it on `synchronize` and at a relaunch.
- [x] 1.2 Keychain items saved as plists and restored. Check: the probe's item survives. (The Simulator can't check this with an unsigned probe: SecItemAdd answers -34018.)

## 2. Relaunch

- [x] 2.1 `Stop::Relaunch`, the boot snapshot, and the relaunch steps in `drive`; `App::relaunch()`. Check: `cargo test --test relaunch`.
- [ ] 2.2 The desktop window's Relaunch menu item (⌘⇧R). Check: by hand (not yet done: it builds).

## 3. Tests and docs

- [x] 3.1 A `relaunch` probe (a default, a file, a keychain item, a global) and `crates/pomme-cli/tests/relaunch.rs`. Check: passes.
- [x] 3.2 hello-expo: a button calling `I18nManager.forceRTL(true)`; a test taps it, relaunches, and sees `isRTL` true. Check: `cargo test --test hello_expo force_rtl`.
- [x] 3.3 README: relaunching. Check: updated.
