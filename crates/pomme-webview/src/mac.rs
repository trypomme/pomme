//! The helper on macOS: one WKWebView per page, each in a window of its own
//! that's never on screen (pages in an off-screen window still run at full
//! speed: timers, video), on the process's main thread.
//!
//! WebKit's own callbacks are the events: `decidePolicyForNavigationAction`
//! (its decision handler is kept until the emulator answers), the navigation
//! callbacks, script message handlers. Pixels come from `takeSnapshot` about
//! 30 times a second, sent only when they change.

use crate::emit;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSImage, NSWindow, NSWindowStyleMask};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo};
use objc2_foundation::{NSArray, NSData, NSError, NSJSONSerialization, NSJSONWritingOptions, NSMutableURLRequest, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer, NSURL};
use objc2_web_kit::{
    WKAudiovisualMediaTypes, WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate, WKScriptMessage, WKScriptMessageHandler, WKSnapshotConfiguration, WKUIDelegate, WKUserContentController, WKUserScript, WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration,
    WKWindowFeatures,
};
use pomme_hle::webkit::{Command, ENGINE_IDS, Event, NavigationType, PageConfig, PageId, UserScript, WebError};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// The script message handler pages' console goes through.
const CONSOLE: &str = "__pommeConsole";
const CONSOLE_SCRIPT: &str = r#"(function () {
  var post = function (level, args) {
    try { window.webkit.messageHandlers.__pommeConsole.postMessage(level + ": " + Array.prototype.map.call(args, String).join(" ")); } catch (e) {}
  };
  ["log", "info", "warn", "error", "debug"].forEach(function (level) {
    var original = console[level];
    console[level] = function () { post(level, arguments); return original.apply(console, arguments); };
  });
  window.addEventListener("error", function (e) { post("error", [e.message + " at " + e.filename + ":" + e.lineno]); });
})();"#;

/// Snapshots a second.
const FPS: f64 = 30.0;

struct Page {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    _delegate: Retained<Delegate>,
    width: f64,
    height: f64,
    scale: f64,
    /// A snapshot is being taken; the last pixels sent (their hash) and title.
    snapping: bool,
    last: u64,
    title: Option<String>,
    /// The finger on the page: where it went down, where it is, and whether it's scrolling.
    finger: Option<Finger>,
}

#[derive(Clone, Copy)]
struct Finger {
    start: (f64, f64),
    last: (f64, f64),
    scrolling: bool,
}

/// How far a finger moves before it scrolls rather than taps (UIKit's 10 pt).
const SLOP: f64 = 10.0;

#[derive(Default)]
struct Host {
    pages: HashMap<PageId, Page>,
    /// Policy id → WebKit's decision handler, waiting for the emulator.
    policies: HashMap<u64, RcBlock<dyn Fn(WKNavigationActionPolicy)>>,
    /// WKNavigation → navigation id (kept, so the address isn't reused).
    navigations: HashMap<usize, (u64, Retained<WKNavigation>)>,
    next: u64,
}

impl Host {
    fn id(&mut self) -> u64 {
        self.next += 1;
        ENGINE_IDS + self.next
    }

    /// The id of a WebKit navigation (a new one for navigations the page started).
    fn navigation(&mut self, n: Option<&WKNavigation>) -> u64 {
        let Some(n) = n else { return self.id() };
        let key = n as *const WKNavigation as usize;
        if let Some((id, _)) = self.navigations.get(&key) {
            return *id;
        }
        let id = self.id();
        self.navigations.insert(key, (id, n.retain()));
        id
    }

    fn adopt(&mut self, n: Option<Retained<WKNavigation>>, id: u64) {
        if let Some(n) = n {
            self.navigations.insert(&*n as *const WKNavigation as usize, (id, n));
        }
    }
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::new(Host::default());
}

pub struct Ivars {
    page: PageId,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; Delegate doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl WKNavigationDelegate for Delegate {
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(&self, _view: &WKWebView, action: &WKNavigationAction, handler: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>) {
            let request = unsafe { action.request() };
            let url = request.URL().and_then(|u| u.absoluteString()).map(|s| s.to_string()).unwrap_or_default();
            let method = request.HTTPMethod().map(|s| s.to_string()).unwrap_or_else(|| "GET".into());
            let main_frame = unsafe { action.targetFrame() }.is_some_and(|f| unsafe { f.isMainFrame() });
            let kind = match unsafe { action.navigationType() }.0 {
                0 => NavigationType::LinkActivated,
                1 => NavigationType::FormSubmitted,
                2 => NavigationType::BackForward,
                3 => NavigationType::Reload,
                4 => NavigationType::FormResubmitted,
                _ => NavigationType::Other,
            };
            let id = HOST.with_borrow_mut(|h| {
                let id = h.id();
                h.policies.insert(id, handler.copy());
                id
            });
            emit(self.ivars().page, Event::Policy { id, url, method, main_frame, kind });
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn started(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            let nav = HOST.with_borrow_mut(|h| h.navigation(navigation));
            emit(self.ivars().page, Event::Started { nav, url: url_of(view) });
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn committed(&self, view: &WKWebView, navigation: Option<&WKNavigation>) {
            let nav = HOST.with_borrow_mut(|h| h.navigation(navigation));
            emit(self.ivars().page, Event::Committed { nav, url: url_of(view) });
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, _view: &WKWebView, navigation: Option<&WKNavigation>) {
            let nav = HOST.with_borrow_mut(|h| h.navigation(navigation));
            emit(self.ivars().page, Event::Finished { nav });
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_provisional(&self, _view: &WKWebView, navigation: Option<&WKNavigation>, error: &NSError) {
            let nav = HOST.with_borrow_mut(|h| h.navigation(navigation));
            emit(self.ivars().page, Event::Failed { nav, provisional: true, error: web_error(error) });
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, _view: &WKWebView, navigation: Option<&WKNavigation>, error: &NSError) {
            let nav = HOST.with_borrow_mut(|h| h.navigation(navigation));
            emit(self.ivars().page, Event::Failed { nav, provisional: false, error: web_error(error) });
        }
    }

    unsafe impl WKUIDelegate for Delegate {
        #[unsafe(method(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn create(&self, _view: &WKWebView, _config: &WKWebViewConfiguration, action: &WKNavigationAction, _features: &WKWindowFeatures) -> *mut WKWebView {
            let url = unsafe { action.request() }.URL().and_then(|u| u.absoluteString()).map(|s| s.to_string()).unwrap_or_default();
            emit(self.ivars().page, Event::OpenWindow { url });
            std::ptr::null_mut()
        }
    }

    unsafe impl WKScriptMessageHandler for Delegate {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        fn message(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
            let name = unsafe { message.name() }.to_string();
            let body = unsafe { message.body() };
            if name == CONSOLE {
                let text = to_json(Some(&body)).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
                return emit(self.ivars().page, Event::Console(text));
            }
            let main_frame = unsafe { message.frameInfo().isMainFrame() };
            let body = to_json(Some(&body)).unwrap_or(Value::Null);
            emit(self.ivars().page, Event::Message { name, body, main_frame });
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker, page: PageId) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars { page });
        unsafe { msg_send![super(this), init] }
    }
}

fn url_of(view: &WKWebView) -> String {
    unsafe { view.URL() }.and_then(|u| u.absoluteString()).map(|s| s.to_string()).unwrap_or_default()
}

fn web_error(e: &NSError) -> WebError {
    WebError { domain: e.domain().to_string(), code: e.code() as i64, description: e.localizedDescription().to_string() }
}

/// A value WebKit handed over (from a script or a message), as JSON (None: nil, JavaScript's undefined).
fn to_json(obj: Option<&AnyObject>) -> Option<Value> {
    let obj = obj?;
    // Wrapped in an array, any JSON value is a valid top level.
    let array = NSArray::from_slice(&[obj]);
    if !unsafe { NSJSONSerialization::isValidJSONObject(&array) } {
        return Some(Value::Null); // dates and the like: not JSON
    }
    let data = unsafe { NSJSONSerialization::dataWithJSONObject_options_error(&array, NSJSONWritingOptions::empty()) }.ok()?;
    let mut v: Value = serde_json::from_slice(&data.to_vec()).ok()?;
    Some(v.get_mut(0).map(Value::take).unwrap_or(Value::Null))
}

fn nsstring(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn user_script(mtm: MainThreadMarker, s: &UserScript) -> Retained<WKUserScript> {
    let time = if s.at_end { WKUserScriptInjectionTime::AtDocumentEnd } else { WKUserScriptInjectionTime::AtDocumentStart };
    unsafe { WKUserScript::initWithSource_injectionTime_forMainFrameOnly(WKUserScript::alloc(mtm), &nsstring(&s.source), time, s.main_frame_only) }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w.max(1.0), h.max(1.0)))
}

/// Far off every screen: a window there is never seen, and its page still runs.
const OFF_SCREEN: f64 = -30000.0;

fn create(mtm: MainThreadMarker, p: PageId, cfg: PageConfig) {
    let config = unsafe { WKWebViewConfiguration::new(mtm) };
    let media = if cfg.autoplay { WKAudiovisualMediaTypes::None } else { WKAudiovisualMediaTypes::All };
    unsafe { config.setMediaTypesRequiringUserActionForPlayback(media) };
    if let Some(name) = &cfg.application_name {
        unsafe { config.setApplicationNameForUserAgent(Some(&nsstring(name))) };
    }
    let delegate = Delegate::new(mtm, p);
    let controller = unsafe { config.userContentController() };
    unsafe {
        controller.addUserScript(&user_script(mtm, &UserScript { source: CONSOLE_SCRIPT.into(), at_end: false, main_frame_only: false }));
        controller.addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &nsstring(CONSOLE));
        for s in &cfg.user_scripts {
            controller.addUserScript(&user_script(mtm, s));
        }
        for n in &cfg.message_handlers {
            controller.addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &nsstring(n));
        }
    }
    let frame = rect(0.0, 0.0, cfg.width, cfg.height);
    let view = unsafe { WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, &config) };
    unsafe {
        view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        view.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        if let Some(ua) = &cfg.user_agent {
            view.setCustomUserAgent(Some(&nsstring(ua)));
        }
    }
    let window = unsafe { NSWindow::initWithContentRect_styleMask_backing_defer(NSWindow::alloc(mtm), rect(OFF_SCREEN, OFF_SCREEN, cfg.width, cfg.height), NSWindowStyleMask::Borderless, NSBackingStoreType::Buffered, false) };
    unsafe { window.setReleasedWhenClosed(false) };
    window.setContentView(Some(&view));
    window.orderFrontRegardless();
    let page = Page { window, view, _delegate: delegate, width: cfg.width, height: cfg.height, scale: cfg.scale, snapping: false, last: 0, title: None, finger: None };
    HOST.with_borrow_mut(|h| h.pages.insert(p, page));
}

fn command(mtm: MainThreadMarker, p: PageId, cmd: Command) {
    if let Command::Create(cfg) = cmd {
        return create(mtm, p, cfg);
    }
    if let Command::Decide { id, allow } = cmd {
        let handler = HOST.with_borrow_mut(|h| h.policies.remove(&id));
        if let Some(handler) = handler {
            handler.call((if allow { WKNavigationActionPolicy::Allow } else { WKNavigationActionPolicy::Cancel },));
        }
        return;
    }
    let Some(view) = HOST.with_borrow(|h| h.pages.get(&p).map(|pg| pg.view.clone())) else { return };
    match cmd {
        Command::LoadHtml { nav, html, base } => {
            let base = base.and_then(|b| NSURL::URLWithString(&nsstring(&b)));
            let n = unsafe { view.loadHTMLString_baseURL(&nsstring(&html), base.as_deref()) };
            HOST.with_borrow_mut(|h| h.adopt(n, nav));
        }
        Command::LoadRequest { nav, request } => {
            let Some(url) = NSURL::URLWithString(&nsstring(&request.url)) else {
                return emit(p, Event::Failed { nav, provisional: true, error: WebError::new("NSURLErrorDomain", -1000, "bad URL") });
            };
            if url.isFileURL() {
                // The app's files are the emulator's, not this process's.
                return emit(p, Event::Failed { nav, provisional: true, error: WebError::new("NSURLErrorDomain", -1100, "file URLs aren't supported in pomme's web views yet") });
            }
            let r = NSMutableURLRequest::requestWithURL(&url);
            r.setHTTPMethod(&nsstring(&request.method));
            for (k, v) in &request.headers {
                r.setValue_forHTTPHeaderField(Some(&nsstring(v)), &nsstring(k));
            }
            if !request.body.is_empty() {
                r.setHTTPBody(Some(&NSData::with_bytes(&request.body)));
            }
            let n = unsafe { view.loadRequest(&r) };
            HOST.with_borrow_mut(|h| h.adopt(n, nav));
        }
        Command::Reload { nav } => {
            let n = unsafe { view.reload() };
            HOST.with_borrow_mut(|h| h.adopt(n, nav));
        }
        Command::GoBack { nav } => {
            let n = unsafe { view.goBack() };
            HOST.with_borrow_mut(|h| h.adopt(n, nav));
        }
        Command::GoForward { nav } => {
            let n = unsafe { view.goForward() };
            HOST.with_borrow_mut(|h| h.adopt(n, nav));
        }
        Command::StopLoading => unsafe { view.stopLoading() },
        Command::Evaluate { id, script } => {
            let block = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
                let result = match unsafe { error.as_ref() } {
                    Some(e) => {
                        // WKJavaScriptExceptionMessage, when it was an exception.
                        let info = e.userInfo();
                        let message = info.objectForKey(&nsstring("WKJavaScriptExceptionMessage")).and_then(|m| m.downcast::<NSString>().ok()).map(|m| m.to_string());
                        Err(message.unwrap_or_else(|| e.localizedDescription().to_string()))
                    }
                    None => Ok(to_json(unsafe { result.as_ref() })),
                };
                emit(p, Event::Evaluated { id, result });
            });
            unsafe { view.evaluateJavaScript_completionHandler(&nsstring(&script), Some(&block)) };
        }
        Command::AddUserScript(s) => unsafe { view.configuration().userContentController().addUserScript(&user_script(mtm, &s)) },
        Command::RemoveAllUserScripts => unsafe {
            let c = view.configuration().userContentController();
            c.removeAllUserScripts();
            c.addUserScript(&user_script(mtm, &UserScript { source: CONSOLE_SCRIPT.into(), at_end: false, main_frame_only: false }));
        },
        Command::AddMessageHandler(n) => {
            let Some(delegate) = HOST.with_borrow(|h| h.pages.get(&p).map(|pg| pg._delegate.clone())) else { return };
            unsafe { view.configuration().userContentController().addScriptMessageHandler_name(ProtocolObject::from_ref(&*delegate), &nsstring(&n)) };
        }
        Command::RemoveMessageHandler(n) => unsafe { view.configuration().userContentController().removeScriptMessageHandlerForName(&nsstring(&n)) },
        Command::UserAgent(ua) => unsafe { view.setCustomUserAgent(ua.as_deref().map(nsstring).as_deref()) },
        Command::Resize { width, height, scale } => {
            HOST.with_borrow_mut(|h| {
                if let Some(pg) = h.pages.get_mut(&p) {
                    pg.width = width;
                    pg.height = height;
                    pg.scale = scale;
                    pg.window.setFrame_display(rect(OFF_SCREEN, OFF_SCREEN, width, height), false);
                }
            });
        }
        Command::Touch { phase, x, y } => touch(mtm, p, phase, x, y),
        Command::Close => {
            if let Some(pg) = HOST.with_borrow_mut(|h| h.pages.remove(&p)) {
                pg.window.close();
            }
        }
        // Pages are drawn into the screen from their pixels: where they are doesn't matter here.
        Command::Place { .. } | Command::Create(_) | Command::Decide { .. } => {}
    }
}

/// A finger on the page, as iOS's WebKit takes one: a tap (down and up without
/// moving past [`SLOP`]) is a click there; a drag scrolls what's under it.
fn touch(mtm: MainThreadMarker, p: PageId, phase: u8, x: f64, y: f64) {
    let Some((window, height, finger)) = HOST.with_borrow(|h| h.pages.get(&p).map(|pg| (pg.window.clone(), pg.height, pg.finger))) else { return };
    let set = |f: Option<Finger>| HOST.with_borrow_mut(|h| h.pages.get_mut(&p).map(|pg| pg.finger = f));
    match (phase, finger) {
        (0, _) => {
            set(Some(Finger { start: (x, y), last: (x, y), scrolling: false }));
        }
        (1, Some(mut f)) => {
            if !f.scrolling && ((x - f.start.0).powi(2) + (y - f.start.1).powi(2)).sqrt() > SLOP {
                f.scrolling = true;
            }
            if f.scrolling {
                // Content follows the finger: dragging up scrolls down.
                scroll(&window, height, f.last, (x - f.last.0, y - f.last.1));
            }
            f.last = (x, y);
            set(Some(f));
        }
        (2, Some(f)) if !f.scrolling => {
            set(None);
            click(mtm, &window, height, (x, y));
        }
        _ => {
            set(None);
        }
    }
}

fn click(_mtm: MainThreadMarker, window: &NSWindow, height: f64, (x, y): (f64, f64)) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    // Window coordinates go up from the bottom.
    let at = NSPoint::new(x, height - y);
    let now = objc2_foundation::NSProcessInfo::processInfo().systemUptime();
    for ty in [NSEventType::LeftMouseDown, NSEventType::LeftMouseUp] {
        let ev = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(ty, at, NSEventModifierFlags::empty(), now, window.windowNumber(), None, 0, 1, 1.0);
        if let Some(ev) = ev {
            window.sendEvent(&ev);
        }
    }
}

fn scroll(window: &NSWindow, height: f64, (x, y): (f64, f64), (dx, dy): (f64, f64)) {
    use objc2_core_graphics::{CGEvent, CGScrollEventUnit};
    let Some(ev) = CGEvent::new_scroll_wheel_event2(None, CGScrollEventUnit::Pixel, 2, dy.round() as i32, dx.round() as i32, 0) else { return };
    // An event made here belongs to no window, and AppKit takes its location as
    // already in the window it's sent to: the point in the page (in global display
    // coordinates, which go down from the top of the main display; the window's go up).
    let screens = objc2_app_kit::NSScreen::screens(MainThreadMarker::new().expect("main thread"));
    let top = screens.firstObject().map(|s| s.frame().size.height).unwrap_or(0.0);
    CGEvent::set_location(Some(&ev), CGPoint::new(x, top - (height - y)));
    if let Some(ev) = objc2_app_kit::NSEvent::eventWithCGEvent(&ev) {
        window.sendEvent(&ev);
    }
}

/// Snapshot every page that isn't already being snapshotted; send pixels that changed.
fn snapshot_all(mtm: MainThreadMarker) {
    let pages: Vec<(PageId, Retained<WKWebView>, f64, f64, f64, f64)> = HOST.with_borrow_mut(|h| {
        h.pages
            .iter_mut()
            .filter(|(_, pg)| !pg.snapping)
            .map(|(&p, pg)| {
                pg.snapping = true;
                (p, pg.view.clone(), pg.width, pg.height, pg.scale, pg.window.backingScaleFactor())
            })
            .collect()
    });
    for (p, view, width, height, scale, backing) in pages {
        // The title, as WebKit's KVO would tell it.
        let title = unsafe { view.title() }.map(|t| t.to_string());
        let changed = HOST.with_borrow_mut(|h| h.pages.get_mut(&p).is_some_and(|pg| std::mem::replace(&mut pg.title, title.clone()) != title));
        if changed {
            emit(p, Event::Title(title));
        }
        let config = unsafe { WKSnapshotConfiguration::new(mtm) };
        // The snapshot comes at the window's backing scale: ask for the phone's.
        unsafe { config.setSnapshotWidth(Some(&NSNumber::new_f64(width * scale / backing.max(1.0)))) };
        let (pw, ph) = ((width * scale).round().max(1.0) as usize, (height * scale).round().max(1.0) as usize);
        let block = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
            let pixels = unsafe { image.as_ref() }.and_then(|img| rgba(img, pw, ph));
            HOST.with_borrow_mut(|h| {
                let Some(pg) = h.pages.get_mut(&p) else { return };
                pg.snapping = false;
                let Some(rgba) = pixels else { return };
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                rgba.hash(&mut hasher);
                let hash = hasher.finish();
                if hash != pg.last {
                    pg.last = hash;
                    emit(p, Event::Frame { width: pw as u32, height: ph as u32, rgba });
                }
            });
        });
        unsafe { view.takeSnapshotWithConfiguration_completionHandler(Some(&config), &block) };
    }
}

/// An image's pixels at exactly `w` × `h`: RGBA, not premultiplied, rows top to bottom.
fn rgba(image: &NSImage, w: usize, h: usize) -> Option<Vec<u8>> {
    let cg = unsafe { image.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) }?;
    let mut buf = vec![0u8; w * h * 4];
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | objc2_core_graphics::CGImageByteOrderInfo::Order32Big.0;
    let ctx = unsafe { CGBitmapContextCreate(buf.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&space), info) }?;
    CGContext::draw_image(Some(&ctx), CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64)), Some(&cg));
    drop(ctx);
    for px in buf.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(buf)
}

pub fn run() -> std::process::ExitCode {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("pomme-webview must run on the main thread");
        return std::process::ExitCode::FAILURE;
    };
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no menu bar: a background helper.
    app.setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
    // Commands arrive on a reader thread and are handled on the main thread, in order.
    std::thread::spawn(|| {
        let mut input = std::io::BufReader::new(std::io::stdin());
        while let Ok(Some(((page, cmd), _))) = crate::read_message::<_, (PageId, Command)>(&mut input) {
            dispatch2::DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main thread");
                command(mtm, page, cmd);
            });
        }
        // The emulator is gone.
        std::process::exit(0);
    });
    let tick = RcBlock::new(move |_| snapshot_all(MainThreadMarker::new().expect("main thread")));
    let _timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / FPS, true, &tick) };
    app.run();
    std::process::ExitCode::SUCCESS
}
