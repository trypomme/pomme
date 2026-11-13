// The page: shows the worker's frames, sends it touches, and hosts its web views (webview.js).
// Touches go into a SharedArrayBuffer ring (see crates/pomme-web/src/lib.rs).
'use strict';

const SCALE = 2; // pixels per point
const HEADER = 8, RING = 256;
const params = new URLSearchParams(location.search);
const bundle = params.get('app') || 'helloexpo.pfs';
const ios = params.get('ios') || ''; // e.g. ?ios=18.6

const canvas = document.getElementById('screen');
const status = document.getElementById('status');
const ctx = canvas.getContext('2d');

if (!self.crossOriginIsolated) {
  status.textContent = 'This page needs cross-origin isolation (serve it with scripts/serve-web.py).';
  throw new Error('not cross-origin isolated');
}

const input = new Int32Array(new SharedArrayBuffer((HEADER + RING * 4) * 4));
const worker = new Worker('worker.js');
WebViews.attach(document.getElementById('layer'), canvas, 390, () => Atomics.notify(input, 0));
const started = performance.now();
let frames = 0;

worker.onmessage = (e) => {
  const m = e.data;
  if (m.type === 'frame') {
    if (canvas.width !== m.width || canvas.height !== m.height) {
      canvas.width = m.width;
      canvas.height = m.height;
    }
    ctx.putImageData(new ImageData(new Uint8ClampedArray(m.rgba), m.width, m.height), 0, 0);
    if (frames++ === 0) status.textContent = `First frame after ${((performance.now() - started) / 1000).toFixed(1)} s. Tap away.`;
  } else if (m.type === 'webview') {
    WebViews.command(JSON.parse(m.json));
  } else if (m.type === 'log') {
    if (!frames) status.textContent = m.text;
    console.log('[pomme]', m.text);
  } else if (m.type === 'stopped') {
    status.textContent = m.text;
    console.warn('[pomme]', m.text);
  }
};
worker.postMessage({ type: 'start', bundle, input, scale: SCALE, ios, webViews: WebViews.sab });

// Pointer → touch phases (UITouchPhase: began 0, moved 1, ended 3, cancelled 4), in points.
function send(phase, ev) {
  const r = canvas.getBoundingClientRect();
  const x = (ev.clientX - r.left) * 390 / r.width;
  const y = (ev.clientY - r.top) * 844 / r.height;
  const n = Atomics.load(input, 0);
  const at = HEADER + (n % RING) * 4;
  input[at] = phase;
  input[at + 1] = Math.round(x * 100);
  input[at + 2] = Math.round(y * 100);
  Atomics.store(input, 0, n + 1);
  Atomics.notify(input, 0);
}
// The canvas lets input through: what isn't over a web view's page reaches the layer behind it.
const layer = document.getElementById('layer');
let down = false;
layer.addEventListener('pointerdown', (e) => { down = true; layer.setPointerCapture(e.pointerId); send(0, e); });
layer.addEventListener('pointermove', (e) => { if (down) send(1, e); });
layer.addEventListener('pointerup', (e) => { if (down) { down = false; send(3, e); } });
layer.addEventListener('pointercancel', (e) => { if (down) { down = false; send(4, e); } });
