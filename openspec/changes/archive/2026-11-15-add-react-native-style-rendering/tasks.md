# Tasks

## 1. Catalog and measurements

- [x] 1.1 Create `apps/hello-uniwind`: Expo 58, React Native 0.88, Uniwind with Tailwind 4; pages of tiles (colors, borders, effects, transforms, text, decoration, layout, states), chosen by `pommeuniwind:///<page>` or the launch argument `-page <page>`. Check: it runs on the iOS 26 Simulator.
- [x] 1.2 Write `scripts/uniwind/simulator.sh` and `scripts/uniwind/pomme.sh` (screenshots of every page) and `scripts/uniwind/compare.swift` (side by side with a diff). Check: they make the screenshots and diffs.
- [x] 1.3 Find what pomme lacks with `pomme coverage` on every page. Check: the list of missing and ignored calls.

## 2. Layers

- [x] 2.1 `CAShapeLayer`: fill and stroke its path, with its fill rule. Check: box shadows' masks cut their inside out.
- [x] 2.2 Layer masks and `compositingFilter`: draw such a layer as a group, cut by the mask's alpha, blended onto what's below. Check: `shadow-*`, `ring-*`, `mix-blend-*` and `brightness-50` match the Simulator.
- [x] 2.3 Brightness above 100%: a multiply layer with a color component over 1 scales what's below. Check: `brightness-150`.
- [x] 2.4 `CAGradientLayer` (axial, radial), `CAReplicatorLayer`, `CATransform3DMakeTranslation` and the other `CATransform3D` functions, `CATransform3DIdentity`, the `kCAFillRule…`, `kCAGradientLayer…`, `kCAFilter…`, `kCALineCap…` and `kCALineJoin…` constants. Check: `bg-linear-*` tiles match.
- [x] 2.5 `CGContextSetLineDash`. Check: `border-dashed`, `border-dotted`, `outline-dashed`.
- [x] 2.6 Shadow paths that reach past the layer's bounds (a spread, an offset). Check: `shadow-[0_0_0_4px_#000]`, `shadow-[0_8px_0_0_…]`.

## 3. Text

- [x] 3.1 Paragraph minimum and maximum line height, and the baseline offset. Check: `leading-*` and every `text-*` (which sets a line height) lay out as on iOS.
- [x] 3.2 `extraLineFragmentRect` and `extraLineFragmentUsedRect` (zero unless the text ends in a newline). Check: texts with `numberOfLines` are as tall as on iOS.
- [x] 3.3 Letter spacing (`NSKernAttributeName`). Check: `tracking-*`.
- [x] 3.4 Underline and strikethrough styles and colors, text shadows (`NSShadow`), italic system fonts (`fontDescriptorWithSymbolicTraits:`, `fontWithDescriptor:size:`). Check: the decoration page.
- [x] 3.5 Tail truncation with an ellipsis. Check: `truncate`, and long text with `numberOfLines={1}`.

## 4. Tests and docs

- [x] 4.1 Write `scripts/uniwind/points.py` and `scripts/uniwind/record.sh`; record `fixtures/uniwind/` (points, light and dark samples). Check: 1104 samples.
- [x] 4.2 Add `crates/pomme-cli/tests/hello_uniwind.rs`. Check: it passes; with gradients and shape layers turned off, 6 of its 7 tests fail.
- [x] 4.3 Run the whole test suite. Check: no regressions.
- [x] 4.4 Update the specs (this change) and the README. Check: `openspec validate --all --strict`.
