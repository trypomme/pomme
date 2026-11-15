# Tasks

## 1. Measure on the Simulator

- [ ] 1.1 Create the probe `crates/pomme-cli/tests/fixtures/swiftuiglass/main.swift`: `Text` with `.glassEffect()`, with `.glassEffect(.clear, in: .rect(cornerRadius: 16))`, with `.glassEffect(.regular.tint(.blue).interactive())`, a `GlassEffectContainer(spacing: 40)` with two glass circles 20 pt apart, and buttons with `.buttonStyle(.glass)` and `.buttonStyle(.glassProminent)`. Over the same stripes as the glass probe. Check: it runs on the iOS 26 Simulator.
- [ ] 1.2 Record the frames (`METRIC` lines) as `ios26.txt` and the UIKit view tree each one makes (`VIEW` lines) as `measured/swiftuiglass.log`. Check: the files are in git.
- [ ] 1.3 Record pixels at sample points as `ios26-pixels.txt`. Check: the file is in git.

## 2. glassEffect

- [ ] 2.1 Find the generated stubs for `glassEffect(_:in:)` and `Glass` in `vendor/swiftui-gen/` after `scripts/swiftui/build.sh`. Copy their exact signatures. Check: write them in a comment at the top of a new file `scripts/swiftui/SwiftUI/Glass.swift`.
- [ ] 2.2 Implement `Glass` (`.regular`, `.clear`, `.identity`, `tint(_:)`, `interactive(_:)`) as a value that maps to a `UIGlassEffect`. Check: `scripts/swiftui/build.sh check` passes.
- [ ] 2.3 Implement `.glassEffect(_:in:)` as a node with a `UIVisualEffectView` behind its content, with the shape turned into a corner configuration. Check: the probe's frames match `ios26.txt` within 0.5 pt.
- [ ] 2.4 Mask the effect view with the path for shapes that have no corner configuration. Check: a custom shape in the probe.

## 3. Container and buttons

- [ ] 3.1 Implement `GlassEffectContainer(spacing:content:)` with a `UIGlassContainerEffect`. Check: the probe's container frames match. (The merged look needs `add-glass-container-merging`.)
- [ ] 3.2 Implement `.buttonStyle(.glass)` and `.buttonStyle(.glassProminent)` using the glass button look from `add-glass-buttons`. Check: the probe's button frames match.

## 4. IDs and morphing

- [ ] 4.1 Implement `glassEffectID(_:in:)` and `glassEffectUnion(id:namespace:)`: store the IDs; join the members of a union into one shape. Check: a union of two views in the probe draws one shape.
- [ ] 4.2 Implement `glassEffectTransition(_:)` with no animation first (`.identity` and `.matchedGeometry` both just switch). Check: the app runs; the views appear and go away.
- [ ] 4.3 Record a morph on the Simulator (a glass button that changes into a wider one inside `withAnimation`) and measure the frames per time. Check: `measured/morph.txt` is in git.
- [ ] 4.4 Animate the shape between frames for views with the same ID. Check: frames at several times match `morph.txt` within 1 pt.

## 5. Tests and docs

- [ ] 5.1 Add `crates/pomme-cli/tests/swiftui_glass.rs` that checks frames, view tree and pixels against the probe's recordings. Check: `cargo test --test swiftui_glass` passes.
- [ ] 5.2 Run `scripts/swiftui/check-exports.sh`. Check: no missing or extra symbols.
- [ ] 5.3 Add SwiftUI glass to `README.md`. Check: the README lists the APIs.

## 6. Visual oracles

Needs `add-visual-oracles` first. See `openspec/specs/visual-oracles/spec.md` once it is archived.

- [ ] 6.1 Write `fixtures/swiftuiglass/oracle/regions.txt` with a region per glass view and button, and mask the text. Record light and dark. Check: the PNGs are in git and within the size budget.
- [ ] 6.2 Draw the marker when the morph's `withAnimation` starts, add a `FRAMES morph …` line, and record. Check: the frames are in git.
- [ ] 6.3 Call `check_still` and `check_motion` in `swiftui_glass.rs` and accept the pomme baselines. Check: the tests pass.
