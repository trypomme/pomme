// The emulator's thread. It blocks while the app runs (sleeping in
// Atomics.wait when the app is idle), so it never returns to the event loop:
// everything it needs comes in the 'start' message.
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

self.onmessage = async (e) => {
  const { bundle, input, scale, ios, webViews, frameStats } = e.data;
  try {
    self.pommeLog('Downloading the app…');
    await wasm_bindgen('pkg/pomme_web_bg.wasm');
    const res = await fetch(bundle);
    if (!res.ok) throw new Error(`${bundle}: ${res.status} ${res.statusText}`);
    const bytes = new Uint8Array(await res.arrayBuffer());
    self.pommeLog(`Booting (${(bytes.length / 1e6).toFixed(0)} MB)…`);
    const why = wasm_bindgen.run(bytes, input, scale, ios || '', webViews, !!frameStats);
    self.postMessage({ type: 'stopped', text: why });
  } catch (err) {
    self.postMessage({ type: 'stopped', text: String(err && err.stack || err) });
  }
};
