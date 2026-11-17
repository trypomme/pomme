// A plain page on another origin that embeds the pomme page in an iframe, as
// a docs site would: served on localhost (the pomme page is on 127.0.0.1, so
// the two are different origins) with no special headers, so it isn't
// cross-origin isolated. run.mjs --embed PORT serves it and drives the frame.
import { createServer } from 'node:http';

export function serveOuter(port, frameUrl) {
  const html = `<!doctype html>
<meta charset="utf-8">
<title>A page that embeds pomme</title>
<body style="margin:0;font:14px system-ui">
<p style="margin:8px">Another site's page, with pomme in a frame:</p>
<iframe id="pomme" src="${frameUrl.replace(/"/g, '&quot;')}" style="width:480px;height:1000px;border:1px solid #ccc"></iframe>
</body>`;
  const server = createServer((req, res) => {
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-cache' });
    res.end(html);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, 'localhost', () => resolve({ url: `http://localhost:${port}/`, close: () => server.close() }));
  });
}
