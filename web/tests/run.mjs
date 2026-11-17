// Drive the browser build in headless Chromium: open the page, wait for the
// app, tap, wait for what the app shows, and save the screen as a PNG.
// scripts/web-test.sh serves web/ and calls this; see web/tests/README.md.
//
// Usage: node web/tests/run.mjs --url URL [steps…] [--out file.png] [--timeout s]
// Steps run in order:
//   --wait TEXT      until the page's status or log has TEXT ("First frame" for the first frame)
//   --see TEXT       until the app's accessibility tree shows TEXT (a label or value)
//   --gone TEXT      until it doesn't (the screen that showed it went)
//   --tap X,Y        a tap at (X, Y) in points
//   --taps "X,Y X,Y…"  taps back to back (30 ms apart), not waiting for the app
//   --sleep MS       wait MS milliseconds
// Options:
//   --out FILE       save the screen (the canvas) as a PNG at the end
//   --timeout S      seconds for each wait (default 60)
//   --no-jspi        start the browser without JSPI
//   --no-webgpu      start the browser without WebGPU
//   --browser NAME   chromium (default) or webkit
//   --embed PORT     open a plain page on localhost:PORT (another origin) that
//                    shows --url in an iframe, and run the steps in the frame
//                    (web/tests/embed.mjs)
//   --compare REF    at the end, compare the screen with REF (a PNG, e.g. from
//                    `pomme run --screenshot`) at --points "x,y x,y…" (in points),
//                    each channel within --tolerance (default 12); REF is at
//                    --ref-scale pixels per point (default 3)
// It exits 1 when a step fails, after printing the page's log.
import { chromium, webkit } from 'playwright';
import { writeFileSync, readFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { serveOuter } from './embed.mjs';

const args = process.argv.slice(2);
const opt = { url: null, out: null, timeout: 60, jspi: true, webgpu: true, browser: 'chromium', embed: null, steps: [], compare: null, points: [], tolerance: 12, refScale: 3 };
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '--url') opt.url = args[++i];
  else if (a === '--out') opt.out = args[++i];
  else if (a === '--timeout') opt.timeout = Number(args[++i]);
  else if (a === '--no-jspi') opt.jspi = false;
  else if (a === '--no-webgpu') opt.webgpu = false;
  else if (a === '--browser') opt.browser = args[++i];
  else if (a === '--embed') opt.embed = Number(args[++i]);
  else if (a === '--compare') opt.compare = args[++i];
  else if (a === '--points') opt.points = args[++i].trim().split(/\s+/).map((p) => p.split(',').map(Number));
  else if (a === '--tolerance') opt.tolerance = Number(args[++i]);
  else if (a === '--ref-scale') opt.refScale = Number(args[++i]);
  else if (['--wait', '--see', '--gone', '--tap', '--taps', '--sleep'].includes(a)) opt.steps.push([a.slice(2), args[++i]]);
  else throw new Error(`unknown argument: ${a}`);
}
if (!opt.url) throw new Error('--url is required');

const flags = opt.webgpu ? ['--enable-unsafe-webgpu'] : ['--disable-features=WebGPU,WebGPUService', '--disable-gpu'];
if (!opt.jspi) flags.push('--js-flags=--no-experimental-wasm-jspi', '--disable-features=WebAssemblyJSPromiseIntegration');
if (!['chromium', 'webkit'].includes(opt.browser)) throw new Error(`unknown browser: ${opt.browser}`);
const browser = opt.browser === 'webkit' ? await webkit.launch() : await chromium.launch({ args: flags });
const page = await browser.newPage({ viewport: opt.embed ? { width: 520, height: 1060 } : { width: 480, height: 1000 } });
const outer = opt.embed ? await serveOuter(opt.embed, opt.url) : null;
let app = page; // the pomme page: the page itself, or the frame it's embedded in
const log = [];
const started = Date.now();
const stamp = () => `${((Date.now() - started) / 1000).toFixed(1)}s`;
page.on('console', (m) => log.push(`${stamp()} ${m.type() === 'log' ? '' : m.type() + ': '}${m.text()}`));
page.on('pageerror', (e) => log.push(`${stamp()} page error: ${e.message}`));

let failed = null;
try {
  if (outer) {
    await page.goto(outer.url);
    app = await (await page.waitForSelector('iframe#pomme')).contentFrame();
    await app.waitForSelector('#status');
    log.push(`${stamp()} embedded in ${outer.url}: ${await app.evaluate(() => `${location.origin}, cross-origin isolated: ${self.crossOriginIsolated}`)}`);
  } else {
    await page.goto(opt.url);
  }
  for (const [kind, value] of opt.steps) {
    log.push(`${stamp()} -- ${kind} ${value}`);
    const timeout = opt.timeout * 1000;
    if (kind === 'wait') {
      const until = Date.now() + timeout;
      for (;;) {
        const status = await app.$eval('#status', (e) => e.textContent);
        if (status.includes(value) || log.some((l) => l.includes(value) && !l.includes(' -- wait '))) break;
        const stopped = log.find((l) => l.includes('warning: [pomme]'));
        if (stopped) throw new Error(`the app stopped while waiting for "${value}": ${stopped}`);
        if (Date.now() > until) throw new Error(`timed out waiting for "${value}"`);
        await page.waitForTimeout(100);
      }
    } else if (kind === 'see' || kind === 'gone') {
      await app.waitForFunction(
        ([t, want]) => want === [...document.querySelectorAll('#accessibility *')].some((e) =>
          (e.getAttribute('aria-label') || '').includes(t) || (e.textContent || '').includes(t) ||
          (e.getAttribute('aria-valuetext') || '').includes(t)),
        [value, kind === 'see'], { timeout, polling: 100 });
    } else if (kind === 'tap' || kind === 'taps') {
      const r = await app.locator('#screen').boundingBox();
      const held = kind === 'tap' ? 80 : 30;
      for (const at of value.trim().split(/\s+/)) {
        const [x, y] = at.split(',').map(Number);
        const px = r.x + (x * r.width) / 390, py = r.y + (y * r.height) / 844;
        await page.mouse.move(px, py);
        await page.mouse.down();
        await page.waitForTimeout(held);
        await page.mouse.up();
        if (kind === 'taps') await page.waitForTimeout(held);
      }
    } else if (kind === 'sleep') {
      await page.waitForTimeout(Number(value));
    }
  }
} catch (e) {
  failed = e.message.split('\n')[0];
}

if (opt.out) {
  // The accessibility tree goes out just before a frame is drawn: wait for
  // the next frame (up to 10 s), so the picture shows what the steps saw.
  const seen = await app.evaluate(() => window.pommeFrames || 0);
  await app.waitForFunction((n) => (window.pommeFrames || 0) > n, seen, { timeout: 10000 }).catch(() => {});
  const png = await app.$eval('#screen', (c) => c.toDataURL('image/png').split(',')[1]);
  mkdirSync(dirname(opt.out), { recursive: true });
  writeFileSync(opt.out, Buffer.from(png, 'base64'));
  log.push(`${stamp()} saved the screen to ${opt.out}`);
}
if (opt.compare && !failed) {
  // The page reads both pictures: the screen's canvas and the reference.
  const ref = readFileSync(opt.compare).toString('base64');
  const rows = await app.evaluate(async ({ ref, points, refScale }) => {
    const img = new Image();
    img.src = `data:image/png;base64,${ref}`;
    await img.decode();
    const c = new OffscreenCanvas(img.width, img.height);
    const rc = c.getContext('2d');
    rc.drawImage(img, 0, 0);
    const screen = document.getElementById('screen');
    const sc = screen.getContext('2d');
    const scale = screen.width / 390;
    return points.map(([x, y]) => ({
      at: [x, y],
      browser: [...sc.getImageData(Math.round(x * scale), Math.round(y * scale), 1, 1).data.slice(0, 3)],
      ref: [...rc.getImageData(Math.round(x * refScale), Math.round(y * refScale), 1, 1).data.slice(0, 3)],
    }));
  }, { ref, points: opt.points, refScale: opt.refScale });
  for (const r of rows) {
    const diff = Math.max(...r.browser.map((v, i) => Math.abs(v - r.ref[i])));
    const ok = diff <= opt.tolerance;
    log.push(`${stamp()} compare (${r.at}): browser rgb(${r.browser}) reference rgb(${r.ref}) ${ok ? 'ok' : `off by ${diff}`}`);
    if (!ok && !failed) failed = `the screen differs from ${opt.compare} at (${r.at}) by ${diff}`;
  }
}
const status = await app.$eval('#status', (e) => e.textContent).catch(() => '?');
await browser.close();
outer?.close();

console.log(`status: ${status}`);
for (const line of log) console.log(line);
if (failed) {
  console.log(`FAILED: ${failed}`);
  process.exit(1);
}
console.log('ok');
