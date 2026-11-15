// Drive the browser build in headless Chromium: open the page, wait for the
// app, tap, wait for what the app shows, and save the screen as a PNG.
// scripts/web-test.sh serves web/ and calls this; see web/tests/README.md.
//
// Usage: node web/tests/run.mjs --url URL [steps…] [--out file.png] [--timeout s]
// Steps run in order:
//   --wait TEXT      until the page's status or log has TEXT ("First frame" for the first frame)
//   --see TEXT       until the app's accessibility tree shows TEXT (a label or value)
//   --tap X,Y        a tap at (X, Y) in points
//   --sleep MS       wait MS milliseconds
// Options:
//   --out FILE       save the screen (the canvas) as a PNG at the end
//   --timeout S      seconds for each wait (default 60)
//   --no-jspi        start the browser without JSPI
// It exits 1 when a step fails, after printing the page's log.
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const args = process.argv.slice(2);
const opt = { url: null, out: null, timeout: 60, jspi: true, steps: [] };
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '--url') opt.url = args[++i];
  else if (a === '--out') opt.out = args[++i];
  else if (a === '--timeout') opt.timeout = Number(args[++i]);
  else if (a === '--no-jspi') opt.jspi = false;
  else if (['--wait', '--see', '--tap', '--sleep'].includes(a)) opt.steps.push([a.slice(2), args[++i]]);
  else throw new Error(`unknown argument: ${a}`);
}
if (!opt.url) throw new Error('--url is required');

const flags = ['--enable-unsafe-webgpu'];
if (!opt.jspi) flags.push('--js-flags=--no-experimental-wasm-jspi', '--disable-features=WebAssemblyJSPromiseIntegration');
const browser = await chromium.launch({ args: flags });
const page = await browser.newPage({ viewport: { width: 480, height: 1000 } });
const log = [];
const started = Date.now();
const stamp = () => `${((Date.now() - started) / 1000).toFixed(1)}s`;
page.on('console', (m) => log.push(`${stamp()} ${m.type() === 'log' ? '' : m.type() + ': '}${m.text()}`));
page.on('pageerror', (e) => log.push(`${stamp()} page error: ${e.message}`));

let failed = null;
try {
  await page.goto(opt.url);
  for (const [kind, value] of opt.steps) {
    log.push(`${stamp()} -- ${kind} ${value}`);
    const timeout = opt.timeout * 1000;
    if (kind === 'wait') {
      const until = Date.now() + timeout;
      for (;;) {
        const status = await page.$eval('#status', (e) => e.textContent);
        if (status.includes(value) || log.some((l) => l.includes(value) && !l.includes(' -- wait '))) break;
        const stopped = log.find((l) => l.includes('warning: [pomme]'));
        if (stopped) throw new Error(`the app stopped while waiting for "${value}": ${stopped}`);
        if (Date.now() > until) throw new Error(`timed out waiting for "${value}"`);
        await page.waitForTimeout(100);
      }
    } else if (kind === 'see') {
      await page.waitForFunction(
        (t) => [...document.querySelectorAll('#accessibility *')].some((e) =>
          (e.getAttribute('aria-label') || '').includes(t) || (e.textContent || '').includes(t) ||
          (e.getAttribute('aria-valuetext') || '').includes(t)),
        value, { timeout, polling: 100 });
    } else if (kind === 'tap') {
      const [x, y] = value.split(',').map(Number);
      const r = await page.locator('#screen').boundingBox();
      const px = r.x + (x * r.width) / 390, py = r.y + (y * r.height) / 844;
      await page.mouse.move(px, py);
      await page.mouse.down();
      await page.waitForTimeout(80);
      await page.mouse.up();
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
  const seen = await page.evaluate(() => window.pommeFrames || 0);
  await page.waitForFunction((n) => (window.pommeFrames || 0) > n, seen, { timeout: 10000 }).catch(() => {});
  const png = await page.$eval('#screen', (c) => c.toDataURL('image/png').split(',')[1]);
  mkdirSync(dirname(opt.out), { recursive: true });
  writeFileSync(opt.out, Buffer.from(png, 'base64'));
  log.push(`${stamp()} saved the screen to ${opt.out}`);
}
const status = await page.$eval('#status', (e) => e.textContent).catch(() => '?');
await browser.close();

console.log(`status: ${status}`);
for (const line of log) console.log(line);
if (failed) {
  console.log(`FAILED: ${failed}`);
  process.exit(1);
}
console.log('ok');
