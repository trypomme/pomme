// The page: shows the worker's frames, sends it touches, hosts its web views (webview.js) and
// shows its accessibility tree to the browser's screen reader (accessibility.js).
// Input goes to the worker as messages (see crates/pomme-web/src/lib.rs). No shared memory, so the
// page needs no special headers, and runs embedded in another site's page. It needs JSPI.
'use strict';

const SCALE = 2; // pixels per point
const AX_EVENT = 16; // + the action: a screen reader's, on an element
const params = new URLSearchParams(location.search);
const bundle = params.get('app') || 'helloexpo.pfs';
const ios = params.get('ios') || ''; // e.g. ?ios=18.6
const frameStats = params.has('stats'); // ?stats: frame rate and stutters in the console
const jspi = !params.has('nojspi') && typeof WebAssembly.Suspending === 'function' && typeof WebAssembly.promising === 'function'; // ?nojspi: as if the browser had none
const NO_JSPI = 'pomme needs a browser with JSPI: Chrome or Edge 137, Firefox 153, Safari 27, or later.';

const canvas = document.getElementById('screen');
const status = document.getElementById('status');
const ctx = canvas.getContext('2d');

if (!jspi) {
  status.textContent = NO_JSPI;
  console.warn('[pomme]', NO_JSPI);
  throw new Error('no JSPI');
}

const worker = new Worker('worker.js');
WebViews.attach(document.getElementById('layer'), canvas, 390, (json) => worker.postMessage({ type: 'webview', json }));
Accessibility.attach(document.getElementById('layer'), (action, id) => {
  push(AX_EVENT + action, id % 2 ** 32, Math.floor(id / 2 ** 32));
}, () => [canvas.width / SCALE, canvas.height / SCALE]);
const started = performance.now();
let frames = 0;
let notice = ''; // what this browser can't do, shown with the status

worker.onmessage = (e) => {
  const m = e.data;
  if (m.type === 'frame') {
    if (canvas.width !== m.width || canvas.height !== m.height) {
      canvas.width = m.width;
      canvas.height = m.height;
    }
    ctx.putImageData(new ImageData(new Uint8ClampedArray(m.rgba), m.width, m.height), 0, 0);
    window.pommeFrames = frames + 1; // for tests (web/tests/run.mjs)
    if (frames++ === 0) status.textContent = `First frame after ${((performance.now() - started) / 1000).toFixed(1)} s. Tap away.${notice ? ' ' + notice : ''}`;
  } else if (m.type === 'display') {
    // The display's own shape clips the screen (its continuous corners, as transitions' cards have).
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${m.width} ${m.height}" preserveAspectRatio="none"><path d="${m.path}"/></svg>`;
    const mask = `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
    for (const k of ['maskImage', 'webkitMaskImage']) layer.style[k] = mask;
    for (const k of ['maskSize', 'webkitMaskSize']) layer.style[k] = '100% 100%';
    layer.style.borderRadius = '0';
  } else if (m.type === 'accessibility') {
    Accessibility.update(JSON.parse(m.json));
  } else if (m.type === 'webview') {
    WebViews.command(JSON.parse(m.json));
  } else if (m.type === 'log') {
    if (m.text.startsWith('WebGPU: off')) notice = 'This browser has no WebGPU in workers, so Metal apps (such as Flutter apps) cannot draw.';
    if (!frames) status.textContent = m.text;
    console.log('[pomme]', m.text);
  } else if (m.type === 'nojspi') {
    status.textContent = NO_JSPI;
    console.warn('[pomme]', NO_JSPI);
  } else if (m.type === 'stopped') {
    status.textContent = m.text;
    console.warn('[pomme]', m.text);
  }
};
worker.postMessage({ type: 'start', bundle, scale: SCALE, ios, frameStats });

// Pointer → touch phases (UITouchPhase: began 0, moved 1, ended 3, cancelled 4), in points.
function send(phase, ev) {
  const r = canvas.getBoundingClientRect();
  const x = (ev.clientX - r.left) * 390 / r.width;
  const y = (ev.clientY - r.top) * 844 / r.height;
  push(phase, Math.round(x * 100), Math.round(y * 100));
}
// An event to the worker: what it is, and two ints.
function push(kind, a, b) {
  worker.postMessage({ type: 'input', kind, a, b });
}
// The canvas lets input through: what isn't over a web view's page reaches the layer behind it.
const layer = document.getElementById('layer');
let down = false;
layer.addEventListener('pointerdown', (e) => { down = true; layer.setPointerCapture(e.pointerId); send(0, e); });
layer.addEventListener('pointermove', (e) => { if (down) send(1, e); });
layer.addEventListener('pointerup', (e) => { if (down) { down = false; send(3, e); } });
layer.addEventListener('pointercancel', (e) => { if (down) { down = false; send(4, e); } });
