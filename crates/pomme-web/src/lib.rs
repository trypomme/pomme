//! pomme in the browser. The emulator runs in a Web Worker, and `run` doesn't
//! return while the app runs. It needs JSPI (WebAssembly JavaScript Promise
//! Integration): it pauses in `pommePause` while the app is idle and after
//! each frame, giving the browser a break (its GPU work is sent, its answers
//! arrive, and the page's messages reach the worker). No shared memory, so the
//! page needs no special headers and runs embedded in other sites.
//! The page talks to it through:
//!
//! - **input**: the page posts each event to the worker, which queues it;
//!   `pommeTakeInput()` returns what's queued as an `Int32Array`, 4 ints per
//!   event: a touch is (phase, x×100, y×100, 0), and what the screen reader
//!   did to an element (see below) is ([`AX_EVENT`] + action, the element's id
//!   low 32 bits, high 32 bits, 0). The emulator takes them when the app is
//!   idle, pausing in `pommePause` until the next deadline otherwise (new
//!   input ends that pause early).
//! - **frames**: the worker calls the global `pommeFrame(rgba, width, height)`
//!   (defined by the worker script), which posts the pixels to the page.
//! - **accessibility**: the screen's accessibility tree, whenever it changes,
//!   goes to the global `pommeAccessibility(json)` (a list of
//!   `pomme_hle::accessibility::Node`s); the page mirrors it in ARIA over the
//!   screen (web/accessibility.js) for the browser's screen reader.
//! - **the display**: its shape (an SVG path in points, with the device's
//!   continuous corners) goes to the global `pommeDisplay(width, height, path)`
//!   whenever the way it's held changes; the page clips the screen to it, as
//!   the display's glass does, so screens' rounded cards in a transition fit
//!   its corners exactly.
//! - **web views**: pages run in the browser itself, in frames the page puts
//!   under the screen (web/webview.js) where pomme leaves a hole. The worker's
//!   commands go out through the global `pommeWebView(json)`; the page's events
//!   come back as messages the worker queues (see [`BrowserEngine`]).
//!
//! Virtual time follows the wall clock while the app is idle; when the guest
//! is busy it simply falls behind and catches up, as a slow phone would.

use pomme_hle::accessibility::{Action, Node};
use pomme_hle::host::{Event, Frame, Host, Idle};
use pomme_hle::touch::Input;
use pomme_hle::{Emu, Stop};
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

/// The first input "phase" that's a screen reader's action: + 0 activate,
/// 1 increment, 2 decrement, 3 focus (touch phases are 0–4).
const AX_EVENT: i32 = 16;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = pommeFrame)]
    fn pomme_frame(rgba: &[u8], width: u32, height: u32);
    #[wasm_bindgen(js_name = pommeLog)]
    fn pomme_log(s: &str);
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn now_ms() -> f64;
    #[wasm_bindgen(js_name = pommeWebView)]
    fn pomme_web_view(json: &str);
    #[wasm_bindgen(js_name = pommeAccessibility)]
    fn pomme_accessibility(json: &str);
    #[wasm_bindgen(js_namespace = performance, js_name = measure)]
    fn performance_measure(name: &str, options: &JsValue);
    #[wasm_bindgen(js_name = pommeDisplay)]
    fn pomme_display(width: f64, height: f64, path: &str);
    /// Pause up to `ms`, or until the page sends input: the browser runs
    /// meanwhile (its GPU work is sent, its answers and the page's messages
    /// arrive); `ms` 0 gives it one task. The worker swaps in a
    /// `WebAssembly.Suspending` for this import (web/worker.js), so only
    /// numbers may cross it.
    #[wasm_bindgen(js_name = pommePause)]
    fn pomme_pause(ms: f64);
    /// The page's input events queued since the last call, 4 ints each.
    #[wasm_bindgen(js_name = pommeTakeInput)]
    fn pomme_take_input() -> js_sys::Int32Array;
    /// The page's web view events queued since the last call: JSON `[page, event]` strings.
    #[wasm_bindgen(js_name = pommeTakeWebViewEvents)]
    fn pomme_take_web_view_events() -> js_sys::Array;
    /// Why the app stopped (`run` returns nothing: with JSPI it's a promise).
    #[wasm_bindgen(js_name = pommeStopped)]
    fn pomme_stopped(why: &str);
}

/// Web views' pages in the browser's own frames (web/webview.js shows them).
/// Commands are JSON `[page, command]` handed to `pommeWebView`; events come
/// back as JSON `[page, event]` messages, which the worker queues until
/// [`poll`](pomme_hle::webkit::WebEngine::poll) takes them.
struct BrowserEngine;

impl pomme_hle::webkit::WebEngine for BrowserEngine {
    fn send(&mut self, page: pomme_hle::webkit::PageId, cmd: pomme_hle::webkit::Command) {
        if let Ok(json) = serde_json::to_string(&(page, cmd)) {
            pomme_web_view(&json);
        }
    }

    fn poll(&mut self) -> Vec<(pomme_hle::webkit::PageId, pomme_hle::webkit::Event)> {
        let mut out = Vec::new();
        for json in pomme_take_web_view_events().iter() {
            let json = json.as_string().unwrap_or_default();
            match serde_json::from_str(&json) {
                Ok(ev) => out.push(ev),
                Err(e) => pomme_log(&format!("web view: bad event ({e}): {json}")),
            }
        }
        out
    }

    fn live(&self) -> bool {
        true
    }

    fn overlays(&self) -> bool {
        true
    }
}

struct WebHost {
    /// wall ms − virtual ms, fixed when the app first idles.
    base: Option<f64>,
    /// Wall time of the last progress line in the console.
    last_log: f64,
    /// Frame statistics in the console (`?stats=1`).
    stats: Option<pomme_hle::host::FrameStats>,
    /// The phone, and how its display was last held (the page has its shape).
    device: &'static pomme_hle::device::Device,
    held: Option<pomme_hle::device::Orientation>,
}

impl WebHost {
    fn take_events(&mut self) -> Vec<Event> {
        let input = pomme_take_input().to_vec();
        input
            .chunks_exact(4)
            .map(|e| {
                let action = match e[0] - AX_EVENT {
                    0 => Some(Action::Activate),
                    1 => Some(Action::Increment),
                    2 => Some(Action::Decrement),
                    3 => Some(Action::Focus),
                    _ => None,
                };
                match action {
                    Some(action) => Event::Accessibility { id: e[1] as u32 as u64 | (e[2] as u32 as u64) << 32, action },
                    None => Event::Touch(Input { at_ns: 0, phase: e[0] as u64, x: e[1] as f64 / 100.0, y: e[2] as f64 / 100.0 }),
                }
            })
            .collect()
    }
}

impl Host for WebHost {
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> Idle {
        let virt_ms = now_ns as f64 / 1e6;
        let base = *self.base.get_or_insert_with(|| now_ms() - virt_ms);
        if now_ms() - self.last_log > 5000.0 {
            self.last_log = now_ms();
            pomme_log(&format!("virtual time {:.1} s", virt_ms / 1e3));
        }
        loop {
            let events = self.take_events();
            let wall_virt_ms = now_ms() - base;
            if !events.is_empty() {
                let at = (wall_virt_ms.max(virt_ms) * 1e6) as u64;
                return Idle::Input { at_ns: at, events };
            }
            let wait_ms = match next_ns {
                Some(n) => n as f64 / 1e6 - wall_virt_ms,
                None => 1000.0,
            };
            if wait_ms <= 0.0 {
                if wait_ms < -250.0 {
                    // Far behind (the guest was busy): don't fast-forward through the backlog.
                    self.base = Some(now_ms() - virt_ms);
                }
                return Idle::Advance;
            }
            pomme_pause(wait_ms.min(1000.0));
        }
    }

    fn frame(&mut self, f: Frame<'_>) {
        if self.held != Some(f.orientation) {
            self.held = Some(f.orientation);
            let (w, h) = self.device.screen(f.orientation);
            pomme_display(w, h, &pomme_hle::render::display_svg_path(self.device, f.orientation));
        }
        pomme_frame(f.rgba, f.width, f.height);
        // A break for the browser after each frame: its GPU work goes out, the page's messages come in.
        pomme_pause(0.0);
        if let Some(stats) = self.stats.as_mut() {
            // In the browser's performance tools too (the worker's track).
            if let Some(t) = f.timing {
                measure("pomme: app", t.since_ms, t.render_start_ms);
                measure("pomme: render", t.render_start_ms, t.render_start_ms + t.render_ms);
            }
            if let Some(line) = stats.frame(&f, now_ms()) {
                pomme_log(&line);
            }
        }
    }

    fn clock_ms(&mut self) -> Option<f64> {
        self.stats.is_some().then(now_ms)
    }

    /// The page can't tell whether a screen reader is on, so it always has the tree.
    fn wants_accessibility(&mut self) -> bool {
        true
    }

    fn accessibility(&mut self, tree: &[Node]) {
        if let Ok(json) = serde_json::to_string(tree) {
            pomme_accessibility(&json);
        }
    }
}

/// A span of the worker's time in the browser's performance tools.
fn measure(name: &str, start_ms: f64, end_ms: f64) {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"start".into(), &start_ms.into());
    let _ = js_sys::Reflect::set(&o, &"end".into(), &end_ms.into());
    performance_measure(name, &o);
}

/// The network: synchronous XMLHttpRequest (fine in a worker), so CORS applies.
/// Requests through the browser's XHR: in the background while the app runs
/// (`start` and `poll`), or at once (`fetch`, for a host that needs the answer now).
#[derive(Default)]
struct XhrNet {
    /// Requests out: the XHR, with the method and URL for errors.
    out: std::collections::HashMap<pomme_hle::net::RequestId, (web_sys::XmlHttpRequest, String, String)>,
    next: pomme_hle::net::RequestId,
}

/// Headers a browser won't let scripts set.
fn forbidden(name: &str) -> bool {
    const NAMES: &[&str] = &["accept-charset", "accept-encoding", "connection", "content-length", "cookie", "date", "host", "keep-alive", "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade", "user-agent", "via"];
    let n = name.to_ascii_lowercase();
    NAMES.contains(&n.as_str()) || n.starts_with("sec-") || n.starts_with("proxy-")
}

fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

/// Open and send `req` (asynchronously or not).
fn send_xhr(req: &pomme_hle::net::Request, background: bool) -> Result<web_sys::XmlHttpRequest, String> {
    let xhr = web_sys::XmlHttpRequest::new().map_err(js_error)?;
    xhr.open_with_async(&req.method, &req.url, background).map_err(js_error)?;
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
    for (k, v) in &req.headers {
        if !forbidden(k) {
            xhr.set_request_header(k, v).map_err(js_error)?;
        }
    }
    let sent = if req.body.is_empty() { xhr.send() } else { xhr.send_with_opt_u8_array(Some(&req.body)) };
    sent.map_err(|e| format!("{} {}: {}", req.method, req.url, js_error(e)))?;
    Ok(xhr)
}

/// The answer of a finished XHR.
fn xhr_response(xhr: &web_sys::XmlHttpRequest, method: &str, url: &str) -> Result<pomme_hle::net::Response, String> {
    let status = xhr.status().map_err(js_error)?;
    if status == 0 {
        return Err(format!("{method} {url}: network error (offline, or blocked by CORS)"));
    }
    let headers = xhr
        .get_all_response_headers()
        .map_err(js_error)?
        .lines()
        .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect();
    let body = xhr.response().map(|b| js_sys::Uint8Array::new(&b).to_vec()).unwrap_or_default();
    Ok(pomme_hle::net::Response { status, headers, body })
}

impl pomme_hle::net::Net for XhrNet {
    fn fetch(&mut self, req: &pomme_hle::net::Request) -> Result<pomme_hle::net::Response, String> {
        let xhr = send_xhr(req, false)?;
        xhr_response(&xhr, &req.method, &req.url)
    }

    /// An asynchronous XHR: the browser runs it while the emulator pauses
    /// (after each frame, and while the app idles).
    fn start(&mut self, req: &pomme_hle::net::Request) -> Option<pomme_hle::net::RequestId> {
        // A request that can't even be sent fails at once, through `fetch`.
        let xhr = send_xhr(req, true).ok()?;
        self.next += 1;
        self.out.insert(self.next, (xhr, req.method.clone(), req.url.clone()));
        Some(self.next)
    }

    fn poll(&mut self, id: pomme_hle::net::RequestId) -> Option<Result<pomme_hle::net::Response, String>> {
        const DONE: u16 = 4;
        if self.out.get(&id)?.0.ready_state() != DONE {
            return None;
        }
        let (xhr, method, url) = self.out.remove(&id)?;
        Some(xhr_response(&xhr, &method, &url))
    }

    fn forget(&mut self, id: pomme_hle::net::RequestId) {
        if let Some((xhr, ..)) = self.out.remove(&id) {
            let _ = xhr.abort();
        }
    }
}

/// pomme-hle's pause hook: one browser task, so the GPU's work and answers get through.
fn pause_once() {
    pomme_pause(0.0);
}

fn log_line(line: &str) {
    pomme_log(line);
}

/// The `.app` directory inside a bundle (its first path under /app/).
fn app_path(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let rest = &s[s.find("/app/")?..];
    Some(rest[..rest.find(".app")? + 4].to_string())
}

/// Mount `bundle` (from `pomme bundle`) and run its app until it stops,
/// presenting iOS `ios` ("18.6", "26"; empty for the default); with
/// `frame_stats`, how smoothly frames arrive goes to the console each second.
/// Runs until the app stops, then tells the page why through `pommeStopped`.
/// The worker calls it through `WebAssembly.promising`, so it returns nothing
/// (the glue can't read a value out of a promise).
#[wasm_bindgen]
pub fn run(bundle: Vec<u8>, scale: f32, ios: String, frame_stats: bool) {
    pomme_hle::host::set_log_hook(Some(log_line));
    pomme_hle::host::set_pause_hook(Some(pause_once));
    match run_inner(bundle, scale, &ios, frame_stats) {
        Ok(s) | Err(s) => pomme_stopped(&s),
    }
}

fn run_inner(bundle: Vec<u8>, scale: f32, ios: &str, frame_stats: bool) -> Result<String, String> {
    let app = app_path(&bundle).ok_or("the bundle has no .app")?;
    pomme_loader::vfs::mount(&bundle)?;
    drop(bundle);
    pomme_log(&format!("loading {app}"));
    let opts = pomme_loader::LoadOptions { sysroot: Some(PathBuf::from("/sysroot")) };
    let p = pomme_loader::load_with(app.as_ref(), &opts).map_err(|e| e.to_string())?;
    let mut emu = Emu::new(p).map_err(|e| e.to_string())?;
    emu.live.scale = scale;
    if !ios.is_empty() {
        emu.system.os = ios.parse()?;
    }
    pomme_log(&format!("presenting iOS {}", emu.system.os));
    *emu.net.client = Some(Box::new(XhrNet::default()));
    *emu.webkit.engine = Some(Box::new(BrowserEngine));
    *emu.live.host = Some(Box::new(WebHost { base: None, last_log: 0.0, stats: frame_stats.then(Default::default), device: emu.device, held: None }));
    pomme_log("running");
    let r = emu.run_main();
    let steps = emu.cpu.steps;
    Ok(match r {
        Ok(code) => format!("exit({code}) after {steps} instructions"),
        Err(stop) => {
            let at = match &stop {
                Stop::MissingFunction { caller, .. } | Stop::MissingMethod { caller, .. } => format!(" (called from {})", emu.symbolize(caller.wrapping_sub(4))),
                Stop::Cpu { pc, .. } => format!(" (at {})", emu.symbolize(*pc)),
                _ => String::new(),
            };
            format!("stopped after {steps} instructions: {stop}{at}")
        }
    })
}
