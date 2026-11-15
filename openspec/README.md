# pomme: specs and plan

These documents describe how pomme must behave, from the CPU to the screen,
and the planned work. They are in two folders:

- `specs/`: **the behavior pomme must have.** Each spec is a list of
  requirements ("pomme SHALL ...") with scenarios ("WHEN ... THEN ..."), and
  tests check them. Specs are grouped by area (`core/`, `uikit/`, ...).
- `changes/`: **planned work.** Each change has a `proposal.md`
  (why and what), a `tasks.md` (the steps, with a check for each step),
  sometimes a `design.md` (how), and `specs/` (how the specs will change when
  the work is done).

## Useful commands

```bash
npx @fission-ai/openspec list --specs         # every spec
npx @fission-ai/openspec show uikit/navigation --type spec
npx @fission-ai/openspec list                 # the planned changes
npx @fission-ai/openspec show add-glass-buttons
npx @fission-ai/openspec validate --all --strict
npx @fission-ai/openspec archive <change>     # when every task is done: moves the change's spec updates into specs/
```

## How we work

1. **Measure first.** For anything iOS draws, lays out or times, the numbers
   come from the iOS 26 Simulator. A probe app
   (`crates/pomme-cli/tests/fixtures/<name>/`) prints what iOS did, and we
   save that output in git. See `testing/simulator-probes`.
2. **Then implement**, mostly in `crates/pomme-hle/src/`.
3. **Then test**: a test in `crates/pomme-cli/tests/` compares pomme with the
   saved measurements (numbers, pixel samples, and pictures: see
   "Pictures as references" below).

Never guess a number. If a task needs a number that is not measured, measure
it first. Everything must stay deterministic: the same app and the same
input give the same result every time (see `core/threads-and-time`).

## Specs by area

| Area | Specs | What the area covers |
| --- | --- | --- |
| `core/` | `mach-o-inspection`, `loader`, `cpu`, `threads-and-time`, `snapshots`, `sysroot` | Reading and loading the app, the arm64 CPU, threads and the virtual clock, saving the whole phone, the open-source system libraries pomme builds. |
| `runtime/` | `objc`, `libsystem`, `cpp-exceptions` | The Objective-C runtime, the C library and system calls, C++ exceptions. |
| `foundation/` | `collections-and-values`, `formats-and-parsing`, `files-and-bundles`, `user-defaults`, `timers-operations-notifications`, `key-value-observing` | Foundation's classes as apps and SDKs use them. |
| `data/` | `sqlite` | SQLite, as iOS ships it. |
| `security/` | `keychain`, `local-authentication`, `common-crypto` | The Keychain, Face ID and Touch ID prompts, digests and random bytes. |
| `store/` | `storekit` | The small part of StoreKit that SDKs touch at launch. |
| `networking/` | `url-loading`, `sockets-and-streams`, `network-path` | HTTP, raw sockets and streams, and the modeled network. |
| `system/` | `devices`, `os-version` | Which iPhone and which iOS version pomme presents. |
| `uikit/` | `app-lifecycle`, `views-and-layers`, `auto-layout`, `rendering`, `core-animation`, `text-and-fonts`, `images-and-symbols`, `status-bar`, `appearance`, `orientation`, `touch-and-gestures`, `scroll-views`, `text-input-and-keyboard`, `controls`, `navigation`, `modals-and-sheets`, `tab-bars`, `clipboard`, `web-views`, `accessibility` | UIKit, Core Animation and drawing: what is on the screen, and how it reacts. |
| `liquid-glass/` | `design`, `corner-configuration`, `effect-views`, `rendering`, `system-chrome` | iOS 26's design: when it is on, glass views, and the bars and controls in it. |
| `swiftui/` | `framework`, `views-and-modifiers`, `layout`, `state-and-data`, `navigation-and-tabs`, `forms-and-controls` | pomme's own SwiftUI framework. |
| `graphics/` | `metal` | Metal on the host GPU (through wgpu). |
| `frameworks/` | `flutter`, `react-native-skia` | App frameworks that draw with their own engines. |
| `sdks/` | `react-native`, `expo-modules`, `firebase` | Third-party SDKs we check end to end. |
| `hosts/` | `cli`, `desktop-window`, `browser`, `host-interface` | Where pomme runs: the command line, a Simulator-like window, a browser tab. |
| `tools/` | `coverage`, `profiler`, `debugging`, `frame-timing` | Tools for finding what is missing, slow or broken. |
| `testing/` | `e2e-driver`, `simulator-probes` | End-to-end tests, and how we measure iOS. |

## Planned changes

### Accessibility

Do these in order: the first one measures what exists, so the others build
on a tested base.

| # | Change | What it adds | Needs first |
| --- | --- | --- | --- |
| A1 | `add-accessibility-oracles` | iOS's own accessibility tree recorded from the Simulator (XCUITest snapshot, order checked once with VoiceOver on a device), and tests that hold pomme, the browser's ARIA layer (Playwright) and the Mac window's AX tree to it. | — |
| A2 | `add-accessibility-announcements` | Announcements and screen-change focus reach the host's screen reader; `isVoiceOverRunning` tells the truth; custom actions can be performed. | A1 |
| A3 | `add-dynamic-type-and-reduce-motion` | The text size setting (12 categories, measured text styles, `UIFontMetrics`, live changes) and Reduce Motion. | A1, and `add-visual-oracles` |

### Liquid Glass

Suggested order. A change lower in the list may need one above it.

| # | Change | What it adds | Needs first |
| --- | --- | --- | --- |
| 0 | `add-visual-oracles` | Screenshots and video frames from the Simulator as test references, pomme's approved renders as baselines, and a diff report with GIFs. Useful for every area, not only glass. | — |
| 1 | `render-system-chrome-as-glass` | Tab bar and back button painted as real glass over the content. | — |
| 2 | `add-glass-container-merging` | Close glass pieces in a container join into one shape. | — |
| 3 | `add-glass-edge-lensing` | The backdrop bends at the glass's edge. | (shares code with 2) |
| 4 | `add-interactive-glass` | Interactive glass grows and brightens when touched. | — |
| 5 | `add-glass-appear-animation` | Glass animates in and out inside `UIView.animate`. | — |
| 6 | `add-glass-buttons` | `UIButton.Configuration`, including `.glass()` and `.prominentGlass()`. | — |
| 7 | `add-glass-bar-button-items` | Navigation bar items and toolbars, on glass capsules. | 1 |
| 8 | `add-scroll-edge-effects` | The blur and fade under bars when content scrolls. | — |
| 9 | `add-tab-bar-minimize` | The tab bar gets small while scrolling; inline bottom accessory. | — |
| 10 | `add-swiftui-glass` | `.glassEffect`, `GlassEffectContainer`, `.buttonStyle(.glass)`, glass IDs. | 2, 6 |
| 11 | `add-glass-accessibility-settings` | Reduce Transparency and Increase Contrast. | 1 |

Every change from 1 to 11 ends with a "Visual oracles" task group, so it
needs change 0 first. Changes 1, 2 and 6 give the most value for real apps.
Change 7 is big, because it also adds bar button items themselves.

## Known gaps (not planned yet)

Things the specs do not cover, because pomme does not do them. Each one is
a good candidate for a new change. Open one with `openspec new change <name>`
and copy the layout of an existing change.

- **core:** no JIT (the interpreter is the only CPU); dispatch sources on
  file descriptors, signals, processes and Mach ports stop the run; a
  `__ulock_wait` that nothing can release stops the run.
- **runtime:** Objective-C and CF objects are never freed (`dealloc` never
  runs); only a short list of classes answers missing methods generically;
  some calls are stubs (`fsync`, `readlink`, `pthread_atfork`, ...); C++
  exceptions cannot unwind through pomme's own frames.
- **foundation:** `addObserverForName:object:queue:usingBlock:` ignores the
  queue; KVO fires only for manual will/did-change and view geometry;
  `NSPointerArray` weak entries are not cleared; `NSRegularExpression` has no
  lookaround or backreferences; only the Gregorian calendar; no time zone
  names.
- **security / store:** no HMAC, ciphers or key derivation in CommonCrypto;
  no device passcode; in-app purchases stop the run.
- **networking:** no sockets or TLS in the browser; reachability for a host
  name does not resolve the name.
- **graphics / frameworks:** no GPU in the browser, so Metal apps (Skia,
  Flutter) draw nothing there; Skia-era Flutter and Defold are planned only.
- **uikit:** no gradients (`CAGradientLayer`, CG gradients) and no group
  opacity; coordinate conversion ignores transforms; storyboards do not
  restore archived properties; one scene only; no kerning or ligatures,
  and text uses Inter instead of SF Pro; the Auto Layout solver is not
  Cassowary (conflicts settle differently); only pan gestures have built-in
  logic (no tap, long-press, swipe, pinch or rotation recognizers);
  `requireGestureRecognizerToFail:` is ignored; no hardware keyboard and no
  edit menus; form sheets and popovers are not acted on; the More list has
  no Edit; text-only clipboard; web views do not load on Windows and Linux,
  and have no file URLs or popups; accessibility has no escape gesture,
  magic tap, Bold Text or Large Content Viewer (announcements, custom
  actions, Dynamic Type and Reduce Motion are planned above).
- **swiftui:** no `NavigationLink` pushes; picker choices do not open;
  no `ScrollView`, lazy stacks or grids; `.sheet`, `.alert`, `.toolbar`,
  `withAnimation`, transitions and gesture modifiers are stubs; no
  `@Observable`, `@StateObject`, `@EnvironmentObject` or `@AppStorage`;
  shapes cannot be filled or stroked as views; no page tab style.
- **system:** no iPad profiles.
- **hosts / testing:** no menus in the Linux window; no hardware keyboard
  events in the browser; no CI; some `POMME_*` debug variables have no user
  docs.
- **housekeeping:** some module comments in the code are out of date and
  say features are missing that exist (in `render.rs`, `navigation.rs`,
  `libsystem.rs`, `posix.rs`, `objc.rs` and `pomme-loader`). The specs
  follow the code.

## Pictures as references (visual oracles)

Like Chromatic for web apps, each test can compare pictures:

- **iOS oracle**: a cropped screenshot (or, for motion, frames from a screen
  recording) from the iOS 26 Simulator. This is the truth. pomme must be
  close to it, within a tolerance.
- **pomme baseline**: what pomme drew when someone last approved it. Any
  change fails the test until someone reviews it and runs the tests with
  `POMME_ACCEPT=1`.

When a test fails, open `target/oracle-report/<test>/index.html`. It shows
the iOS image, the pomme image and the difference, and for motion a GIF
with iOS and pomme side by side. See `changes/add-visual-oracles/`.

## Words

| Word | Meaning |
| --- | --- |
| Backdrop | Everything already drawn behind a piece of glass. |
| Capsule | A shape with fully round ends, like a pill. |
| Chrome | The parts UIKit draws for the app: bars, back button, switches. |
| Deterministic | The same input always gives the same result, so runs can be repeated exactly. |
| Fixture / probe | A small iOS app we run on the Simulator (and in pomme) to measure what iOS does. |
| Guest | The iOS app and the code it runs inside pomme. The **host** is the real machine pomme runs on. |
| HLE | High-level emulation: pomme writes its own version of a system library instead of running Apple's. |
| Oracle | The reference answer a test compares with (for example, Simulator pictures). |
| Stub | A placeholder function. In pomme a stub usually stops the app and names what is missing. |
| Sysroot | pomme's copy of the system libraries that run as guest code (`vendor/sysroot`). |
| Virtual time | pomme's own clock: one guest instruction is one nanosecond. |
