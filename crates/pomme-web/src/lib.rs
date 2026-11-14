//! pomme in the browser. The emulator runs in a Web Worker: it blocks the
//! way it does natively, and the page talks to it through:
//!
//! - **input**: a SharedArrayBuffer viewed as an `Int32Array`. Slot 0 counts
//!   events written; events are 4 ints each in a ring of [`RING`] starting at
//!   slot [`HEADER`]: a touch is (phase, x×100, y×100, 0), and what the screen
//!   reader did to an element (see below) is ([`AX_EVENT`] + action, the
//!   element's id low 32 bits, high 32 bits, 0). The page writes an event,
//!   bumps the count and `Atomics.notify`s; the worker reads them when the app
//!   is idle, sleeping in `Atomics.wait` until the next deadline otherwise.
//! - **frames**: the worker calls the global `pommeFrame(rgba, width, height)`
//!   (defined by the worker script), which posts the pixels to the page.
//! - **accessibility**: the screen's accessibility tree, whenever it changes,
//!   goes to the global `pommeAccessibility(json)` (a list of
//!   `pomme_hle::accessibility::Node`s); the page mirrors it in ARIA over the
//!   screen (web/accessibility.js) for the browser's screen reader.
//! - **web views**: pages run in the browser itself, in frames the page puts
//!   under the screen (web/webview.js) where pomme leaves a hole. The worker's
//!   commands go out through the global `pommeWebView(json)`; the page's events
//!   come back through a second SharedArrayBuffer, a byte ring of messages (see
//!   [`BrowserEngine`]).
//!
//! Virtual time follows the wall clock while the app is idle; when the guest
//! is busy it simply falls behind and catches up, as a slow phone would.

use pomme_hle::accessibility::{Action, Node};
use pomme_hle::host::{Event, Frame, Host, Idle};
use pomme_hle::touch::Input;
use pomme_hle::{Emu, Stop};
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

const HEADER: u32 = 8;
const RING: u32 = 256;
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
}

/// Bytes of the web view event ring (after its two-int header: bytes written, bytes read).
pub const WEB_RING: u32 = 1 << 20;

/// Web views' pages in the browser's own frames (web/webview.js shows them).
/// Commands are JSON `[page, command]` handed to `pommeWebView`; events come
/// back as JSON `[page, event]` messages in the ring, each a little-endian
/// u32 length and the bytes, the page writing and the worker reading.
struct BrowserEngine {
    header: js_sys::Int32Array,
    ring: js_sys::Uint8Array,
}

impl BrowserEngine {
    fn new(sab: &js_sys::SharedArrayBuffer) -> Self {
        BrowserEngine { header: js_sys::Int32Array::new_with_byte_offset_and_length(sab, 0, 2), ring: js_sys::Uint8Array::new_with_byte_offset_and_length(sab, 8, WEB_RING) }
    }

    fn byte(&self, at: u32) -> u8 {
        self.ring.get_index(at % WEB_RING)
    }
}

impl pomme_hle::webkit::WebEngine for BrowserEngine {
    fn send(&mut self, page: pomme_hle::webkit::PageId, cmd: pomme_hle::webkit::Command) {
        if let Ok(json) = serde_json::to_string(&(page, cmd)) {
            pomme_web_view(&json);
        }
    }

    fn poll(&mut self) -> Vec<(pomme_hle::webkit::PageId, pomme_hle::webkit::Event)> {
        let written = js_sys::Atomics::load(&self.header, 0).unwrap_or(0) as u32;
        let mut read = js_sys::Atomics::load(&self.header, 1).unwrap_or(0) as u32;
        let mut out = Vec::new();
        while written.wrapping_sub(read) >= 4 {
            let len = u32::from_le_bytes([self.byte(read), self.byte(read + 1), self.byte(read + 2), self.byte(read + 3)]);
            let bytes: Vec<u8> = (0..len).map(|i| self.byte(read + 4 + i)).collect();
            read = read.wrapping_add(4 + len);
            match serde_json::from_slice(&bytes) {
                Ok(ev) => out.push(ev),
                Err(e) => pomme_log(&format!("web view: bad event ({e}): {}", String::from_utf8_lossy(&bytes))),
            }
        }
        let _ = js_sys::Atomics::store(&self.header, 1, read as i32);
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
    input: js_sys::Int32Array,
    read: i32,
    /// wall ms − virtual ms, fixed when the app first idles.
    base: Option<f64>,
    /// Wall time of the last progress line in the console.
    last_log: f64,
}

impl WebHost {
    fn take_events(&mut self) -> Vec<Event> {
        let written = js_sys::Atomics::load(&self.input, 0).unwrap_or(0);
        let mut out = Vec::new();
        while self.read < written {
            let at = HEADER + (self.read as u32 % RING) * 4;
            let get = |i: u32| self.input.get_index(at + i);
            let kind = get(0);
            let action = match kind - AX_EVENT {
                0 => Some(Action::Activate),
                1 => Some(Action::Increment),
                2 => Some(Action::Decrement),
                3 => Some(Action::Focus),
                _ => None,
            };
            out.push(match action {
                Some(action) => Event::Accessibility { id: get(1) as u32 as u64 | (get(2) as u32 as u64) << 32, action },
                None => Event::Touch(Input { at_ns: 0, phase: kind as u64, x: get(1) as f64 / 100.0, y: get(2) as f64 / 100.0 }),
            });
            self.read += 1;
        }
        out
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
            let seen = js_sys::Atomics::load(&self.input, 0).unwrap_or(0);
            let _ = js_sys::Atomics::wait_with_timeout(&self.input, 0, seen, wait_ms.min(1000.0));
        }
    }

    fn frame(&mut self, f: Frame<'_>) {
        pomme_frame(f.rgba, f.width, f.height);
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

/// The network: synchronous XMLHttpRequest (fine in a worker), so CORS applies.
struct XhrNet;

/// Headers a browser won't let scripts set.
fn forbidden(name: &str) -> bool {
    const NAMES: &[&str] = &["accept-charset", "accept-encoding", "connection", "content-length", "cookie", "date", "host", "keep-alive", "origin", "referer", "te", "trailer", "transfer-encoding", "upgrade", "user-agent", "via"];
    let n = name.to_ascii_lowercase();
    NAMES.contains(&n.as_str()) || n.starts_with("sec-") || n.starts_with("proxy-")
}

impl pomme_hle::net::Net for XhrNet {
    fn fetch(&mut self, req: &pomme_hle::net::Request) -> Result<pomme_hle::net::Response, String> {
        let err = |e: JsValue| e.as_string().unwrap_or_else(|| format!("{e:?}"));
        let xhr = web_sys::XmlHttpRequest::new().map_err(err)?;
        xhr.open_with_async(&req.method, &req.url, false).map_err(err)?;
        xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Arraybuffer);
        for (k, v) in &req.headers {
            if !forbidden(k) {
                xhr.set_request_header(k, v).map_err(err)?;
            }
        }
        let sent = if req.body.is_empty() { xhr.send() } else { xhr.send_with_opt_u8_array(Some(&req.body)) };
        sent.map_err(|e| format!("{} {}: {}", req.method, req.url, err(e)))?;
        let status = xhr.status().map_err(err)?;
        if status == 0 {
            return Err(format!("{} {}: network error (offline, or blocked by CORS)", req.method, req.url));
        }
        let headers = xhr
            .get_all_response_headers()
            .map_err(err)?
            .lines()
            .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
            .collect();
        let body = xhr.response().map(|b| js_sys::Uint8Array::new(&b).to_vec()).unwrap_or_default();
        Ok(pomme_hle::net::Response { status, headers, body })
    }
}

/// The `.app` directory inside a bundle (its first path under /app/).
fn app_path(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();
    let rest = &s[s.find("/app/")?..];
    Some(rest[..rest.find(".app")? + 4].to_string())
}

/// Mount `bundle` (from `pomme bundle`) and run its app until it stops,
/// presenting iOS `ios` ("18.6", "26"; empty for the default).
/// Never returns while the app runs; returns a description of why it stopped.
#[wasm_bindgen]
pub fn run(bundle: Vec<u8>, input: js_sys::Int32Array, scale: f32, ios: String, web_views: js_sys::SharedArrayBuffer) -> String {
    match run_inner(bundle, input, scale, &ios, &web_views) {
        Ok(s) | Err(s) => s,
    }
}

fn run_inner(bundle: Vec<u8>, input: js_sys::Int32Array, scale: f32, ios: &str, web_views: &js_sys::SharedArrayBuffer) -> Result<String, String> {
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
    *emu.net.client = Some(Box::new(XhrNet));
    *emu.webkit.engine = Some(Box::new(BrowserEngine::new(web_views)));
    *emu.live.host = Some(Box::new(WebHost { input, read: 0, base: None, last_log: 0.0 }));
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
