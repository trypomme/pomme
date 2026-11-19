// Another site's page in a web view (loadRequest:), with and without the
// isolation headers pomme's page used to need (COOP same-origin, COEP
// require-corp). The web view is an <iframe> (web/webview.js), so under
// require-corp a site that doesn't send Cross-Origin-Resource-Policy or its
// own COEP was refused; without the headers it loads. This drives webview.js
// directly, with no app: a host page on 127.0.0.1 and a "third-party" page on
// localhost (another origin) that sends no special headers.
//
//   node webview-frames.mjs [chromium|webkit]
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, webkit } from 'playwright';

const here = dirname(fileURLToPath(import.meta.url));
const webviewJs = readFileSync(join(here, '..', 'webview.js'), 'utf8');
const MARK = 'A third-party page';

function serve(host, handler) {
  const server = createServer(handler);
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, host, () => resolve({ port: server.address().port, close: () => server.close() }));
  });
}

// The other site: a plain page, no CORP, no COEP.
const site = await serve('localhost', (req, res) => {
  res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
  res.end(`<!doctype html><title>${MARK}</title><p id="mark">${MARK}</p>`);
});
const siteUrl = `http://localhost:${site.port}/`;

// The host: a page with webview.js; /isolated sends the old headers.
const host = await serve('127.0.0.1', (req, res) => {
  const headers = { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-cache' };
  if (req.url.startsWith('/isolated')) {
    headers['Cross-Origin-Opener-Policy'] = 'same-origin';
    headers['Cross-Origin-Embedder-Policy'] = 'require-corp';
  }
  res.writeHead(200, headers);
  res.end(`<!doctype html><meta charset="utf-8"><title>web view host</title>
<div id="layer" style="position:relative;width:390px;height:400px"><canvas id="c" width="390" height="400" style="width:390px;height:400px"></canvas></div>
<script>${webviewJs.replace(/<\/script/gi, '<\\/script')}</script>
<script>
  window.events = [];
  WebViews.attach(document.getElementById('layer'), document.getElementById('c'), 390, (json) => events.push(JSON.parse(json)));
  WebViews.command([1, { Create: { width: 390, height: 300, message_handlers: [], user_scripts: [] } }]);
  WebViews.command([1, { LoadRequest: { nav: 1, request: { url: ${JSON.stringify(siteUrl)} } } }]);
  WebViews.command([1, { Decide: { id: 1, allow: true } }]);
  WebViews.command([1, { Place: { x: 0, y: 0, width: 390, height: 300, visible: true, covered: false } }]);
</script>`);
});

// Whether the web view's frame shows the other site's page.
async function shows(browser, path) {
  const page = await browser.newPage();
  try {
    await page.goto(`http://127.0.0.1:${host.port}${path}`);
    // webview.js reports Finished on the frame's load event, refused or not.
    await page.waitForFunction(() => events.some(([, e]) => e.Finished), null, { timeout: 15000 });
    const frame = page.frames().find((f) => f !== page.mainFrame());
    if (!frame) return false;
    const text = await frame.evaluate(() => document.getElementById('mark')?.textContent ?? null).catch(() => null);
    return text === MARK;
  } finally {
    await page.close();
  }
}

const name = process.argv[2] || 'chromium';
const browser = await (name === 'webkit' ? webkit : chromium).launch();
let failed = false;
try {
  const plain = await shows(browser, '/');
  const isolated = await shows(browser, '/isolated');
  console.log(`${name}: no isolation headers: ${plain ? 'loads' : 'refused'}; COOP+COEP require-corp: ${isolated ? 'loads' : 'refused'}`);
  if (!plain) { console.error('FAIL: the third-party page should load in a web view without the isolation headers'); failed = true; }
  if (isolated) console.log('note: this browser loads it under require-corp too');
} finally {
  await browser.close();
  site.close();
  host.close();
}
process.exit(failed ? 1 : 0);
