// Web views in the browser: each WKWebView the app makes is an <iframe> under
// the screen's canvas, where pomme leaves a hole in its frames (so views the
// app draws above the page still cover it). The worker sends commands
// ({type: 'webview', json}); events go back to it as JSON messages
// (see crates/pomme-web/src/lib.rs, BrowserEngine).
//
// A page the app builds (loadHTMLString:baseURL:) is the frame's srcdoc, with a
// prelude first: it puts every navigation to the app's navigation delegate (the
// Navigation API's navigate event, cancelled until the delegate allows it),
// defines window.webkit.messageHandlers and passes the console on. Being the
// page's own document, the frame is scriptable (evaluateJavaScript). Another
// site's page (loadRequest:) is cross-origin: it shows and takes input, but
// its navigations aren't put to the delegate and scripts can't run in it.
'use strict';

const WebViews = (() => {
  const pages = new Map(); // page id → state
  let layer, canvas, send, screenWidth = 390;
  let next = 0; // ids for navigations pages start (the worker's are below 2^32)
  const ENGINE_IDS = 2 ** 32;

  function emit(page, event) {
    send(JSON.stringify([page, event]));
  }

  const policy = (page, id, url, kind) => emit(page, { Policy: { id, url, method: 'GET', main_frame: true, kind } });

  // The document's prelude: navigation policy, message handlers, console.
  const script = (s) => `<script>${s.source.replace(/<\/script/gi, '<\\/script')}<\/script>`;
  function prelude(id, scripts) {
    const user = scripts.map(script).join('');
    return `<script>(function () {
  var host = top.__pommeWebViews, page = ${JSON.stringify(id)};
  if (window.navigation) navigation.addEventListener('navigate', function (e) {
    if (e.cancelable && !host.navigate(page, e.destination.url, e.navigationType)) e.preventDefault();
  });
  var handlers = new Proxy({}, { get: function (_, name) {
    return host.handles(page, String(name)) ? { postMessage: function (body) { host.message(page, String(name), body === undefined ? null : JSON.parse(JSON.stringify(body))); } } : undefined;
  } });
  Object.defineProperty(window, 'webkit', { value: { messageHandlers: handlers }, configurable: true });
  ['log', 'info', 'warn', 'error', 'debug'].forEach(function (level) {
    var original = console[level];
    console[level] = function () { host.console(page, level + ': ' + Array.prototype.map.call(arguments, String).join(' ')); return original.apply(console, arguments); };
  });
  window.addEventListener('error', function (e) { host.console(page, 'error: ' + e.message + ' at ' + e.filename + ':' + e.lineno); });
})();<\/script>${user}`;
  }

  // What the frames call (they're same-origin with this page).
  window.__pommeWebViews = {
    navigate(page, url, type) {
      const p = pages.get(page);
      if (!p) return true;
      if (p.allowOnce.has(url)) { p.allowOnce.delete(url); return true; }
      const id = ENGINE_IDS + ++next;
      p.pending.set(id, { url });
      policy(page, id, url, type === 'reload' ? 'Reload' : type === 'traverse' ? 'BackForward' : 'Other');
      return false;
    },
    handles: (page, name) => !!pages.get(page)?.handlers.has(name),
    message: (page, name, body) => emit(page, { Message: { name, body, main_frame: true } }),
    console: (page, text) => emit(page, { Console: text }),
  };

  function create(page, cfg) {
    const iframe = document.createElement('iframe');
    iframe.allow = 'autoplay; fullscreen; encrypted-media; picture-in-picture';
    iframe.style.cssText = `position:absolute;left:0;top:0;border:0;transform-origin:0 0;display:none;width:${cfg.width}px;height:${cfg.height}px;background:#fff`;
    layer.insertBefore(iframe, canvas);
    const p = { iframe, handlers: new Set(cfg.message_handlers), scripts: cfg.user_scripts, pending: new Map(), allowOnce: new Set(), nav: null, url: '', title: null, rect: null, visible: false, covered: false };
    iframe.addEventListener('load', () => loaded(page, p));
    pages.set(page, p);
  }

  function loaded(page, p) {
    if (p.nav === null) return;
    const nav = p.nav;
    p.nav = null;
    emit(page, { Committed: { nav, url: p.url } });
    emit(page, { Finished: { nav } });
    title(page, p);
  }

  function title(page, p) {
    let t = null;
    try { t = p.iframe.contentDocument?.title ?? null; } catch (e) { /* another site's page */ }
    if (t !== p.title) { p.title = t; emit(page, { Title: t }); }
  }

  // An allowed load the app started: an HTML string becomes the frame's document; a URL its page.
  function start(page, p, nav, load) {
    p.nav = nav;
    p.url = load.url;
    emit(page, { Started: { nav, url: load.url } });
    if (load.html !== undefined) {
      const atEnd = p.scripts.filter((s) => s.at_end), atStart = p.scripts.filter((s) => !s.at_end);
      const base = load.url === 'about:blank' ? '' : `<base href="${load.url.replace(/"/g, '&quot;')}">`;
      p.iframe.srcdoc = base + prelude(page, atStart) + load.html + atEnd.map(script).join('');
    } else {
      p.iframe.removeAttribute('srcdoc');
      p.iframe.src = load.url;
    }
  }

  function evaluate(page, p, id, script) {
    let result;
    try {
      const w = p.iframe.contentWindow;
      const v = w.eval(script);
      if (v === undefined) result = 'Undefined';
      else if (typeof v === 'function' || typeof v === 'symbol' || (v && typeof v.then === 'function')) result = { Exception: 'JavaScript execution returned a result of an unsupported type' };
      else result = { Value: JSON.parse(JSON.stringify(v)) };
    } catch (e) {
      result = { Exception: String((e && e.message) || e) };
    }
    emit(page, { Evaluated: { id, result } });
  }

  function place(p) {
    const k = canvas.getBoundingClientRect().width / screenWidth; // CSS pixels per point
    const [x, y, w, h] = p.rect;
    // Input over a page nothing covers is the page's (its buttons, YouTube's controls);
    // elsewhere it falls through to the screen, and the app.
    Object.assign(p.iframe.style, { display: p.visible ? 'block' : 'none', pointerEvents: p.covered ? 'none' : 'auto', width: `${w}px`, height: `${h}px`, transform: `translate(${x * k}px, ${y * k}px) scale(${k})` });
  }

  function command([page, cmd]) {
    const [name, a] = typeof cmd === 'string' ? [cmd, null] : Object.entries(cmd)[0];
    if (name === 'Create') return create(page, a);
    const p = pages.get(page);
    if (!p) return;
    switch (name) {
      case 'LoadHtml': {
        const url = a.base ? new URL(a.base).href : 'about:blank';
        p.pending.set(a.nav, { url, html: a.html, app: true });
        return policy(page, a.nav, url, 'Other');
      }
      case 'LoadRequest':
        p.pending.set(a.nav, { url: a.request.url, app: true });
        return policy(page, a.nav, a.request.url, 'Other');
      case 'Decide': {
        const load = p.pending.get(a.id);
        p.pending.delete(a.id);
        if (!load || !a.allow) return;
        if (load.app) return start(page, p, a.id, load);
        // The page's own navigation, allowed: let it through this time.
        if (!/^(https?|about|data|blob):/.test(load.url)) {
          return emit(page, { Failed: { nav: a.id, provisional: true, error: { domain: 'WebKitErrorDomain', code: 101, description: 'The URL can’t be shown' } } });
        }
        p.allowOnce.add(load.url);
        p.nav = a.id;
        p.url = load.url;
        emit(page, { Started: { nav: a.id, url: load.url } });
        try { p.iframe.contentWindow.location.href = load.url; } catch (e) { p.iframe.src = load.url; }
        return;
      }
      case 'Evaluate': return evaluate(page, p, a.id, a.script);
      case 'AddUserScript': p.scripts.push(a); return;
      case 'RemoveAllUserScripts': p.scripts = []; return;
      case 'AddMessageHandler': p.handlers.add(a); return;
      case 'RemoveMessageHandler': p.handlers.delete(a); return;
      case 'Resize': p.iframe.style.width = `${a.width}px`; p.iframe.style.height = `${a.height}px`; return;
      case 'Place': p.rect = [a.x, a.y, a.width, a.height]; p.visible = a.visible; p.covered = a.covered; place(p); return;
      case 'Reload': p.nav = a.nav; try { p.iframe.contentWindow.location.reload(); } catch (e) { p.iframe.src = p.iframe.src; } return;
      case 'GoBack': try { p.iframe.contentWindow.history.back(); } catch (e) { /* another site's page */ } return;
      case 'GoForward': try { p.iframe.contentWindow.history.forward(); } catch (e) { /* another site's page */ } return;
      case 'StopLoading': try { p.iframe.contentWindow.stop(); } catch (e) { /* another site's page */ } return;
      case 'Close': p.iframe.remove(); pages.delete(page); return;
      default: return; // UserAgent (the browser's own), Touch (input reaches the frame itself)
    }
  }

  return {
    attach(theLayer, theCanvas, width, sendToWorker) {
      layer = theLayer; canvas = theCanvas; screenWidth = width; send = sendToWorker;
      new ResizeObserver(() => pages.forEach((p) => p.rect && place(p))).observe(canvas);
      setInterval(() => pages.forEach((p, id) => title(id, p)), 500);
    },
    command,
  };
})();
