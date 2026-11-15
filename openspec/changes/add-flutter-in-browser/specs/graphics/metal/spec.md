# Spec Delta

## ADDED Requirements

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
