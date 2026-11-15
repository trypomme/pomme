# frameworks/react-native-skia Specification

## Purpose

Run React Native Skia, which draws with Skia's Ganesh backend on Metal,
and show its canvas inside the React Native screen.

Words used here:

- **Skia**: a 2D graphics library. React Native Skia uses it to draw
  shapes, gradients and paths.
- **Ganesh**: Skia's GPU backend. It turns its own shader language (SkSL)
  into Metal Shading Language (MSL) while the app runs.
- **MSAA**: drawing with several samples per pixel, for smooth edges.

Test app: `apps/hello-skia`; tests: `crates/pomme-cli/tests/hello_skia.rs`
(skipped without the app and the sysroot; pixel tests are skipped without a
GPU).

## Requirements

### Requirement: Skia starts on Metal and presents frames

pomme SHALL let Skia create its Metal context, compile its generated
shaders and present frames of the canvas, without stopping.

#### Scenario: Canvas presented

- **WHEN** hello-skia runs for 6 s
- **THEN** the view tree has `SkiaPictureView (45.0, 290.3, 300.0×300.0)`, at least 10 shader sources were compiled (one with `fragment Outputs fragmentMain`), and at least one frame was presented

### Requirement: Skia's frame looks right

Skia's frame SHALL run on the host GPU and show in the canvas, matching
the iOS Simulator within 12 per channel. Stroked paths drawn through
Skia's MSAA atlas (stencil, then cover) SHALL be filled only on the stroke.

#### Scenario: Shapes and gradient

- **WHEN** the frame is read at 3×
- **THEN** the rounded rectangle is #1e293b, outside its corner is white, the circle goes from rose to amber, and the chevron is white on its stroke and #1e293b just under it
