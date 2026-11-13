//! WebKit: WKWebView and the objects around it (configuration, user content
//! controller and script messages, navigation actions, the navigation and
//! UI delegates), as iOS has them.
//!
//! The guest can't run a browser engine, so pomme hosts one: the front end
//! hands the emulator a [`WebEngine`], as it hands it a [`crate::net::Net`].
//! On iOS a WKWebView's pages run out of process too (WebKit's WebContent
//! process); the engine plays that part. The emulator and the engine talk in
//! messages: the emulator sends [`Command`]s (load this HTML, evaluate this
//! script, the delegate allowed that navigation), the engine answers with
//! [`Event`]s (the page wants to navigate, a load finished, a script's
//! result, a script message), which are delivered to the app on the main run
//! loop, a run-loop turn later, as WebKit's callbacks are.
//!
//! Every navigation the engine makes, the app's own loads included, is first
//! put to the navigation delegate (`decidePolicyForNavigationAction:`) and
//! goes ahead only if it allows it. That's how pages talk to apps by
//! navigating to made-up URLs (`ytplayer://onReady`) that the delegate
//! catches and cancels. Pages can also post script messages
//! (`window.webkit.messageHandlers.NAME.postMessage(body)`) to handlers the
//! app registered with its WKUserContentController; values cross between
//! JavaScript and Objective-C as WebKit converts them (numbers to NSNumber,
//! arrays to NSArray, objects to NSDictionary, null to NSNull, undefined to nil).
//!
//! The engine may also ask the emulator for a URL's contents ([`Event::Fetch`]):
//! file URLs in the app's sandbox, which only the emulator can read, and, for
//! an engine that routes its network through pomme, everything else (through
//! [`crate::net::Net`], so tests serve pages from fixtures).
//!
//! With no engine, pages don't load: every load fails with WKErrorUnknown,
//! as does every script.
//!
//! [`Scripted`] is an engine for tests: no browser, a page that does what
//! the test says (navigate here once loaded, answer that script with this).

use crate::foundation;
use crate::net::{Request, Response};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// A web view's page, as the engine knows it (the WKWebView's address).
pub type PageId = u64;

/// What the page asks for: the navigation's ids. The emulator numbers the
/// loads the app starts below 2^32; the engine numbers its own (links,
/// scripts setting `location`, redirects) from [`ENGINE_IDS`] up.
pub const ENGINE_IDS: u64 = 1 << 32;

/// WKNavigationType.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationType {
    LinkActivated = 0,
    FormSubmitted = 1,
    BackForward = 2,
    Reload = 3,
    FormResubmitted = 4,
    Other = -1,
}

/// A failed load, as an NSError.
#[derive(Clone, Debug, PartialEq)]
pub struct WebError {
    /// "NSURLErrorDomain", "WebKitErrorDomain", "WKErrorDomain".
    pub domain: String,
    pub code: i64,
    pub description: String,
}

impl WebError {
    pub fn new(domain: &str, code: i64, description: &str) -> Self {
        WebError { domain: domain.into(), code, description: description.into() }
    }
}

/// A script WKUserScript adds to every page.
#[derive(Clone, Debug, PartialEq)]
pub struct UserScript {
    pub source: String,
    /// WKUserScriptInjectionTimeAtDocumentEnd (true) or AtDocumentStart.
    pub at_end: bool,
    pub main_frame_only: bool,
}

/// What a new page is made with (its WKWebViewConfiguration).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageConfig {
    pub user_scripts: Vec<UserScript>,
    /// Names of the script message handlers (`window.webkit.messageHandlers.NAME`).
    pub message_handlers: Vec<String>,
    /// Size in points, and pixels per point.
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    /// The whole User-Agent the app set (`customUserAgent`), or what it appends (`applicationNameForUserAgent`).
    pub user_agent: Option<String>,
    pub application_name: Option<String>,
    pub inline_media: bool,
    /// Media may play without a tap (`mediaTypesRequiringUserActionForPlayback` is none).
    pub autoplay: bool,
}

/// The emulator to the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// A new page (a WKWebView was made).
    Create(PageConfig),
    /// `loadHTMLString:baseURL:` (and `loadData:` of HTML): `base` is the document's URL.
    LoadHtml { nav: u64, html: String, base: Option<String> },
    /// `loadRequest:`, `loadFileURL:allowingReadAccessToURL:`.
    LoadRequest { nav: u64, request: Request },
    Reload { nav: u64 },
    GoBack { nav: u64 },
    GoForward { nav: u64 },
    StopLoading,
    /// The navigation delegate's decision on [`Event::Policy`] `id`.
    Decide { id: u64, allow: bool },
    /// Evaluate a script in the main frame; the result comes as [`Event::Evaluated`].
    Evaluate { id: u64, script: String },
    AddUserScript(UserScript),
    RemoveAllUserScripts,
    AddMessageHandler(String),
    RemoveMessageHandler(String),
    UserAgent(Option<String>),
    /// The web view's size changed (points, pixels per point).
    Resize { width: f64, height: f64, scale: f64 },
    /// What the emulator found for [`Event::Fetch`] `id`.
    Fetched { id: u64, result: std::result::Result<Response, String> },
    /// The web view went away.
    Close,
}

/// The engine to the emulator.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A navigation is about to happen: answer with [`Command::Decide`].
    /// For the app's own loads `id` is the load's `nav`.
    Policy { id: u64, url: String, method: String, main_frame: bool, kind: NavigationType },
    /// The allowed navigation `nav` started (provisionally), committed, finished or failed.
    Started { nav: u64, url: String },
    Committed { nav: u64, url: String },
    Finished { nav: u64 },
    Failed { nav: u64, provisional: bool, error: WebError },
    /// A script's result (None: undefined), or the exception's message.
    Evaluated { id: u64, result: std::result::Result<Option<Value>, String> },
    /// `window.webkit.messageHandlers.NAME.postMessage(body)`.
    Message { name: String, body: Value, main_frame: bool },
    Title(Option<String>),
    Progress(f64),
    /// The page wants a new window (`window.open`, `target=_blank`): the UI delegate decides.
    OpenWindow { url: String },
    /// The engine needs a URL's contents from the emulator: answer with [`Command::Fetched`].
    Fetch { id: u64, request: Request },
}

/// A web engine the front end provides.
pub trait WebEngine {
    /// A command for `page`. Events it causes may be ready at once (see [`poll`](Self::poll)).
    fn send(&mut self, page: PageId, cmd: Command);
    /// The events ready now, in order.
    fn poll(&mut self) -> Vec<(PageId, Event)>;
}

/// WKNavigationActionPolicy / WKNavigationResponsePolicy.
const POLICY_CANCEL: u64 = 0;

/// WKErrorDomain codes.
const WK_ERROR_UNKNOWN: i64 = 1;
const WK_ERROR_JAVASCRIPT_EXCEPTION: i64 = 4;

#[derive(Clone, Debug, Default)]
struct Page {
    config: u64,
    navigation_delegate: u64,
    ui_delegate: u64,
    scroll_view: u64,
    /// The committed URL (an NSURL), title, loading and progress, as the engine last said.
    url: u64,
    title: Option<String>,
    loading: bool,
    progress: f64,
    custom_user_agent: Option<String>,
    size: (f64, f64),
}

#[derive(Default)]
pub struct State {
    /// The front end's engine (None: pages don't load).
    pub engine: Option<Box<dyn WebEngine>>,
    pages: HashMap<u64, Page>,
    next_id: u64,
    /// Navigation id → WKNavigation object.
    navigations: HashMap<u64, u64>,
    /// Script id → (web view, completion block).
    scripts: HashMap<u64, (u64, u64)>,
    /// Events waiting for the main run loop, and whether it's been asked to deliver them.
    queue: VecDeque<(PageId, Event)>,
    delivering: bool,
    said_no_engine: bool,
    /// User content controller → its script message handlers (name, handler), and its user scripts.
    handlers: HashMap<u64, Vec<(String, u64)>>,
    user_scripts: HashMap<u64, Vec<u64>>,
    /// `+[WKWebsiteDataStore defaultDataStore]`.
    default_store: u64,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn string(emu: &mut Emu, s: &str) -> Result<u64> {
    foundation::new_nsstring(emu, s)
}

fn opt_string(emu: &mut Emu, s: Option<&str>) -> Result<u64> {
    match s {
        Some(s) => string(emu, s),
        None => Ok(0),
    }
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn retain(emu: &mut Emu, o: u64) -> Result<u64> {
    if o != 0 {
        send(emu, o, "retain", &[])?;
    }
    Ok(o)
}

fn nsurl(emu: &mut Emu, s: &str) -> Result<u64> {
    let s = string(emu, s)?;
    let cls = host_class(emu, "NSURL")?;
    send(emu, cls, "URLWithString:", &[s])
}

fn url_string(emu: &mut Emu, url: u64) -> Result<Option<String>> {
    if url == 0 {
        return Ok(None);
    }
    let s = send(emu, url, "absoluteString", &[])?;
    Ok(Some(foundation::nsstring(emu, s)?))
}

fn new_id(emu: &mut Emu) -> u64 {
    emu.webkit.next_id += 1;
    emu.webkit.next_id
}

/// A block whose invoke is the host function `f`, carrying `data` (read back with [`block_data`]).
fn data_block(emu: &mut Emu, f: HostFn, data: u64) -> Result<u64> {
    let invoke = emu.host_thunk(f)?;
    // Block_layout {isa, flags, reserved, invoke, descriptor}, descriptor {reserved, size}, then the data.
    let b = emu.alloc(56)?;
    let isa = host_class(emu, "__NSGlobalBlock__")?;
    emu.write_u64(b, isa)?;
    emu.p.memory.write_u32(b + 8, 1 << 28)?; // BLOCK_IS_GLOBAL
    emu.write_u64(b + 16, invoke)?;
    emu.write_u64(b + 24, b + 32)?;
    emu.write_u64(b + 40, 56)?;
    emu.write_u64(b + 48, data)?;
    Ok(b)
}

fn block_data(emu: &Emu, b: u64) -> Result<u64> {
    emu.read_u64(b + 48)
}

fn error(emu: &mut Emu, e: &WebError, url: Option<&str>) -> Result<u64> {
    let mut pairs = vec![(string(emu, "NSLocalizedDescription")?, string(emu, &e.description)?)];
    if let Some(u) = url {
        let (k, v) = (string(emu, "NSErrorFailingURLStringKey")?, string(emu, u)?);
        pairs.push((k, v));
        let (k, v) = (string(emu, "NSErrorFailingURLKey")?, nsurl(emu, u)?);
        if v != 0 {
            pairs.push((k, v));
        }
    }
    let info = foundation::new_dict_of(emu, &pairs)?;
    let domain = string(emu, &e.domain)?;
    let cls = host_class(emu, "NSError")?;
    send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, e.code as u64, info])
}

fn page(emu: &Emu, v: u64) -> Page {
    emu.webkit.pages.get(&v).cloned().unwrap_or_default()
}

fn page_mut(emu: &mut Emu, v: u64) -> &mut Page {
    emu.webkit.pages.entry(v).or_default()
}

// ---- talking to the engine ----

/// Send a command; without an engine, answer as a web view whose content can't load would.
fn command(emu: &mut Emu, v: PageId, cmd: Command) -> Result<()> {
    if emu.trace {
        println!("[webkit] {v:#x} ← {}", describe_command(&cmd));
    }
    match emu.webkit.engine.as_mut() {
        Some(engine) => engine.send(v, cmd),
        None => {
            if !emu.webkit.said_no_engine {
                emu.webkit.said_no_engine = true;
                eprintln!("[webkit] no web engine: web views won't load pages");
            }
            let none = || WebError::new("WKErrorDomain", WK_ERROR_UNKNOWN, "pomme has no web engine to load this page");
            let ev = match cmd {
                Command::LoadHtml { nav, .. } | Command::LoadRequest { nav, .. } | Command::Reload { nav } => Some(Event::Failed { nav, provisional: true, error: none() }),
                Command::Evaluate { id, .. } => Some(Event::Evaluated { id, result: Err(none().description) }),
                _ => None,
            };
            if let Some(ev) = ev {
                emu.webkit.queue.push_back((v, ev));
            }
        }
    }
    pump(emu)
}

fn describe_command(cmd: &Command) -> String {
    match cmd {
        Command::LoadHtml { nav, html, base } => format!("load HTML ({} bytes) at {base:?} (navigation {nav})", html.len()),
        Command::Evaluate { id, script } => format!("evaluate {id}: {}", script.chars().take(120).collect::<String>()),
        Command::Fetched { id, result } => format!("fetched {id}: {}", match result {
            Ok(r) => format!("{} ({} bytes)", r.status, r.body.len()),
            Err(e) => e.clone(),
        }),
        other => format!("{other:?}"),
    }
}

/// Take what the engine has ready and have the main run loop deliver it.
fn pump(emu: &mut Emu) -> Result<()> {
    if let Some(engine) = emu.webkit.engine.as_mut() {
        let events = engine.poll();
        emu.webkit.queue.extend(events);
    }
    if !emu.webkit.queue.is_empty() && !emu.webkit.delivering {
        emu.webkit.delivering = true;
        crate::uikit::after_on_main(emu, 0.0, deliver, 0)?;
    }
    Ok(())
}

/// Run-loop callout: deliver the waiting events, one per turn (each callback
/// may start more, which queue behind it), as WebKit's IPC messages arrive.
fn deliver(emu: &mut Emu) -> Result<()> {
    emu.webkit.delivering = false;
    if let Some((v, ev)) = emu.webkit.queue.pop_front() {
        if emu.trace {
            println!("[webkit] {v:#x} → {ev:?}");
        }
        handle(emu, v, ev)?;
    }
    pump(emu)
}

fn handle(emu: &mut Emu, v: u64, ev: Event) -> Result<()> {
    // Fetches are the emulator's to answer, whether or not the view is still there.
    if let Event::Fetch { id, request } = ev {
        let result = fetch(emu, &request);
        return command(emu, v, Command::Fetched { id, result });
    }
    if !emu.webkit.pages.contains_key(&v) {
        return Ok(());
    }
    let pg = page(emu, v);
    let delegate = pg.navigation_delegate;
    match ev {
        Event::Policy { id, url, method, main_frame, kind } => decide_policy(emu, v, id, &url, &method, main_frame, kind),
        Event::Started { nav, url } => {
            let n = navigation(emu, nav)?;
            let u = nsurl(emu, &url)?;
            if page(emu, v).url == 0 {
                page_mut(emu, v).url = retain(emu, u)?;
            }
            let p = page_mut(emu, v);
            p.loading = true;
            p.progress = 0.1;
            delegate_call(emu, delegate, "webView:didStartProvisionalNavigation:", &[v, n])
        }
        Event::Committed { nav, url } => {
            let n = navigation(emu, nav)?;
            let u = nsurl(emu, &url)?;
            let u = retain(emu, u)?;
            let p = page_mut(emu, v);
            p.url = u;
            p.progress = p.progress.max(0.5);
            delegate_call(emu, delegate, "webView:didCommitNavigation:", &[v, n])
        }
        Event::Finished { nav } => {
            let n = navigation(emu, nav)?;
            let p = page_mut(emu, v);
            p.loading = false;
            p.progress = 1.0;
            emu.webkit.navigations.remove(&nav);
            delegate_call(emu, delegate, "webView:didFinishNavigation:", &[v, n])
        }
        Event::Failed { nav, provisional, error: e } => {
            let n = navigation(emu, nav)?;
            let url = url_string(emu, pg.url)?;
            let err = error(emu, &e, url.as_deref())?;
            page_mut(emu, v).loading = false;
            emu.webkit.navigations.remove(&nav);
            let sel = if provisional { "webView:didFailProvisionalNavigation:withError:" } else { "webView:didFailNavigation:withError:" };
            delegate_call(emu, delegate, sel, &[v, n, err])
        }
        Event::Evaluated { id, result } => {
            let Some((_, block)) = emu.webkit.scripts.remove(&id) else { return Ok(()) };
            if block == 0 {
                return Ok(());
            }
            let (obj, err) = match result {
                Ok(None) => (0, 0),
                Ok(Some(val)) => (crate::json::build(emu, &val)?, 0),
                Err(message) => {
                    let e = WebError::new("WKErrorDomain", WK_ERROR_JAVASCRIPT_EXCEPTION, &format!("A JavaScript exception occurred: {message}"));
                    (0, error(emu, &e, None)?)
                }
            };
            crate::libsystem::call_block(emu, block, &[obj, err])?;
            Ok(())
        }
        Event::Message { name, body, main_frame } => script_message(emu, v, &name, &body, main_frame),
        Event::Title(t) => {
            page_mut(emu, v).title = t;
            Ok(())
        }
        Event::Progress(p) => {
            page_mut(emu, v).progress = p;
            Ok(())
        }
        Event::OpenWindow { url } => open_window(emu, v, &url),
        Event::Fetch { .. } => Ok(()),
    }
}

fn delegate_call(emu: &mut Emu, delegate: u64, sel: &str, args: &[u64]) -> Result<()> {
    let s = objc::intern(emu, sel)?;
    if delegate != 0 && objc::responds(emu, delegate, s)? {
        send(emu, delegate, sel, args)?;
    }
    Ok(())
}

/// The WKNavigation object for navigation `id` (made for the engine's own navigations as they start).
fn navigation(emu: &mut Emu, id: u64) -> Result<u64> {
    if let Some(&n) = emu.webkit.navigations.get(&id) {
        return Ok(n);
    }
    let n = new_object(emu, "WKNavigation")?;
    emu.webkit.navigations.insert(id, n);
    Ok(n)
}

/// A navigation the app starts: its WKNavigation (returned to the app) and id.
fn start_navigation(emu: &mut Emu) -> Result<(u64, u64)> {
    let id = new_id(emu);
    let n = navigation(emu, id)?;
    Ok((id, n))
}

/// A URL the engine asked the emulator for: the app's files, or the network.
fn fetch(emu: &mut Emu, req: &Request) -> std::result::Result<Response, String> {
    if let Some(path) = req.url.strip_prefix("file://") {
        let path = percent_decode(path.split(['?', '#']).next().unwrap_or(""));
        let Some(body) = crate::fs::read_file(emu, &path) else { return Err("no such file".into()) };
        let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        let mime = crate::uttype::for_tag("public.filename-extension", ext, None).and_then(crate::uttype::mime_of).unwrap_or("application/octet-stream");
        return Ok(Response { status: 200, headers: vec![("Content-Type".into(), mime.into()), ("Content-Length".into(), body.len().to_string())], body });
    }
    if emu.network.path == crate::network::Path::Offline {
        return Err("offline".into());
    }
    let result = match emu.net.client.as_mut() {
        Some(c) => c.fetch(req),
        None => Err("offline".into()),
    };
    if emu.trace {
        println!("[webkit] fetch {} {} → {}", req.method, req.url, match &result {
            Ok(r) => format!("{} ({} bytes)", r.status, r.body.len()),
            Err(e) => e.clone(),
        });
    }
    result
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- navigation policy ----

/// Put a navigation to the delegate: `decidePolicyForNavigationAction:` with
/// a decision handler (or the variant with webpage preferences, which WebKit
/// prefers when the delegate has it). No delegate, or one that doesn't
/// decide: allowed.
#[allow(clippy::too_many_arguments)]
fn decide_policy(emu: &mut Emu, v: u64, id: u64, url: &str, method: &str, main_frame: bool, kind: NavigationType) -> Result<()> {
    let delegate = page(emu, v).navigation_delegate;
    let with_prefs = "webView:decidePolicyForNavigationAction:preferences:decisionHandler:";
    let plain = "webView:decidePolicyForNavigationAction:decisionHandler:";
    let responds = |emu: &mut Emu, sel: &str| -> Result<bool> {
        let s = objc::intern(emu, sel)?;
        Ok(delegate != 0 && objc::responds(emu, delegate, s)?)
    };
    let sel = if responds(emu, with_prefs)? {
        with_prefs
    } else if responds(emu, plain)? {
        plain
    } else {
        return command(emu, v, Command::Decide { id, allow: true });
    };
    let action = navigation_action(emu, v, url, method, main_frame, kind)?;
    // The handler knows its web view and navigation: (view, id) in a little record.
    let rec = emu.alloc(16)?;
    emu.write_u64(rec, v)?;
    emu.write_u64(rec + 8, id)?;
    if sel == with_prefs {
        let prefs = new_object(emu, "WKWebpagePreferences")?;
        let handler = data_block(emu, decided, rec)?;
        send(emu, delegate, sel, &[v, action, prefs, handler])?;
    } else {
        let handler = data_block(emu, decided, rec)?;
        send(emu, delegate, sel, &[v, action, handler])?;
    }
    Ok(())
}

/// The decision handler: `decisionHandler(WKNavigationActionPolicy)` (only the first call counts).
fn decided(emu: &mut Emu) -> Result<()> {
    let (b, policy) = (emu.arg(0), emu.arg(1));
    let rec = block_data(emu, b)?;
    let (v, id) = (emu.read_u64(rec)?, emu.read_u64(rec + 8)?);
    if v == 0 {
        return Ok(());
    }
    emu.write_u64(rec, 0)?;
    command(emu, v, Command::Decide { id, allow: policy != POLICY_CANCEL })
}

fn navigation_action(emu: &mut Emu, v: u64, url: &str, method: &str, main_frame: bool, kind: NavigationType) -> Result<u64> {
    let u = nsurl(emu, url)?;
    let rcls = host_class(emu, "NSMutableURLRequest")?;
    let request = send(emu, rcls, "requestWithURL:", &[u])?;
    if !method.is_empty() && method != "GET" {
        let m = string(emu, method)?;
        send(emu, request, "setHTTPMethod:", &[m])?;
    }
    let frame = frame_info(emu, v, main_frame, request)?;
    let source = frame_info(emu, v, true, 0)?;
    let a = new_object(emu, "WKNavigationAction")?;
    objc::bag_set(emu, a, "request", request);
    objc::bag_set(emu, a, "targetFrame", frame);
    objc::bag_set(emu, a, "sourceFrame", source);
    objc::bag_set(emu, a, "navigationType", kind as i64 as u64);
    Ok(a)
}

fn frame_info(emu: &mut Emu, v: u64, main: bool, request: u64) -> Result<u64> {
    let f = new_object(emu, "WKFrameInfo")?;
    objc::bag_set(emu, f, "mainFrame", main as u64);
    objc::bag_set(emu, f, "webView", v);
    objc::bag_set(emu, f, "request", request);
    Ok(f)
}

/// `window.open`, `target=_blank`: the UI delegate may make a web view for it; pomme
/// doesn't host popups, so whatever it returns is left as it is.
fn open_window(emu: &mut Emu, v: u64, url: &str) -> Result<()> {
    let delegate = page(emu, v).ui_delegate;
    let sel = "webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:";
    let s = objc::intern(emu, sel)?;
    if delegate == 0 || !objc::responds(emu, delegate, s)? {
        return Ok(());
    }
    let config = page(emu, v).config;
    let action = navigation_action(emu, v, url, "GET", false, NavigationType::LinkActivated)?;
    objc::bag_set(emu, action, "targetFrame", 0);
    let features = new_object(emu, "WKWindowFeatures")?;
    send(emu, delegate, sel, &[v, config, action, features])?;
    Ok(())
}

// ---- script messages ----

fn script_message(emu: &mut Emu, v: u64, name: &str, body: &Value, main_frame: bool) -> Result<()> {
    let config = page(emu, v).config;
    let controller = config_object(emu, config, "userContentController", "WKUserContentController")?;
    let Some(handler) = handlers(emu, controller).into_iter().find(|(n, _)| n == name).map(|(_, h)| h) else { return Ok(()) };
    let m = new_object(emu, "WKScriptMessage")?;
    let body = crate::json::build(emu, body)?;
    let name_s = string(emu, name)?;
    let frame = frame_info(emu, v, main_frame, 0)?;
    objc::bag_set(emu, m, "body", body);
    objc::bag_set(emu, m, "name", name_s);
    objc::bag_set(emu, m, "webView", v);
    objc::bag_set(emu, m, "frameInfo", frame);
    send(emu, handler, "userContentController:didReceiveScriptMessage:", &[controller, m])?;
    Ok(())
}

/// A controller's script message handlers, in the order added.
fn handlers(emu: &Emu, controller: u64) -> Vec<(String, u64)> {
    emu.webkit.handlers.get(&controller).cloned().unwrap_or_default()
}

/// A controller's user scripts (WKUserScript objects).
fn user_scripts(emu: &Emu, controller: u64) -> Vec<u64> {
    emu.webkit.user_scripts.get(&controller).cloned().unwrap_or_default()
}

fn user_script(emu: &mut Emu, s: u64) -> Result<UserScript> {
    let src = objc::bag_get(emu, s, "source").unwrap_or(0);
    let source = if src != 0 { foundation::nsstring(emu, src)? } else { String::new() };
    Ok(UserScript { source, at_end: objc::bag_get(emu, s, "injectionTime").unwrap_or(0) == 1, main_frame_only: objc::bag_get(emu, s, "forMainFrameOnly").unwrap_or(1) & 0xff != 0 })
}

/// The web views using `controller` (through their configurations).
fn views_with(emu: &Emu, controller: u64) -> Vec<u64> {
    let mut vs: Vec<u64> = emu.webkit.pages.iter().filter(|(_, p)| objc::bag_get(emu, p.config, "userContentController") == Some(controller)).map(|(&v, _)| v).collect();
    vs.sort();
    vs
}

fn add_handler(emu: &mut Emu) -> Result<()> {
    let (c, handler, name) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let name = foundation::nsstring(emu, name)?;
    let list = emu.webkit.handlers.entry(c).or_default();
    if list.iter().any(|(n, _)| *n == name) {
        // WebKit raises: a handler by that name exists.
        return Err(crate::Stop::Unsupported(format!("NSInvalidArgumentException: Attempt to add script message handler with name '{name}' when one already exists.")));
    }
    list.push((name.clone(), handler));
    retain(emu, handler)?;
    for v in views_with(emu, c) {
        command(emu, v, Command::AddMessageHandler(name.clone()))?;
    }
    Ok(())
}

fn remove_handler(emu: &mut Emu, name: Option<String>) -> Result<()> {
    let c = emu.arg(0);
    let list = emu.webkit.handlers.entry(c).or_default();
    let removed: Vec<String> = list.iter().filter(|(n, _)| name.as_ref().is_none_or(|x| x == n)).map(|(n, _)| n.clone()).collect();
    list.retain(|(n, _)| !removed.contains(n));
    for v in views_with(emu, c) {
        for n in &removed {
            command(emu, v, Command::RemoveMessageHandler(n.clone()))?;
        }
    }
    Ok(())
}

fn add_user_script(emu: &mut Emu) -> Result<()> {
    let (c, s) = (emu.arg(0), emu.arg(2));
    retain(emu, s)?;
    emu.webkit.user_scripts.entry(c).or_default().push(s);
    let script = user_script(emu, s)?;
    for v in views_with(emu, c) {
        command(emu, v, Command::AddUserScript(script.clone()))?;
    }
    Ok(())
}

fn remove_user_scripts(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    emu.webkit.user_scripts.remove(&c);
    for v in views_with(emu, c) {
        command(emu, v, Command::RemoveAllUserScripts)?;
    }
    Ok(())
}

// ---- WKWebViewConfiguration ----

/// A configuration's object property, made the first time it's asked for (as WebKit's are).
fn config_object(emu: &mut Emu, config: u64, key: &str, class: &str) -> Result<u64> {
    if config == 0 {
        return Ok(0);
    }
    if let Some(o) = objc::bag_get(emu, config, key).filter(|&o| o != 0) {
        return Ok(o);
    }
    let o = new_object(emu, class)?;
    objc::bag_set(emu, config, key, o);
    Ok(o)
}

/// `-copy`: a new configuration with the same settings, sharing the controller,
/// preferences, data store and process pool (WebKit copies them shallowly).
fn config_copy(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    for (key, class) in CONFIG_OBJECTS {
        config_object(emu, c, key, class)?;
    }
    let n = new_object(emu, "WKWebViewConfiguration")?;
    objc::bag_copy(emu, c, n);
    ret(emu, n)
}

const CONFIG_OBJECTS: &[(&str, &str)] = &[("userContentController", "WKUserContentController"), ("preferences", "WKPreferences"), ("defaultWebpagePreferences", "WKWebpagePreferences"), ("websiteDataStore", "WKWebsiteDataStore"), ("processPool", "WKProcessPool")];

fn page_config(emu: &mut Emu, v: u64, config: u64) -> Result<PageConfig> {
    let controller = config_object(emu, config, "userContentController", "WKUserContentController")?;
    let mut user_scripts_out = Vec::new();
    for s in user_scripts(emu, controller) {
        user_scripts_out.push(user_script(emu, s)?);
    }
    let app_name = objc::bag_get(emu, config, "applicationNameForUserAgent").unwrap_or(0);
    let application_name = if app_name != 0 { Some(foundation::nsstring(emu, app_name)?) } else { None };
    let b = crate::uiview::view(emu, v).bounds;
    Ok(PageConfig {
        user_scripts: user_scripts_out,
        message_handlers: handlers(emu, controller).into_iter().map(|(n, _)| n).collect(),
        width: b[2],
        height: b[3],
        scale: emu.device.scale,
        user_agent: None,
        application_name,
        inline_media: objc::bag_get(emu, config, "allowsInlineMediaPlayback").unwrap_or(0) & 0xff != 0,
        // WKAudiovisualMediaTypeAll (the default) needs a tap for any media; none: autoplay.
        autoplay: objc::bag_get(emu, config, "mediaTypesRequiringUserActionForPlayback").unwrap_or(u64::MAX) == 0,
    })
}

// ---- WKWebView ----

/// `-initWithFrame:configuration:` (and `-initWithFrame:`, with a default configuration).
fn web_view_init(emu: &mut Emu, with_config: bool) -> Result<()> {
    let (v, config) = (emu.arg(0), if with_config { emu.arg(2) } else { 0 });
    crate::uiview::view_init_with_frame(emu)?;
    // The web view keeps its own copy of the configuration.
    let config = if config != 0 {
        send(emu, config, "copy", &[])?
    } else {
        let c = new_object(emu, "WKWebViewConfiguration")?;
        send(emu, c, "init", &[])?
    };
    for (key, class) in CONFIG_OBJECTS {
        config_object(emu, config, key, class)?;
    }
    // Its content scrolls in a scroll view filling it.
    let b = crate::uiview::view(emu, v).bounds;
    let sv = new_object(emu, "UIScrollView")?;
    objc::send_fp(emu, sv, "initWithFrame:", &[], &[0.0, 0.0, b[2], b[3]])?;
    send(emu, sv, "setAutoresizingMask:", &[18])?; // flexible width and height
    send(emu, v, "addSubview:", &[sv])?;
    emu.webkit.pages.insert(v, Page { config, scroll_view: sv, size: (b[2], b[3]), ..Default::default() });
    let pc = page_config(emu, v, config)?;
    command(emu, v, Command::Create(pc))?;
    ret(emu, v)
}

/// `-loadHTMLString:baseURL:`: the page is the string, its URL the base (about:blank without one).
fn load_html(emu: &mut Emu) -> Result<()> {
    let (v, html, base) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let html = if html != 0 { foundation::nsstring(emu, html)? } else { String::new() };
    let base = url_string(emu, base)?;
    let (nav, n) = start_navigation(emu)?;
    page_mut(emu, v).loading = true;
    command(emu, v, Command::LoadHtml { nav, html, base })?;
    ret(emu, n)
}

/// `-loadData:MIMEType:characterEncodingName:baseURL:`: HTML (and text) as a page.
fn load_data(emu: &mut Emu) -> Result<()> {
    let (v, data, base) = (emu.arg(0), emu.arg(2), emu.arg(5));
    let bytes = if data != 0 { foundation::data_bytes(emu, data)? } else { Vec::new() };
    let html = String::from_utf8_lossy(&bytes).into_owned();
    let base = url_string(emu, base)?;
    let (nav, n) = start_navigation(emu)?;
    page_mut(emu, v).loading = true;
    command(emu, v, Command::LoadHtml { nav, html, base })?;
    ret(emu, n)
}

fn load_request(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), emu.arg(2));
    let url = send(emu, r, "URL", &[])?;
    let url = url_string(emu, url)?.unwrap_or_default();
    let m = send(emu, r, "HTTPMethod", &[])?;
    let method = if m != 0 { foundation::nsstring(emu, m)? } else { "GET".into() };
    let h = send(emu, r, "allHTTPHeaderFields", &[])?;
    let mut headers = Vec::new();
    if h != 0 {
        let d = foundation::as_cf_dict(emu, h)?;
        for (k, val) in foundation::dict_pairs(emu, d)? {
            headers.push((foundation::nsstring(emu, k)?, foundation::nsstring(emu, val)?));
        }
    }
    let b = send(emu, r, "HTTPBody", &[])?;
    let body = if b != 0 { foundation::data_bytes(emu, b)? } else { Vec::new() };
    let (nav, n) = start_navigation(emu)?;
    page_mut(emu, v).loading = true;
    command(emu, v, Command::LoadRequest { nav, request: Request { method, url, headers, body } })?;
    ret(emu, n)
}

/// `-loadFileURL:allowingReadAccessToURL:`: the engine fetches the file (and what it
/// links to) from the emulator, which has the app's files.
fn load_file(emu: &mut Emu) -> Result<()> {
    let (v, url) = (emu.arg(0), emu.arg(2));
    let url = url_string(emu, url)?.unwrap_or_default();
    let (nav, n) = start_navigation(emu)?;
    page_mut(emu, v).loading = true;
    command(emu, v, Command::LoadRequest { nav, request: Request { method: "GET".into(), url, headers: vec![], body: vec![] } })?;
    ret(emu, n)
}

fn history(emu: &mut Emu, which: u8) -> Result<()> {
    let v = emu.arg(0);
    let (nav, n) = start_navigation(emu)?;
    let cmd = match which {
        0 => Command::Reload { nav },
        1 => Command::GoBack { nav },
        _ => Command::GoForward { nav },
    };
    command(emu, v, cmd)?;
    ret(emu, n)
}

/// `-evaluateJavaScript:completionHandler:` (and the variants with a frame and content world).
fn evaluate(emu: &mut Emu, block_arg: usize) -> Result<()> {
    let (v, script, block) = (emu.arg(0), emu.arg(2), emu.arg(block_arg));
    let script = foundation::nsstring(emu, script)?;
    let block = if block != 0 { crate::libc::copy_block(emu, block)? } else { 0 };
    let id = new_id(emu);
    emu.webkit.scripts.insert(id, (v, block));
    command(emu, v, Command::Evaluate { id, script })
}

/// The web view's size changes with it: the page is told.
fn layout_subviews(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let b = crate::uiview::view(emu, v).bounds;
    let pg = page(emu, v);
    if pg.size != (b[2], b[3]) && emu.webkit.pages.contains_key(&v) {
        page_mut(emu, v).size = (b[2], b[3]);
        let scale = emu.device.scale;
        command(emu, v, Command::Resize { width: b[2], height: b[3], scale })?;
    }
    Ok(())
}

fn dealloc(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    if emu.webkit.pages.remove(&v).is_some() {
        if let Some(e) = emu.webkit.engine.as_mut() {
            e.send(v, Command::Close);
        }
        emu.webkit.scripts.retain(|_, (w, _)| *w != v);
    }
    Ok(())
}

// ---- properties ----

/// A stored property with WebKit's default (a configuration or preferences flag).
macro_rules! flag {
    ($class:expr, $get:literal, $set:literal, $key:literal, $default:expr) => {
        [
            ($class, false, $get, (|e| {
                let r = objc::bag_get(e, e.arg(0), $key).unwrap_or($default);
                ret(e, r)
            }) as HostFn),
            ($class, false, $set, (|e| {
                let (o, x) = (e.arg(0), e.arg(2));
                objc::bag_set(e, o, $key, x);
                Ok(())
            }) as HostFn),
        ]
    };
}

const WV: &str = "WKWebView";
const CFG: &str = "WKWebViewConfiguration";
const UCC: &str = "WKUserContentController";

pub(crate) const FLAGS: &[[(&str, bool, &str, HostFn); 2]] = &[
    flag!(CFG, "allowsInlineMediaPlayback", "setAllowsInlineMediaPlayback:", "allowsInlineMediaPlayback", 0),
    flag!(CFG, "mediaTypesRequiringUserActionForPlayback", "setMediaTypesRequiringUserActionForPlayback:", "mediaTypesRequiringUserActionForPlayback", u64::MAX),
    flag!(CFG, "allowsAirPlayForMediaPlayback", "setAllowsAirPlayForMediaPlayback:", "allowsAirPlayForMediaPlayback", 1),
    flag!(CFG, "allowsPictureInPictureMediaPlayback", "setAllowsPictureInPictureMediaPlayback:", "allowsPictureInPictureMediaPlayback", 1),
    flag!(CFG, "applicationNameForUserAgent", "setApplicationNameForUserAgent:", "applicationNameForUserAgent", 0),
    flag!(CFG, "dataDetectorTypes", "setDataDetectorTypes:", "dataDetectorTypes", 0),
    flag!(CFG, "ignoresViewportScaleLimits", "setIgnoresViewportScaleLimits:", "ignoresViewportScaleLimits", 0),
    flag!(CFG, "suppressesIncrementalRendering", "setSuppressesIncrementalRendering:", "suppressesIncrementalRendering", 0),
    flag!(CFG, "limitsNavigationsToAppBoundDomains", "setLimitsNavigationsToAppBoundDomains:", "limitsNavigationsToAppBoundDomains", 0),
    flag!(CFG, "upgradeKnownHostsToHTTPS", "setUpgradeKnownHostsToHTTPS:", "upgradeKnownHostsToHTTPS", 1),
    flag!(CFG, "selectionGranularity", "setSelectionGranularity:", "selectionGranularity", 0),
    flag!(CFG, "userContentController", "setUserContentController:", "userContentController", 0),
    flag!(CFG, "preferences", "setPreferences:", "preferences", 0),
    flag!(CFG, "defaultWebpagePreferences", "setDefaultWebpagePreferences:", "defaultWebpagePreferences", 0),
    flag!(CFG, "websiteDataStore", "setWebsiteDataStore:", "websiteDataStore", 0),
    flag!(CFG, "processPool", "setProcessPool:", "processPool", 0),
    flag!("WKPreferences", "javaScriptEnabled", "setJavaScriptEnabled:", "javaScriptEnabled", 1),
    flag!("WKPreferences", "javaScriptCanOpenWindowsAutomatically", "setJavaScriptCanOpenWindowsAutomatically:", "javaScriptCanOpenWindowsAutomatically", 0),
    flag!("WKPreferences", "minimumFontSize", "setMinimumFontSize:", "minimumFontSize", 0),
    flag!("WKPreferences", "isFraudulentWebsiteWarningEnabled", "setFraudulentWebsiteWarningEnabled:", "fraudulentWebsiteWarningEnabled", 1),
    flag!("WKPreferences", "isElementFullscreenEnabled", "setElementFullscreenEnabled:", "elementFullscreenEnabled", 0),
    flag!("WKPreferences", "isTextInteractionEnabled", "setTextInteractionEnabled:", "textInteractionEnabled", 1),
    flag!("WKWebpagePreferences", "allowsContentJavaScript", "setAllowsContentJavaScript:", "allowsContentJavaScript", 1),
    flag!("WKWebpagePreferences", "preferredContentMode", "setPreferredContentMode:", "preferredContentMode", 0),
    flag!(WV, "allowsBackForwardNavigationGestures", "setAllowsBackForwardNavigationGestures:", "allowsBackForwardNavigationGestures", 0),
    flag!(WV, "allowsLinkPreview", "setAllowsLinkPreview:", "allowsLinkPreview", 1),
    flag!(WV, "isInspectable", "setInspectable:", "inspectable", 0),
    flag!(WV, "underPageBackgroundColor", "setUnderPageBackgroundColor:", "underPageBackgroundColor", 0),
];

/// A property read off a bag (WKNavigationAction, WKFrameInfo, WKScriptMessage, WKUserScript).
macro_rules! getter {
    ($class:expr, $get:literal, $key:literal, $default:expr) => {
        ($class, false, $get, (|e| {
            let r = objc::bag_get(e, e.arg(0), $key).unwrap_or($default);
            ret(e, r)
        }) as HostFn)
    };
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // WKWebViewConfiguration
    (CFG, false, "init", |e| {
        let c = e.arg(0);
        for (key, class) in CONFIG_OBJECTS {
            config_object(e, c, key, class)?;
        }
        ret(e, c)
    }),
    (CFG, false, "copy", config_copy),
    (CFG, false, "copyWithZone:", config_copy),
    (CFG, false, "setURLSchemeHandler:forURLScheme:", |_| Ok(())),
    (CFG, false, "urlSchemeHandlerForURLScheme:", |e| ret(e, 0)),
    // WKUserContentController
    (UCC, false, "addScriptMessageHandler:name:", add_handler),
    (UCC, false, "addScriptMessageHandler:contentWorld:name:", |e| {
        e.cpu.x[3] = e.cpu.x[4];
        add_handler(e)
    }),
    (UCC, false, "removeScriptMessageHandlerForName:", |e| {
        let n = foundation::nsstring(e, e.arg(2))?;
        remove_handler(e, Some(n))
    }),
    (UCC, false, "removeScriptMessageHandlerForName:contentWorld:", |e| {
        let n = foundation::nsstring(e, e.arg(2))?;
        remove_handler(e, Some(n))
    }),
    (UCC, false, "removeAllScriptMessageHandlers", |e| remove_handler(e, None)),
    (UCC, false, "addUserScript:", add_user_script),
    (UCC, false, "removeAllUserScripts", remove_user_scripts),
    (UCC, false, "userScripts", |e| {
        let list = user_scripts(e, e.arg(0));
        let a = foundation::new_array_of(e, &list)?;
        ret(e, a)
    }),
    (UCC, false, "addContentRuleList:", |_| Ok(())),
    (UCC, false, "removeAllContentRuleLists", |_| Ok(())),
    // WKUserScript
    ("WKUserScript", false, "initWithSource:injectionTime:forMainFrameOnly:", |e| {
        let (s, src, time, main) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4) & 0xff);
        let src = if src != 0 { send(e, src, "copy", &[])? } else { 0 };
        objc::bag_set(e, s, "source", src);
        objc::bag_set(e, s, "injectionTime", time);
        objc::bag_set(e, s, "forMainFrameOnly", main);
        ret(e, s)
    }),
    ("WKUserScript", false, "initWithSource:injectionTime:forMainFrameOnly:inContentWorld:", |e| {
        let (s, src, time, main) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4) & 0xff);
        let src = if src != 0 { send(e, src, "copy", &[])? } else { 0 };
        objc::bag_set(e, s, "source", src);
        objc::bag_set(e, s, "injectionTime", time);
        objc::bag_set(e, s, "forMainFrameOnly", main);
        ret(e, s)
    }),
    getter!("WKUserScript", "source", "source", 0),
    getter!("WKUserScript", "injectionTime", "injectionTime", 0),
    getter!("WKUserScript", "isForMainFrameOnly", "forMainFrameOnly", 1),
    // WKNavigationAction, WKFrameInfo, WKScriptMessage
    getter!("WKNavigationAction", "request", "request", 0),
    getter!("WKNavigationAction", "targetFrame", "targetFrame", 0),
    getter!("WKNavigationAction", "sourceFrame", "sourceFrame", 0),
    getter!("WKNavigationAction", "navigationType", "navigationType", u64::MAX),
    getter!("WKNavigationAction", "buttonNumber", "buttonNumber", 0),
    getter!("WKNavigationAction", "modifierFlags", "modifierFlags", 0),
    getter!("WKNavigationAction", "shouldPerformDownload", "shouldPerformDownload", 0),
    getter!("WKFrameInfo", "isMainFrame", "mainFrame", 1),
    getter!("WKFrameInfo", "request", "request", 0),
    getter!("WKFrameInfo", "webView", "webView", 0),
    getter!("WKFrameInfo", "securityOrigin", "securityOrigin", 0),
    getter!("WKScriptMessage", "body", "body", 0),
    getter!("WKScriptMessage", "name", "name", 0),
    getter!("WKScriptMessage", "webView", "webView", 0),
    getter!("WKScriptMessage", "frameInfo", "frameInfo", 0),
    getter!("WKScriptMessage", "world", "world", 0),
    ("WKWebsiteDataStore", true, "defaultDataStore", |e| {
        if e.webkit.default_store == 0 {
            e.webkit.default_store = new_object(e, "WKWebsiteDataStore")?;
        }
        ret(e, e.webkit.default_store)
    }),
    ("WKWebsiteDataStore", true, "nonPersistentDataStore", |e| {
        let s = new_object(e, "WKWebsiteDataStore")?;
        ret(e, s)
    }),
    // WKWebView
    (WV, false, "initWithFrame:configuration:", |e| web_view_init(e, true)),
    (WV, false, "initWithFrame:", |e| web_view_init(e, false)),
    (WV, false, "configuration", |e| {
        let c = page(e, e.arg(0)).config;
        let c = if c != 0 { send(e, c, "copy", &[])? } else { 0 };
        ret(e, c)
    }),
    (WV, false, "navigationDelegate", |e| {
        let d = page(e, e.arg(0)).navigation_delegate;
        ret(e, d)
    }),
    (WV, false, "setNavigationDelegate:", |e| {
        let (v, d) = (e.arg(0), e.arg(2));
        page_mut(e, v).navigation_delegate = d; // weak, as WebKit's
        Ok(())
    }),
    (WV, false, "UIDelegate", |e| {
        let d = page(e, e.arg(0)).ui_delegate;
        ret(e, d)
    }),
    (WV, false, "setUIDelegate:", |e| {
        let (v, d) = (e.arg(0), e.arg(2));
        page_mut(e, v).ui_delegate = d;
        Ok(())
    }),
    (WV, false, "scrollView", |e| {
        let s = page(e, e.arg(0)).scroll_view;
        ret(e, s)
    }),
    (WV, false, "URL", |e| {
        let u = page(e, e.arg(0)).url;
        ret(e, u)
    }),
    (WV, false, "title", |e| {
        let t = page(e, e.arg(0)).title;
        let s = opt_string(e, t.as_deref())?;
        ret(e, s)
    }),
    (WV, false, "isLoading", |e| {
        let l = page(e, e.arg(0)).loading;
        ret(e, l as u64)
    }),
    (WV, false, "estimatedProgress", |e| {
        let p = page(e, e.arg(0)).progress;
        crate::uikit::ret_f64(e, p);
        Ok(())
    }),
    (WV, false, "canGoBack", |e| ret(e, 0)),
    (WV, false, "canGoForward", |e| ret(e, 0)),
    (WV, false, "hasOnlySecureContent", |e| {
        let u = page(e, e.arg(0)).url;
        let u = url_string(e, u)?.unwrap_or_default();
        ret(e, u.starts_with("https:") as u64)
    }),
    (WV, false, "serverTrust", |e| ret(e, 0)),
    (WV, false, "themeColor", |e| ret(e, 0)),
    (WV, false, "customUserAgent", |e| {
        let ua = page(e, e.arg(0)).custom_user_agent;
        let s = opt_string(e, ua.as_deref())?;
        ret(e, s)
    }),
    (WV, false, "setCustomUserAgent:", |e| {
        let (v, s) = (e.arg(0), e.arg(2));
        let ua = if s != 0 { Some(foundation::nsstring(e, s)?).filter(|s| !s.is_empty()) } else { None };
        page_mut(e, v).custom_user_agent = ua.clone();
        command(e, v, Command::UserAgent(ua))
    }),
    (WV, false, "loadHTMLString:baseURL:", load_html),
    (WV, false, "loadData:MIMEType:characterEncodingName:baseURL:", load_data),
    (WV, false, "loadRequest:", load_request),
    (WV, false, "loadFileURL:allowingReadAccessToURL:", load_file),
    (WV, false, "reload", |e| history(e, 0)),
    (WV, false, "reloadFromOrigin", |e| history(e, 0)),
    (WV, false, "goBack", |e| history(e, 1)),
    (WV, false, "goForward", |e| history(e, 2)),
    (WV, false, "stopLoading", |e| {
        let v = e.arg(0);
        page_mut(e, v).loading = false;
        command(e, v, Command::StopLoading)
    }),
    (WV, false, "evaluateJavaScript:completionHandler:", |e| evaluate(e, 3)),
    (WV, false, "evaluateJavaScript:inFrame:inContentWorld:completionHandler:", |e| evaluate(e, 5)),
    (WV, false, "layoutSubviews", layout_subviews),
    (WV, false, "dealloc", dealloc),
];

// ---- a scripted engine, for tests ----

/// What a scripted page can do when something happens to it.
pub struct ScriptedPage<'a> {
    page: PageId,
    out: &'a mut Vec<(PageId, Event)>,
    next: &'a mut u64,
    pending: &'a mut HashMap<u64, String>,
    handlers: &'a [String],
}

impl ScriptedPage<'_> {
    /// The page navigates (a script setting `location.href`): put to the delegate first.
    pub fn navigate(&mut self, url: &str) {
        *self.next += 1;
        let id = ENGINE_IDS + *self.next;
        self.pending.insert(id, url.to_string());
        self.out.push((self.page, Event::Policy { id, url: url.into(), method: "GET".into(), main_frame: true, kind: NavigationType::Other }));
    }
    /// `window.webkit.messageHandlers[name].postMessage(body)` (nothing if no such handler).
    pub fn post_message(&mut self, name: &str, body: Value) {
        if self.handlers.iter().any(|h| h == name) {
            self.out.push((self.page, Event::Message { name: name.into(), body, main_frame: true }));
        }
    }
    pub fn set_title(&mut self, title: &str) {
        self.out.push((self.page, Event::Title(Some(title.into()))));
    }
}

type OnLoad = Box<dyn FnMut(&mut ScriptedPage, &str) + Send>;
type OnEval = Box<dyn FnMut(&mut ScriptedPage, &str) -> std::result::Result<Option<Value>, String> + Send>;

/// Everything the app asked of a scripted page, for tests to check.
#[derive(Clone, Debug, Default)]
pub struct Log {
    /// Every command, in order, with its page.
    pub commands: Vec<(PageId, Command)>,
    /// The documents loaded (HTML, or the URL requested) and their URLs.
    pub loaded: Vec<(String, Option<String>)>,
    /// Scripts evaluated.
    pub scripts: Vec<String>,
    /// Navigations the delegate allowed and cancelled (URLs).
    pub allowed: Vec<String>,
    pub cancelled: Vec<String>,
}

#[derive(Default)]
struct ScriptedState {
    log: Log,
    out: Vec<(PageId, Event)>,
    next: u64,
    /// Policy id → URL, waiting for the delegate's decision.
    pending: HashMap<u64, String>,
    handlers: HashMap<PageId, Vec<String>>,
    on_load: Option<OnLoad>,
    on_eval: Vec<(String, OnEval)>,
}

/// A web engine with no browser behind it: pages are what the test makes them.
/// Navigations are put to the delegate and, when allowed, start, commit and
/// finish at once; once a document loads, `on_load` runs (as the page's
/// scripts would); a script evaluated runs the first `on_eval` whose text
/// it contains, or is undefined.
#[derive(Clone, Default)]
pub struct Scripted(Arc<Mutex<ScriptedState>>);

impl Scripted {
    pub fn new() -> Self {
        Self::default()
    }
    /// What the page does once a document loads (given the document: its HTML, or its URL).
    pub fn on_load(self, f: impl FnMut(&mut ScriptedPage, &str) + Send + 'static) -> Self {
        self.0.lock().unwrap().on_load = Some(Box::new(f));
        self
    }
    /// What a script containing `text` does, and its result.
    pub fn on_eval(self, text: &str, f: impl FnMut(&mut ScriptedPage, &str) -> std::result::Result<Option<Value>, String> + Send + 'static) -> Self {
        self.0.lock().unwrap().on_eval.push((text.into(), Box::new(f)));
        self
    }
    /// What the app has done so far.
    pub fn log(&self) -> Log {
        self.0.lock().unwrap().log.clone()
    }
}

impl ScriptedState {
    fn page<'a>(&'a mut self, page: PageId, handlers: &'a [String]) -> ScriptedPage<'a> {
        ScriptedPage { page, out: &mut self.out, next: &mut self.next, pending: &mut self.pending, handlers }
    }

    fn load(&mut self, page: PageId, nav: u64, url: String, document: String) {
        self.log.loaded.push((document.clone(), Some(url.clone())));
        self.pending.insert(nav, format!("\u{0}{document}\u{0}{url}"));
        self.out.push((page, Event::Policy { id: nav, url, method: "GET".into(), main_frame: true, kind: NavigationType::Other }));
    }
}

impl WebEngine for Scripted {
    fn send(&mut self, page: PageId, cmd: Command) {
        let mut st = self.0.lock().unwrap();
        st.log.commands.push((page, cmd.clone()));
        match cmd {
            Command::Create(c) => {
                st.handlers.insert(page, c.message_handlers);
            }
            Command::AddMessageHandler(n) => st.handlers.entry(page).or_default().push(n),
            Command::RemoveMessageHandler(n) => st.handlers.entry(page).or_default().retain(|h| *h != n),
            Command::LoadHtml { nav, html, base } => {
                let url = base.unwrap_or_else(|| "about:blank".into());
                st.load(page, nav, url, html);
            }
            Command::LoadRequest { nav, request } => {
                let url = request.url.clone();
                st.load(page, nav, url.clone(), url);
            }
            Command::Decide { id, allow } => {
                let Some(what) = st.pending.remove(&id) else { return };
                // A load the app started carries its document; a page's own navigation is just a URL.
                let (document, url) = match what.strip_prefix('\u{0}').and_then(|r| r.rsplit_once('\u{0}')) {
                    Some((doc, url)) => (Some(doc.to_string()), url.to_string()),
                    None => (None, what),
                };
                if !allow {
                    st.log.cancelled.push(url);
                    return;
                }
                st.log.allowed.push(url.clone());
                st.out.push((page, Event::Started { nav: id, url: url.clone() }));
                st.out.push((page, Event::Committed { nav: id, url: url.clone() }));
                st.out.push((page, Event::Finished { nav: id }));
                let doc = document.unwrap_or(url);
                if let Some(mut f) = st.on_load.take() {
                    let handlers = st.handlers.get(&page).cloned().unwrap_or_default();
                    f(&mut st.page(page, &handlers), &doc);
                    st.on_load = Some(f);
                }
            }
            Command::Evaluate { id, script } => {
                st.log.scripts.push(script.clone());
                let handlers = st.handlers.get(&page).cloned().unwrap_or_default();
                let mut rules = std::mem::take(&mut st.on_eval);
                let mut result = Ok(None);
                if let Some((_, f)) = rules.iter_mut().find(|(t, _)| script.contains(t.as_str())) {
                    result = f(&mut st.page(page, &handlers), &script);
                }
                st.on_eval = rules;
                st.out.push((page, Event::Evaluated { id, result }));
            }
            _ => {}
        }
    }

    fn poll(&mut self) -> Vec<(PageId, Event)> {
        std::mem::take(&mut self.0.lock().unwrap().out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_loads_after_the_delegate_allows() {
        let mut page = Scripted::new().on_load(|p, doc| {
            if doc.contains("ready") {
                p.navigate("app://ready");
            }
        });
        page.send(1, Command::LoadHtml { nav: 7, html: "<p>ready</p>".into(), base: Some("https://example.com/".into()) });
        let ev = page.poll();
        assert_eq!(ev, vec![(1, Event::Policy { id: 7, url: "https://example.com/".into(), method: "GET".into(), main_frame: true, kind: NavigationType::Other })]);
        page.send(1, Command::Decide { id: 7, allow: true });
        let ev = page.poll();
        assert!(matches!(ev[0].1, Event::Started { nav: 7, .. }));
        assert!(matches!(ev[2].1, Event::Finished { nav: 7 }));
        // The page's own navigation, put to the delegate, which cancels it.
        let Event::Policy { id, ref url, .. } = ev[3].1 else { panic!("{ev:?}") };
        assert_eq!(url, "app://ready");
        assert!(id >= ENGINE_IDS);
        page.send(1, Command::Decide { id, allow: false });
        assert!(page.poll().is_empty());
        assert_eq!(page.log().cancelled, vec!["app://ready".to_string()]);
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("/a%20b/c%2"), "/a b/c%2");
    }
}
