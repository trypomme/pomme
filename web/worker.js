// The emulator's thread. `run` doesn't return while the app runs. With JSPI
// (WebAssembly JavaScript Promise Integration) the emulator pauses in
// `pommePause` after each frame and while the app is idle, which gives the
// browser a break: its GPU work is sent and its answers arrive. Without JSPI
// it blocks in Atomics.wait instead, and never gives the browser a break.
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

let input = null; // the page's input ring (an Int32Array on a SharedArrayBuffer)

// pommePause(ms, seen): wait up to ms, or until the page writes input past `seen`.
// Blocking, for browsers without JSPI (as the emulator always did):
function pauseBlocking(ms, seen) {
  if (ms > 0) Atomics.wait(input, 0, seen, ms);
}
// Pausing, with JSPI: a promise the emulator waits on while the browser runs.
// ms 0 is one new task (a MessageChannel message: setTimeout(0) can be slowed to 4 ms).
const channel = new MessageChannel();
const queued = [];
channel.port1.onmessage = () => queued.shift()();
function nextTask() {
  return new Promise((resolve) => { queued.push(resolve); channel.port2.postMessage(0); });
}
function pauseAsync(ms, seen) {
  if (ms <= 0) return nextTask();
  if (Atomics.waitAsync) {
    const w = Atomics.waitAsync(input, 0, seen, ms);
    return w.async ? w.value : nextTask();
  }
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// The glue calls its imports through small functions of its own, and `run`
// itself. To pause, the import must be a WebAssembly.Suspending and `run` must
// be called through WebAssembly.promising: so swap them in as pomme is
// instantiated. Both only pass numbers, so they need none of the glue's help.
function useJspi(jspi) {
  const instantiate = WebAssembly.instantiate;
  const streaming = WebAssembly.instantiateStreaming;
  const patchImports = (imports) => {
    for (const ns of Object.values(imports)) {
      for (const name of Object.keys(ns)) {
        if (name.startsWith('__wbg_pommePause_')) ns[name] = jspi ? new WebAssembly.Suspending(pauseAsync) : pauseBlocking;
      }
    }
    return imports;
  };
  const wrap = (result) => {
    if (!jspi) return result;
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

self.onmessage = async (e) => {
  const { bundle, scale, ios, webViews, frameStats } = e.data;
  input = e.data.input;
  const jspi = e.data.jspi !== false && typeof WebAssembly.Suspending === 'function' && typeof WebAssembly.promising === 'function';
  try {
    self.pommeLog(jspi ? 'JSPI: on' : 'JSPI: off. Apps run, but Metal apps (such as Flutter apps) cannot draw.');
    if (jspi) {
      // Metal needs WebGPU here, in the worker: say so now if there's none.
      const adapter = self.navigator.gpu ? await self.navigator.gpu.requestAdapter().catch(() => null) : null;
      if (!adapter) self.pommeLog('WebGPU: off. Apps run, but Metal apps (such as Flutter apps) cannot draw.');
    }
    useJspi(jspi);
    self.pommeLog('Downloading the app…');
    await wasm_bindgen('pkg/pomme_web_bg.wasm');
    const res = await fetch(bundle);
    if (!res.ok) throw new Error(`${bundle}: ${res.status} ${res.statusText}`);
    const bytes = new Uint8Array(await res.arrayBuffer());
    self.pommeLog(`Booting (${(bytes.length / 1e6).toFixed(0)} MB)…`);
    wasm_bindgen.run(bytes, input, scale, ios || '', webViews, !!frameStats, jspi); // reports through pommeStopped
  } catch (err) {
    self.pommeStopped(String((err && err.stack) || err));
  }
};
