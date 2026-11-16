# Tasks

## 1. Measure on the Simulator

- [x] 1.1a Add the `rtl` probe (`crates/pomme-cli/tests/fixtures/rtl/`, with `en.lproj` and `ar.lproj`): views pinned with leading/trailing, left/right and center constraints, one after another, a subtree forced left to right, horizontal stack views (fill, fill equally, equal spacing) and a vertical one aligned to the leading edge, and a label with natural alignment; print every frame and the direction values. `fixtures/rtlplain` is the same app with no Arabic localization. (Found: a constraint's leading and trailing follow the direction of the view its items share, so a subtree forced left to right lays out left to right, though its views' own direction stays right to left; `forceRightToLeft` is right to left in any app.)
- [x] 1.1b Probes for the system UI, each with `en.lproj` and `ar.lproj`, recorded in `ar` and `en`: `fixtures/rtlnav` (a push and a pop sampled each frame, the bar's views, the back swipe's edges), `fixtures/rtlcontrols` (a switch off and on, a slider, a segmented control, a table cell, and images; their subviews and stills), and the tab bar probe's Arabic run (`fixtures/tabbar/ios26-ar.txt`). (Found: everything mirrors within its control or bar; the back button moves to the right and the swipe starts at the right edge (`edges` 8); `chevron.forward` points left though `flipsForRightToLeftLayoutDirection` is 0; a table row in Arabic is 2 pt taller.)
- [x] 1.2 Record it in `ar` and `en`, and `rtlplain` in `ar` (`fixtures/rtl/ios26-ar.txt`, `ios26-en.txt`, with stills; `fixtures/rtlplain/ios26-ar.txt`). Check: files in git.
- [x] 1.3 Record the push and pop in `ar`: the moving screen's position each frame (`fixtures/rtlnav/ios26-ar.txt`), which is the mirror of the English motion. Check: in git.
- [x] 1.4a Add Arabic and Hebrew text, and mixed Arabic with English and numbers, to the probe as labels, and record stills in `ar` and `en`. Check: files in git. (Found: natural alignment follows the view's direction, not the text's: Arabic text is on the left on an English phone; "Pomme مرحبا 123" shows as "Pomme 123 مرحبا", the digits after the Arabic word being Arabic numbers.)
- [x] 1.4b (`fixtures/rtlfield`) Text fields with mixed text in `ar` and `en`: alignment, the text rect, and `caretRectForPosition:` at every position. Check: files in git. (Found: a field's natural alignment follows its text's direction, in any app; after a character the caret is at its far edge in its own direction; a rounded-rect field's text rect starts 34 pt in, at the left, in a right-to-left app.)
- [x] 1.5 (`fixtures/swiftuirtl`, its own probe: an HStack, a leading-aligned Text, `.padding(.leading)`, `chevron.forward`; stills in `ar` and `en`.) Add a SwiftUI screen to the probe (`HStack`, `.padding(.leading)`, `.frame(alignment: .leading)`, an SF Symbol that mirrors, `.flipsForRightToLeftLayoutDirection(true)`) and record it in `ar`. Check: files in git.

## 2. Implement direction

- [x] 2.1 In `crates/pomme-hle/src/direction.rs`, answer `userInterfaceLayoutDirection` and the trait collection's `layoutDirection` from the app's chosen localization (decision 1). Check: `cargo test --test rtl` (all three runs).
- [x] 2.2 Store `semanticContentAttribute`; add `effectiveUserInterfaceLayoutDirection` and both `+userInterfaceLayoutDirectionForSemanticContentAttribute:` forms. Check: `cargo test --test rtl`.
- [x] 2.3 Auto Layout: leading and trailing are attributes of their own, resolved when solving in the direction of the view the constraint's items share (the constant turns around in right to left); left, right and center do not mirror. Check: frames from 1.2.
- [x] 2.4 `UIStackView` (`crates/pomme-hle/src/stackview.rs`; pomme had none): arranged subviews, axis, distribution (fill, fill equally, equal spacing, equal centering), alignment, spacing, its content size as its intrinsic size; rows run from the leading side. Check: frames from 1.2, and `cargo test -p pomme-hle stackview`.
- [x] 2.5 Natural text alignment and `NSWritingDirectionNatural` follow the direction: labels and text views default to natural alignment, drawn right-aligned in a right-to-left view; `defaultWritingDirectionForLanguage:` answers. Check: the labels match the stills in `ar` and `en` (looked at once).
- [x] 2.6 Navigation: the push comes from the left and the pop leaves to the left; the back button (and its chevron) on the right, its tap area too; the back swipe from the right edge (`navigation.rs`, `render.rs`). Check: `cargo test --test rtl` (`push_and_pop_from_the_left` against the measured curves mirrored, `back_button_on_the_right`, `back_swipe_from_the_right_edge`).
- [x] 2.7a Tab bar: mirrored, tabs in reverse order, a badge at its image's top left (`tabbar.rs`). Check: `cargo test --test tab_bar right_to_left_bar`.
- [x] 2.7b Switch, slider and segmented control: their parts mirrored in the control and touches mirrored on the way in (`valuecontrols.rs`). Check: `cargo test --test rtl controls_mirror tapping_the_leftmost_segment_selects_the_last`.
- [ ] 2.7c Table cells (image on the right, disclosure on the left, recorded in `fixtures/rtlcontrols`) once pomme has UITableView, which it doesn't yet.
- [x] 2.8 Images flipped for right to left (`imageFlippedForRightToLeftLayoutDirection`, `flipsForRightToLeftLayoutDirection`) draw mirrored in a right-to-left view; symbols with `forward` or `backward` in their name mirror there (`chevron.forward`, `arrow.backward`, now mapped and measured). Check: `cargo test --test rtl images_that_flip`.

## 3. Implement shaping and bidi

- [x] 3.1 Add `rustybuzz` (`crates/pomme-hle/src/shaping.rs`): text is shaped in segments of one face and font; measuring gives each UTF-16 unit its cluster's advance (TextKit keeps one glyph per unit, so its glyph APIs are unchanged); labels, text views, attributed strings and single-line UI text draw the shaped glyphs. Noto Sans Arabic, Hebrew and Thai join the fallback fonts (`fallback.rs`, was `cjk.rs`). Check: the Arabic labels match the stills; `cargo test --test intltext` (the font for each script).
- [x] 3.2 Re-run every text test. With kerning, the fonts that stand in for Apple's moved away from the recordings (the status bar clock, a tab title, segment widths), so they are shaped without kerning; an app's own fonts are kerned, as CoreText does. The status bar draws its plain colon (its measured lift), not Inter's contextual one. Check: `cargo test` passes with no baseline changed.
- [x] 3.3 Add `unicode-bidi`: each line's runs in the order shown, the paragraph's direction from its first strong letter (else the view's). Check: `cargo test -p pomme-hle shaping` (orders from the stills) and the stills.
- [x] 3.4a Text fields: natural alignment by the text's direction, `caretRectForPosition:` in bidi order (`shaping::caret_edges`), the rounded-rect border's text rect. Check: `cargo test --test rtl text_fields`.
- [ ] 3.4b Moving the caret with arrow keys and drawing it (pomme's fields draw no caret yet).

## 4. Frameworks

- [x] 4.1 SwiftUI: `layoutDirection` is the app's (`Hosting.swift`); views are laid out from the leading edge and each placed view is mirrored in its parent in right to left (`Nodes.swift`), UIKit controls mirroring their own parts. Check: `cargo test --test rtl swiftui_mirrors` (the shapes where the still has them).
- [x] 4.2 React Native: hello-expo declares English and Arabic (expo-localization's `supportedLocales`) and shows `I18nManager.isRTL`: true on an Arabic phone (React Native's own RCTI18nUtil, from the app's chosen localization; expo-localization allows right to left), false on an English one; the screen lays out right to left. react-native-screens sets the bar's direction through `appearanceWhenContainedInInstancesOfClasses:`, now answered (a proxy that keeps what's set). Check: `cargo test --test hello_expo i18n_manager_follows_the_apps_localization`. (`forceRTL` across a relaunch: not checked.)
- [x] 4.3 Flutter: hello-flutter supports English, Arabic and Japanese (`flutter_localizations`) and prints its direction; on an Arabic phone it's right to left. Check: `cargo test --test hello_flutter text_direction_follows_the_language`.

## 5. Tests and docs

- [ ] 5.1 Add all of this change's stills and frames to `crates/pomme-cli/tests/rtl.rs` as visual oracles. Check: `cargo test --test rtl`; look at each `side-by-side` once.
- [x] 5.2 The direction APIs are implemented (not answered generically by a permissive class), so the coverage report counts them as such; what isn't mirrored yet is listed in `README.md` and here.
- [x] 5.3 `README.md`: languages, regions, translations, other scripts and right to left, with example commands.
