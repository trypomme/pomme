# graphics/metal Specification

## Purpose

Give apps Apple's Metal GPU API and run their GPU work on the host's GPU
through wgpu. Shaders written in Metal Shading Language (MSL) are
translated to WGSL. The first users are Skia (Ganesh) and Flutter
(Impeller).

Words used here:

- **wgpu**: a cross-platform GPU library. It runs on Vulkan, Metal, DirectX
  and WebGPU.
- **MSL / WGSL**: the shader languages of Metal and of WebGPU (wgpu).
- **Command buffer**: a list of GPU commands. Encoders record into it;
  `commit` runs it.
- **Drawable**: the texture a `CAMetalLayer` shows on screen.
- **Sample count**: how many samples a texture keeps per pixel (MSAA).

## Requirements

### Requirement: Metal objects behave like Metal's

pomme SHALL provide devices, command queues, command buffers, render and
blit encoders, buffers, textures, samplers, depth-stencil states, shader
libraries, pipelines and `CAMetalLayer` drawables. Buffer contents SHALL
live in guest memory. A descriptor's values SHALL be copied when a state
object is made from it.

#### Scenario: Skia's objects

- **WHEN** hello-skia creates its Metal context and draws a frame
- **THEN** no Metal call stops the app, and a drawable is presented

### Requirement: Textures report what was asked for

A texture SHALL report the width, height, pixel format, usage and sample
count it was made with. A pipeline SHALL keep its raster sample count.

#### Scenario: Multisampled texture

- **WHEN** Skia makes a multisampled texture for its MSAA atlas
- **THEN** the texture's `sampleCount` is the count Skia asked for, not 1, and Skia's stroked paths render

### Requirement: Commands run on the host GPU

At `commit`, pomme SHALL run the commands in order: buffer copies in guest
memory, render passes on wgpu with the buffers' current contents, then
read presented drawables back into their layer. Shaders compiled from
source SHALL be translated MSL → WGSL.

#### Scenario: Frame on the GPU

- **WHEN** hello-skia commits its frame on a machine with a GPU
- **THEN** the canvas shows Skia's rounded rectangle, gradient circle and chevron

### Requirement: Flutter's precompiled libraries and IOSurfaces

For `newLibraryWithData:`, pomme SHALL give each function the MSL made for
that engine version by `scripts/flutter/impeller-msl.sh`, and SHALL say so
when there is none. pomme SHALL provide IOSurfaces, textures on them, and
layers whose contents are an IOSurface.

#### Scenario: Impeller library

- **WHEN** Flutter loads its precompiled shader library
- **THEN** its functions get the substitute MSL, and frames show through the layer's IOSurface

### Requirement: MSL translates to valid WGSL

The translator SHALL turn every shader that Skia generated for hello-skia,
and every Impeller shader for hello-flutter's engine (unspecialized and
with every function constant set), into WGSL that naga parses and
validates. Each translation SHALL report its entry points and bindings.

#### Scenario: Shader fixtures

- **WHEN** `cargo test -p pomme-msl` runs
- **THEN** all Skia fixtures and all (at least 59) Impeller fixtures translate and validate

### Requirement: No GPU is not a crash

When the host has no GPU, or the build has no GPU support, pomme SHALL keep
running the app, still do the buffer copies, print the GPU error once, and
let tests see it. `POMME_DUMP_MSL=dir` SHALL write every shader source to
`dir`.

#### Scenario: Headless machine

- **WHEN** hello-flutter runs where wgpu finds no adapter
- **THEN** the app keeps running and the GPU error is recorded, so the pixel test skips

### Requirement: Commands run on the browser's GPU

In the browser, pomme SHALL run Metal's commands on the browser's GPU
(WebGPU), with the same WGSL and in the same order as on a native host. A
frame drawn in the browser SHALL have the same pixels as the same frame
drawn by a native run, apart from differences between GPUs.

#### Scenario: hello-skia in the browser

- **WHEN** hello-skia commits its frame in a browser with JSPI and WebGPU
- **THEN** the canvas shows Skia's rounded rectangle, gradient circle and chevron, as in a native run

### Requirement: Waiting for the GPU does not block the browser

When pomme must wait for the GPU in the browser (to set the GPU up, or to
get a frame's pixels back), it SHALL wait by pausing, so the browser can
finish the GPU work. It SHALL NOT wait in a way that keeps the browser from
running.

#### Scenario: Pixels come back

- **WHEN** a Metal app presents a drawable in a browser with JSPI and WebGPU
- **THEN** the drawable's pixels come back and the layer shows them in the same frame
