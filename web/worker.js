// The emulator's thread. `run` doesn't return while the app runs: with JSPI
// (WebAssembly JavaScript Promise Integration) the emulator pauses in
// `pommePause` after each frame and while the app is idle, which gives the
// browser a break: its GPU work is sent, its answers arrive, and the page's
// messages (touches, screen reader actions, web view events) reach this
// worker's onmessage. They wait in queues until the emulator takes them
// (`pommeTakeInput`, `pommeTakeWebViewEvents`). pomme needs JSPI: without it
// the worker says so ({type: 'nojspi'}) and stops.
'use strict';

importScripts('pkg/pomme_web.js');

self.pommeFrame = (rgba, width, height) => {
  const copy = rgba.slice(); // a view into wasm memory: copy, then transfer
  self.postMessage({ type: 'frame', rgba: copy.buffer, width, height }, [copy.buffer]);
};
self.pommeLog = (text) => self.postMessage({ type: 'log', text });
self.pommeWebView = (json) => self.postMessage({ type: 'webview', json });
self.pommeDisplay = (width, height, path) => self.postMessage({ type: 'display', width, height, path });
self.pommeAccessibility = (json) => self.postMessage({ type: 'accessibility', json });
self.pommeStopped = (text) => self.postMessage({ type: 'stopped', text });

// The page's events, until the emulator takes them.
let input = []; // 4 ints each: what it is, two values, 0 (see crates/pomme-web/src/lib.rs)
let webViewEvents = []; // JSON `[page, event]` strings
// The idle pause waiting for input, if any: a new input event ends it at once.
let wake = null;

self.pommeTakeInput = () => {
  const events = Int32Array.from(input);
  input = [];
  return events;
};
self.pommeTakeWebViewEvents = () => {
  const events = webViewEvents;
  webViewEvents = [];
  return events;
};

// pommePause(ms): a promise the emulator waits on while the browser runs.
// ms 0 is one new task (a MessageChannel message: setTimeout(0) can be slowed
// to 4 ms). Otherwise up to ms, ended early by new input; input already
// waiting ends it on the next task. (Web view events don't end it: the
// emulator takes them every few milliseconds while pages run.)
const channel = new MessageChannel();
const queued = [];
channel.port1.onmessage = () => queued.shift()();
function nextTask() {
  return new Promise((resolve) => { queued.push(resolve); channel.port2.postMessage(0); });
}
function pauseAsync(ms) {
  if (ms <= 0 || input.length) return nextTask();
  return new Promise((resolve) => {
    const timer = setTimeout(() => { wake = null; resolve(); }, ms);
    wake = () => { clearTimeout(timer); wake = null; resolve(); };
  });
}

const hasJspi = () => typeof WebAssembly.Suspending === 'function' && typeof WebAssembly.promising === 'function';

// The glue calls its imports through small functions of its own, and `run`
// itself. To pause, the import must be a WebAssembly.Suspending and `run` must
// be called through WebAssembly.promising: so swap them in as pomme is
// instantiated. Both only pass numbers, so they need none of the glue's help.
function useJspi() {
  const instantiate = WebAssembly.instantiate;
  const streaming = WebAssembly.instantiateStreaming;
  const patchImports = (imports) => {
    for (const ns of Object.values(imports)) {
      for (const name of Object.keys(ns)) {
        if (name.startsWith('__wbg_pommePause_')) ns[name] = new WebAssembly.Suspending(pauseAsync);
      }
    }
    return imports;
  };
  const wrap = (result) => {
    const real = result.instance.exports;
    const promisingRun = WebAssembly.promising(real.run);
    const exports = { ...real, run: (...args) => { promisingRun(...args).catch((e) => self.pommeStopped(String((e && e.stack) || e))); } };
    return { instance: { exports }, module: result.module };
  };
  WebAssembly.instantiateStreaming = async (source, imports) => wrap(await streaming(source, patchImports(imports)));
  WebAssembly.instantiate = async (source, imports) => {
    const r = await instantiate(source, patchImports(imports));
    return r instanceof WebAssembly.Instance ? r : wrap(r);
  };
}

async function start({ bundle, scale, ios, frameStats }) {
  if (!hasJspi()) {
    self.postMessage({ type: 'nojspi' });
    return;
  }
  try {
    // Metal needs WebGPU here, in the worker: say so now if there's none.
    const adapter = self.navigator.gpu ? await self.navigator.gpu.requestAdapter().catch(() => null) : null;
    if (!adapter) self.pommeLog('WebGPU: off. Apps run, but Metal apps (such as Flutter apps) cannot draw.');
    useJspi();
    self.pommeLog('Downloading the app…');
    await wasm_bindgen('pkg/pomme_web_bg.wasm');
    const res = await fetch(bundle);
    if (!res.ok) throw new Error(`${bundle}: ${res.status} ${res.statusText}`);
    const bytes = new Uint8Array(await res.arrayBuffer());
    self.pommeLog(`Booting (${(bytes.length / 1e6).toFixed(0)} MB)…`);
    wasm_bindgen.run(bytes, scale, ios || '', !!frameStats); // reports through pommeStopped
  } catch (err) {
    self.pommeStopped(String((err && err.stack) || err));
  }
}

self.onmessage = (e) => {
  const m = e.data;
  if (m.type === 'input') {
    input.push(m.kind | 0, m.a | 0, m.b | 0, 0);
    if (wake) wake();
  } else if (m.type === 'webview') {
    webViewEvents.push(m.json);
  } else if (m.type === 'start') {
    start(m);
  }
};
