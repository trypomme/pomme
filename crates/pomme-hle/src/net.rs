//! Networking: NSURLRequest, NSURLSession data tasks, NSHTTPURLResponse.
//!
//! Every request goes through one host interface, [`Net`], like every other
//! source of non-determinism. The front end decides what that means: real
//! HTTP (the CLI), synchronous XHR (the browser worker), or canned
//! [`Fixtures`] (tests). With no `Net` the device is offline.
//!
//! A task's request is made when it resumes; the response is delivered on
//! the main run loop a little later in virtual time ([`LATENCY`]), through
//! the session delegate (`didReceiveResponse`, `didReceiveData`,
//! `didCompleteWithError`) or the task's completion handler. So a run with
//! the same responses is the same run.
//!
//! One exception, as on iOS: a completion handler on a session with no
//! delegate queue of its own (the shared session, or `delegateQueue:nil`)
//! runs on a background queue, at once. Code that waits on the main thread
//! for the answer (a semaphore, as React Native's packager check does) gets
//! it, instead of waiting for a main run loop that can't turn.

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// Virtual time between a task resuming and its response arriving.
pub const LATENCY_S: f64 = 0.05;

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// A connection the host holds for the app's socket (see [`crate::sockets`]).
pub type SocketId = u64;

/// What a connection can do now, for poll and select.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readiness {
    /// Data is waiting (or the peer closed: a read returns at once).
    pub readable: bool,
    pub writable: bool,
    /// The peer closed or the connection failed.
    pub closed: bool,
}

/// A Bonjour service found on the network.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    /// Its instance name ("hello-expo").
    pub name: String,
    /// "_expo._tcp".
    pub kind: String,
    /// "local."
    pub domain: String,
    pub host: String,
    pub port: u16,
    pub addresses: Vec<std::net::IpAddr>,
    /// Its TXT record, in order: key, and value (None: a key with no `=`).
    pub txt: Vec<(String, Option<Vec<u8>>)>,
}

pub trait Net {
    /// Perform a request. Err: the request failed before any response (no network, bad host, …).
    fn fetch(&mut self, req: &Request) -> std::result::Result<Response, String>;

    // Sockets (BSD sockets for apps that speak to servers themselves: gRPC,
    // WebSockets, ...). By default there's no network for them: names don't
    // resolve and connections don't open, as on a phone offline.

    /// A host name's addresses.
    fn resolve(&mut self, _host: &str) -> std::result::Result<Vec<std::net::IpAddr>, String> {
        Err("offline".into())
    }
    /// Open a TCP connection (it then doesn't block: reads and writes may say WouldBlock).
    fn connect(&mut self, _addr: std::net::SocketAddr) -> std::io::Result<SocketId> {
        Err(std::io::ErrorKind::NetworkUnreachable.into())
    }
    fn send(&mut self, _s: SocketId, _data: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::NotConnected.into())
    }
    /// Up to `max` bytes: WouldBlock when none have come yet, empty when the peer closed.
    fn recv(&mut self, _s: SocketId, _max: usize) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotConnected.into())
    }
    fn readiness(&mut self, _s: SocketId) -> Readiness {
        Readiness { closed: true, ..Default::default() }
    }
    /// Wait (real time) until one of `sockets` is readable or `timeout` passes.
    fn wait(&mut self, _sockets: &[SocketId], _timeout: std::time::Duration) {}
    fn shutdown(&mut self, _s: SocketId, _how: std::net::Shutdown) {}
    fn close(&mut self, _s: SocketId) {}
    fn local_addr(&mut self, _s: SocketId) -> Option<std::net::SocketAddr> {
        None
    }
    fn peer_addr(&mut self, _s: SocketId) -> Option<std::net::SocketAddr> {
        None
    }
    /// Bonjour services of `kind` (`_expo._tcp`) on the computer's network,
    /// as found so far: a phone on the same Wi-Fi would see the same ones.
    /// The first call starts looking; none without a live network.
    fn browse(&mut self, _kind: &str) -> Vec<Service> {
        Vec::new()
    }
}

/// The host's own TCP, for a [`Net`] that gives apps real sockets: DNS
/// through the host's resolver, connections that don't block once open.
#[derive(Default)]
pub struct HostSockets {
    streams: HashMap<SocketId, std::net::TcpStream>,
    next: SocketId,
}

/// How long opening a connection may take before it's ETIMEDOUT.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

impl HostSockets {
    pub fn resolve(&mut self, host: &str) -> std::result::Result<Vec<std::net::IpAddr>, String> {
        use std::net::ToSocketAddrs;
        let addrs = (host, 0).to_socket_addrs().map_err(|e| e.to_string())?;
        Ok(addrs.map(|a| a.ip()).collect())
    }

    pub fn connect(&mut self, addr: std::net::SocketAddr) -> std::io::Result<SocketId> {
        let s = std::net::TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        s.set_nonblocking(true)?;
        self.next += 1;
        self.streams.insert(self.next, s);
        Ok(self.next)
    }

    fn stream(&mut self, s: SocketId) -> std::io::Result<&mut std::net::TcpStream> {
        self.streams.get_mut(&s).ok_or_else(|| std::io::ErrorKind::NotConnected.into())
    }

    pub fn send(&mut self, s: SocketId, data: &[u8]) -> std::io::Result<usize> {
        use std::io::Write;
        self.stream(s)?.write(data)
    }

    pub fn recv(&mut self, s: SocketId, max: usize) -> std::io::Result<Vec<u8>> {
        use std::io::Read;
        let mut buf = vec![0u8; max.max(1)];
        let n = self.stream(s)?.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    pub fn readiness(&mut self, s: SocketId) -> Readiness {
        let Ok(stream) = self.stream(s) else { return Readiness { closed: true, ..Default::default() } };
        let mut one = [0u8; 1];
        match stream.peek(&mut one) {
            Ok(0) => Readiness { readable: true, writable: false, closed: true },
            Ok(_) => Readiness { readable: true, writable: true, closed: false },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Readiness { readable: false, writable: true, closed: false },
            Err(_) => Readiness { readable: true, writable: false, closed: true },
        }
    }

    pub fn wait(&mut self, sockets: &[SocketId], timeout: std::time::Duration) {
        let start = std::time::Instant::now();
        loop {
            if sockets.iter().any(|&s| self.readiness(s).readable) || start.elapsed() >= timeout {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1).min(timeout));
        }
    }

    pub fn shutdown(&mut self, s: SocketId, how: std::net::Shutdown) {
        if let Ok(stream) = self.stream(s) {
            let _ = stream.shutdown(how);
        }
    }

    pub fn close(&mut self, s: SocketId) {
        self.streams.remove(&s);
    }

    pub fn local_addr(&mut self, s: SocketId) -> Option<std::net::SocketAddr> {
        self.stream(s).ok()?.local_addr().ok()
    }

    pub fn peer_addr(&mut self, s: SocketId) -> Option<std::net::SocketAddr> {
        self.stream(s).ok()?.peer_addr().ok()
    }
}

/// Canned responses by URL (exact match, or prefix ending in `*`). Unknown URLs get a 404.
#[derive(Default, Clone)]
pub struct Fixtures(pub Vec<(String, Response)>);

impl Fixtures {
    pub fn json(mut self, url: &str, body: &str) -> Self {
        let headers = vec![("Content-Type".into(), "application/json; charset=utf-8".into())];
        self.0.push((url.into(), Response { status: 200, headers, body: body.as_bytes().to_vec() }));
        self
    }
    /// A file's bytes, as `content_type`.
    pub fn file(mut self, url: &str, content_type: &str, body: Vec<u8>) -> Self {
        let headers = vec![("Content-Type".into(), content_type.into()), ("Content-Length".into(), body.len().to_string())];
        self.0.push((url.into(), Response { status: 200, headers, body }));
        self
    }
    pub fn lookup(&self, url: &str) -> Option<&Response> {
        self.0.iter().find(|(u, _)| u == url || u.strip_suffix('*').is_some_and(|p| url.starts_with(p))).map(|(_, r)| r)
    }
}

impl Net for Fixtures {
    fn fetch(&mut self, req: &Request) -> std::result::Result<Response, String> {
        Ok(self.lookup(&req.url).cloned().unwrap_or(Response { status: 404, headers: vec![], body: b"not found".to_vec() }))
    }
}

#[derive(Clone, Debug, Default)]
struct UrlRequest {
    url: u64,
    method: String,
    headers: Vec<(String, String)>,
    body: u64,
    timeout: f64,
}

#[derive(Clone, Debug, Default)]
struct UrlResponse {
    url: u64,
    status: u16,
    headers: Vec<(String, String)>,
    /// A plain NSURLResponse (a file URL's), not an HTTP one.
    plain: bool,
}

#[derive(Clone, Debug, Default)]
struct Task {
    id: u64,
    session: u64,
    request: u64,
    completion: u64,
    /// NSURLSessionTaskState: running 0, suspended 1, canceling 2, completed 3.
    state: u64,
    response: u64,
    data: u64,
    error: u64,
    received: u64,
    /// Delivery progress: 0 response, 1 data, 2 completion.
    stage: u8,
    /// A download task: the body arrives as a temporary file.
    download: bool,
    result: Option<std::result::Result<Response, String>>,
}

#[derive(Clone, Default)]
pub struct State {
    pub client: crate::External<Box<dyn Net>>,
    /// Print each request and how it went (`--log-requests`).
    pub log_requests: bool,
    requests: HashMap<u64, UrlRequest>,
    responses: HashMap<u64, UrlResponse>,
    /// session → (delegate, configuration)
    sessions: HashMap<u64, (u64, u64)>,
    /// Sessions given a delegate queue of their own (`delegateQueue:` not nil).
    own_queue: std::collections::HashSet<u64>,
    shared: u64,
    tasks: HashMap<u64, Task>,
    next_id: u64,
    /// The block passed as `completionHandler:` to delegate methods that need one.
    allow_block: u64,
    /// The disposition the delegate last gave that block (didReceiveResponse).
    disposition: Option<u64>,
    /// Stream tasks a data task became: stream task → (session, delegate, body data).
    stream_tasks: HashMap<u64, (u64, u64, u64)>,
    /// Stream tasks whose streams were asked for (`captureStreams`), not yet handed over.
    captured: std::collections::HashSet<u64>,
    /// NSURLProtocol properties: (request, key) → value.
    properties: HashMap<(u64, String), u64>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn string(emu: &mut Emu, s: &str) -> Result<u64> {
    foundation::new_nsstring(emu, s)
}

fn url_string(emu: &mut Emu, url: u64) -> Result<String> {
    if url == 0 {
        return Ok(String::new());
    }
    let s = send(emu, url, "absoluteString", &[])?;
    foundation::nsstring(emu, s)
}

/// A response's headers as NSHTTPURLResponse has them: each name in canonical
/// form (`content-type` → `Content-Type`, as HTTP/2 sends them lowercase), and
/// repeated headers joined with ", ".
fn canonical_headers(headers: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in headers {
        let name = k
            .split('-')
            .map(|w| {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect())
            })
            .collect::<Vec<_>>()
            .join("-");
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some((_, existing)) => {
                existing.push_str(", ");
                existing.push_str(&v);
            }
            None => out.push((name, v)),
        }
    }
    out
}

fn headers_dict(emu: &mut Emu, headers: &[(String, String)]) -> Result<u64> {
    let mut pairs = Vec::new();
    for (k, v) in headers {
        pairs.push((string(emu, k)?, string(emu, v)?));
    }
    foundation::new_dict_of(emu, &pairs)
}

fn dict_strings(emu: &mut Emu, d: u64) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    if d == 0 {
        return Ok(out);
    }
    let d = foundation::as_cf_dict(emu, d)?;
    for (k, v) in foundation::dict_pairs(emu, d)? {
        let v = if foundation::is_cf(emu, v) || v != 0 { send(emu, v, "description", &[])? } else { v };
        out.push((foundation::nsstring(emu, k)?, foundation::nsstring(emu, v)?));
    }
    Ok(out)
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: Option<String>, append: bool) {
    let pos = headers.iter().position(|(k, _)| k.eq_ignore_ascii_case(name));
    match (pos, value) {
        (Some(i), Some(v)) if append => headers[i].1 = format!("{},{v}", headers[i].1),
        (Some(i), Some(v)) => headers[i].1 = v,
        (Some(i), None) => {
            headers.remove(i);
        }
        (None, Some(v)) => headers.push((name.to_string(), v)),
        (None, None) => {}
    }
}

// ---- NSURLRequest ----

fn req(emu: &Emu, r: u64) -> UrlRequest {
    emu.net.requests.get(&r).cloned().unwrap_or_default()
}

fn new_request(emu: &mut Emu, class: &str, state: UrlRequest) -> Result<u64> {
    let r = new_object(emu, class)?;
    emu.net.requests.insert(r, state);
    Ok(r)
}

fn default_request(url: u64) -> UrlRequest {
    UrlRequest { url, method: "GET".into(), headers: Vec::new(), body: 0, timeout: 60.0 }
}

/// `+requestWithURL:` (and the variants with cache policy and timeout).
fn request_with_url(emu: &mut Emu) -> Result<()> {
    let (cls, url) = (emu.arg(0), emu.arg(2));
    let name = emu.objc.class_name(cls);
    let r = new_request(emu, if name.contains("Mutable") { "NSMutableURLRequest" } else { "NSURLRequest" }, default_request(url))?;
    ret(emu, r)
}

/// `-initWithURL:` on an allocated request (possibly a subclass).
fn request_init(emu: &mut Emu) -> Result<()> {
    let (r, url) = (emu.arg(0), emu.arg(2));
    emu.net.requests.insert(r, default_request(url));
    ret(emu, r)
}

fn request_copy(emu: &mut Emu, mutable: bool) -> Result<()> {
    let state = req(emu, emu.arg(0));
    let r = new_request(emu, if mutable { "NSMutableURLRequest" } else { "NSURLRequest" }, state)?;
    ret(emu, r)
}

fn request_header(emu: &mut Emu) -> Result<()> {
    let (r, name) = (emu.arg(0), emu.arg(2));
    let name = foundation::nsstring(emu, name)?;
    let v = req(emu, r).headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name)).map(|(_, v)| v.clone());
    let s = match v {
        Some(v) => string(emu, &v)?,
        None => 0,
    };
    ret(emu, s)
}

fn request_set_header(emu: &mut Emu, append: bool) -> Result<()> {
    let (r, value, name) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let name = foundation::nsstring(emu, name)?;
    let value = if value == 0 { None } else { Some(foundation::nsstring(emu, value)?) };
    let st = emu.net.requests.entry(r).or_default();
    set_header(&mut st.headers, &name, value, append);
    Ok(())
}

fn request_headers(emu: &mut Emu) -> Result<()> {
    let h = req(emu, emu.arg(0)).headers;
    let d = headers_dict(emu, &h)?;
    ret(emu, d)
}

fn request_set_headers(emu: &mut Emu) -> Result<()> {
    let (r, d) = (emu.arg(0), emu.arg(2));
    let h = dict_strings(emu, d)?;
    emu.net.requests.entry(r).or_default().headers = h;
    Ok(())
}

fn request_method(emu: &mut Emu) -> Result<()> {
    let m = req(emu, emu.arg(0)).method;
    let s = string(emu, if m.is_empty() { "GET" } else { &m })?;
    ret(emu, s)
}

fn request_set_method(emu: &mut Emu) -> Result<()> {
    let (r, m) = (emu.arg(0), emu.arg(2));
    let m = foundation::nsstring(emu, m)?;
    emu.net.requests.entry(r).or_default().method = m.to_ascii_uppercase();
    Ok(())
}

// ---- NSHTTPURLResponse ----

fn resp(emu: &Emu, r: u64) -> UrlResponse {
    emu.net.responses.get(&r).cloned().unwrap_or_default()
}

fn new_response(emu: &mut Emu, state: UrlResponse) -> Result<u64> {
    let r = new_object(emu, if state.plain { "NSURLResponse" } else { "NSHTTPURLResponse" })?;
    emu.net.responses.insert(r, state);
    Ok(r)
}

fn response_init(emu: &mut Emu) -> Result<()> {
    let (r, url, status, headers) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(5));
    let headers = dict_strings(emu, headers)?;
    emu.net.responses.insert(r, UrlResponse { url, status: status as u16, headers, plain: false });
    ret(emu, r)
}

fn response_header(emu: &Emu, r: u64, name: &str) -> Option<String> {
    resp(emu, r).headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
}

/// (MIME type, charset) from Content-Type.
fn content_type(emu: &Emu, r: u64) -> (Option<String>, Option<String>) {
    let Some(ct) = response_header(emu, r, "Content-Type") else { return (None, None) };
    let mut parts = ct.split(';').map(str::trim);
    let mime = parts.next().filter(|s| !s.is_empty()).map(str::to_ascii_lowercase);
    let charset = parts.find_map(|p| p.strip_prefix("charset=").map(|c| c.trim_matches('"').to_ascii_lowercase()));
    (mime, charset)
}

fn opt_string(emu: &mut Emu, s: Option<String>) -> Result<()> {
    let v = match s {
        Some(s) => string(emu, &s)?,
        None => 0,
    };
    ret(emu, v)
}

// ---- NSURLSession ----

fn new_session(emu: &mut Emu, config: u64, delegate: u64, queue: u64) -> Result<u64> {
    let s = new_object(emu, "NSURLSession")?;
    if delegate != 0 {
        send(emu, delegate, "retain", &[])?;
    }
    emu.net.sessions.insert(s, (delegate, config));
    if queue != 0 {
        emu.net.own_queue.insert(s);
    }
    Ok(s)
}

fn shared_session(emu: &mut Emu) -> Result<()> {
    if emu.net.shared == 0 {
        emu.net.shared = new_session(emu, 0, 0, 0)?;
    }
    ret(emu, emu.net.shared)
}

use crate::libc::copy_block;

fn new_task(emu: &mut Emu, session: u64, request: u64, completion: u64) -> Result<u64> {
    let t = new_object(emu, "NSURLSessionDataTask")?;
    emu.net.next_id += 1;
    let completion = if completion != 0 { copy_block(emu, completion)? } else { 0 };
    emu.net.tasks.insert(t, Task { id: emu.net.next_id, session, request, completion, state: 1, ..Default::default() });
    Ok(t)
}

fn data_task(emu: &mut Emu, completion: bool) -> Result<()> {
    let (s, r) = (emu.arg(0), emu.arg(2));
    let block = if completion { emu.arg(3) } else { 0 };
    let t = new_task(emu, s, r, block)?;
    ret(emu, t)
}

/// `downloadTaskWithRequest:` (and `…completionHandler:`, and the URL forms).
fn download_task(emu: &mut Emu, with_url: bool, completion: bool) -> Result<()> {
    let (s, r) = (emu.arg(0), emu.arg(2));
    let block = if completion { emu.arg(3) } else { 0 };
    let r = if with_url { new_request(emu, "NSURLRequest", default_request(r))? } else { r };
    let t = new_task(emu, s, r, block)?;
    if let Some(x) = emu.net.tasks.get_mut(&t) {
        x.download = true;
    }
    ret(emu, t)
}

/// `uploadTaskWithRequest:fromFile:` (and `…completionHandler:`): the file is the body.
fn upload_from_file(emu: &mut Emu, completion: bool) -> Result<()> {
    let (r, file) = (emu.arg(2), emu.arg(3));
    let path = url_string(emu, file)?;
    let path = percent_decode(path.strip_prefix("file://").unwrap_or(&path));
    let bytes = crate::fs::read_file(emu, &path).unwrap_or_default();
    let d = foundation::new_data(emu, &bytes)?;
    emu.net.requests.entry(r).or_default().body = d;
    if completion {
        emu.cpu.x[3] = emu.arg(4);
    }
    data_task(emu, completion)
}

/// Where a download task's body goes until its handler has seen it.
fn download_path(id: u64) -> String {
    format!("{}/tmp/CFNetworkDownload_{id}.tmp", foundation::DATA_CONTAINER)
}

fn data_task_with_url(emu: &mut Emu, completion: bool) -> Result<()> {
    let (s, url) = (emu.arg(0), emu.arg(2));
    let block = if completion { emu.arg(3) } else { 0 };
    let r = new_request(emu, "NSURLRequest", default_request(url))?;
    let t = new_task(emu, s, r, block)?;
    ret(emu, t)
}

// ---- running tasks ----

fn task(emu: &Emu, t: u64) -> Task {
    emu.net.tasks.get(&t).cloned().unwrap_or_default()
}

/// `-resume`: make the request now; deliver the result after [`LATENCY_S`].
fn resume(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    let tk = task(emu, t);
    if tk.state != 1 {
        return Ok(());
    }
    let r = req(emu, tk.request);
    let mut request = Request { method: if r.method.is_empty() { "GET".into() } else { r.method.clone() }, url: url_string(emu, r.url)?, headers: r.headers.clone(), body: Vec::new() };
    if r.body != 0 {
        request.body = foundation::data_bytes(emu, r.body)?;
    }
    let result = if let Some(path) = request.url.strip_prefix("file://") {
        read_file_url(emu, path)
    } else if emu.network.path == crate::network::Path::Offline {
        // The phone has no network (--network offline): nothing goes out.
        Err("offline".to_string())
    } else {
        match emu.net.client.as_mut() {
            Some(c) => c.fetch(&request),
            None => Err("offline".to_string()),
        }
    };
    if emu.trace || emu.net.log_requests {
        println!("[net] {} {} → {}", request.method, request.url, match &result {
            Ok(r) => format!("{} ({} bytes)", r.status, r.body.len()),
            Err(e) => e.clone(),
        });
    }
    if let Some(x) = emu.net.tasks.get_mut(&t) {
        x.state = 0;
        x.result = Some(result);
    }
    send(emu, t, "retain", &[])?;
    let tk = task(emu, t);
    if tk.completion != 0 && !emu.net.own_queue.contains(&tk.session) {
        // A completion handler with no queue of the session's own: a background queue, now.
        let q = crate::libsystem::global_queue(emu)?;
        return crate::libsystem::submit_host(emu, q, deliver_from_queue, t);
    }
    crate::uikit::after_on_main(emu, LATENCY_S, deliver, t)
}

/// [`deliver`], run as dispatch work (its context is the task).
fn deliver_from_queue(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    deliver_task(emu, t)
}

/// A file URL, read as the URL loading system does on iOS: the file's bytes
/// with a plain (not HTTP) response, its MIME type from the extension; a
/// missing file is NSURLErrorFileDoesNotExist.
fn read_file_url(emu: &mut Emu, path: &str) -> std::result::Result<Response, String> {
    let path = percent_decode(path);
    let Some(body) = crate::fs::read_file(emu, &path) else { return Err("no such file".into()) };
    let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let mime = crate::uttype::for_tag("public.filename-extension", ext, None)
        .and_then(crate::uttype::mime_of)
        .unwrap_or("application/octet-stream");
    let headers = vec![("Content-Type".into(), mime.to_string()), ("Content-Length".into(), body.len().to_string())];
    Ok(Response { status: 0, headers, body })
}

/// Undo `%XX` escapes (a `%` not followed by two hex digits stays as it is).
pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
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

fn cancel(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    if let Some(x) = emu.net.tasks.get_mut(&t)
        && x.state < 2
    {
        // Deliver a cancellation error instead of whatever comes back.
        x.result = Some(Err("cancelled".into()));
        if x.state == 1 {
            x.state = 0;
            send(emu, t, "retain", &[])?;
            return crate::uikit::after_on_main(emu, 0.0, deliver, t);
        }
    }
    Ok(())
}

fn url_error(emu: &mut Emu, what: &str, url: u64) -> Result<u64> {
    // NSURLErrorCancelled −999, NSURLErrorNotConnectedToInternet −1009, NSURLErrorCannotFindHost −1003
    let code: i64 = match what {
        "cancelled" => -999,
        "offline" => -1009,
        "no such file" => -1100, // NSURLErrorFileDoesNotExist
        _ => -1003,
    };
    let desc = match what {
        "cancelled" => "cancelled".to_string(),
        "offline" => "The Internet connection appears to be offline.".to_string(),
        other => other.to_string(),
    };
    let (k1, v1) = (string(emu, "NSLocalizedDescription")?, string(emu, &desc)?);
    let k2 = string(emu, "NSErrorFailingURLKey")?;
    let info = if url != 0 { foundation::new_dict_of(emu, &[(k1, v1), (k2, url)])? } else { foundation::new_dict_of(emu, &[(k1, v1)])? };
    let domain = string(emu, "NSURLErrorDomain")?;
    let cls = host_class(emu, "NSError")?;
    send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, code as u64, info])
}

fn allow_block(emu: &mut Emu) -> Result<u64> {
    if emu.net.allow_block == 0 {
        // (Its argument is the disposition: allow, cancel, become a download or a stream.)
        emu.net.allow_block = crate::libsystem::host_block(emu, |e| {
            e.net.disposition = Some(e.arg(1));
            Ok(())
        })?;
    }
    Ok(emu.net.allow_block)
}

/// `NSURLSessionResponseBecomeStream`.
const BECOME_STREAM: u64 = 3;

/// The delegate asked for the response as a stream (React Native does, for
/// Metro's multipart bundle): the data task becomes a stream task
/// (`didBecomeStreamTask:`); when the delegate captures its streams, it gets
/// an input stream over the response's body (`didBecomeInputStream:outputStream:`).
/// Then the data task completes.
fn become_stream(emu: &mut Emu, t: u64, s: u64, delegate: u64) -> Result<()> {
    let tk = task(emu, t);
    let st = new_object(emu, "NSURLSessionStreamTask")?;
    emu.net.stream_tasks.insert(st, (s, delegate, tk.data));
    delegate_call(emu, delegate, "URLSession:dataTask:didBecomeStreamTask:", &[s, t, st])?;
    if emu.net.captured.remove(&st) {
        let bytes = if tk.data == 0 { Vec::new() } else { foundation::data_bytes(emu, tk.data)? };
        let input = crate::streams::input_stream_with_bytes(emu, &bytes)?;
        let output = foundation::cf(emu, "_CFWriteStreamCreateWithAllocatedBuffers", &[0, 0])?;
        delegate_call(emu, delegate, "URLSession:streamTask:didBecomeInputStream:outputStream:", &[s, st, input, output])?;
    }
    if let Some(x) = emu.net.tasks.get_mut(&t) {
        x.state = 3;
    }
    delegate_call(emu, delegate, "URLSession:task:didCompleteWithError:", &[s, t, 0])?;
    send(emu, t, "release", &[])
        .map(|_| ())
}

fn delegate_call(emu: &mut Emu, delegate: u64, sel: &str, args: &[u64]) -> Result<()> {
    let s = objc::intern(emu, sel)?;
    if delegate != 0 && objc::responds(emu, delegate, s)? {
        send(emu, delegate, sel, args)?;
    }
    Ok(())
}

/// Time between a response's delegate callbacks (response, data, completion), as
/// on a device: apps' delegates hop each one onto their own queues and expect the
/// previous one to have settled.
const CALLBACK_GAP_S: f64 = 0.001;

/// Timer callout: hand the task's result over, one step per run-loop turn:
/// the response, then the data, then completion (or the completion handler, at once).
fn deliver(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(1);
    deliver_task(emu, t)
}

fn deliver_task(emu: &mut Emu, t: u64) -> Result<()> {
    let tk = task(emu, t);
    if tk.state >= 3 {
        return Ok(());
    }
    let url = req(emu, tk.request).url;
    let (delegate, _) = emu.net.sessions.get(&tk.session).copied().unwrap_or_default();
    // First step: make the response objects.
    if tk.stage == 0 {
        let (response, data, error) = match tk.result.clone().unwrap_or(Err("offline".into())) {
            Ok(r) => {
                // A file URL's response is a plain NSURLResponse (status 0 marks it).
                let plain = r.status == 0;
                let response = new_response(emu, UrlResponse { url, status: r.status, headers: canonical_headers(r.headers), plain })?;
                let data = foundation::new_data(emu, &r.body)?;
                (response, data, 0)
            }
            Err(e) => (0, 0, url_error(emu, &e, url)?),
        };
        let received = if data != 0 { foundation::data_bytes(emu, data)?.len() as u64 } else { 0 };
        if let Some(x) = emu.net.tasks.get_mut(&t) {
            x.response = response;
            x.data = data;
            x.error = error;
            x.received = received;
        }
        if tk.completion != 0 {
            if let Some(x) = emu.net.tasks.get_mut(&t) {
                x.state = 3;
            }
            if tk.download {
                // The body as a file, removed once the handler returns.
                let location = if error == 0 { download_file(emu, tk.id, data)? } else { 0 };
                crate::libsystem::call_block(emu, tk.completion, &[location, response, error])?;
                crate::fs::remove(emu, &download_path(tk.id));
            } else {
                crate::libsystem::call_block(emu, tk.completion, &[data, response, error])?;
            }
            send(emu, t, "release", &[])?;
            return Ok(());
        }
    }
    let tk = task(emu, t);
    let s = tk.session;
    // Stages: 0 response, 1 data, 2 completion.
    let mut stage = tk.stage;
    if stage == 0 && tk.response == 0 {
        stage = 2; // failed before any response
    }
    match stage {
        // A download: the file, then completion.
        0 | 1 if tk.download => {
            let location = download_file(emu, tk.id, tk.data)?;
            delegate_call(emu, delegate, "URLSession:downloadTask:didFinishDownloadingToURL:", &[s, t, location])?;
            crate::fs::remove(emu, &download_path(tk.id));
            stage = 2;
        }
        0 => {
            let allow = allow_block(emu)?;
            emu.net.disposition = None;
            delegate_call(emu, delegate, "URLSession:dataTask:didReceiveResponse:completionHandler:", &[s, t, tk.response, allow])?;
            if emu.net.disposition == Some(BECOME_STREAM) {
                return become_stream(emu, t, s, delegate);
            }
            stage = if tk.received > 0 { 1 } else { 2 };
        }
        1 => {
            delegate_call(emu, delegate, "URLSession:dataTask:didReceiveData:", &[s, t, tk.data])?;
            stage = 2;
        }
        _ => {
            if let Some(x) = emu.net.tasks.get_mut(&t) {
                x.state = 3;
            }
            delegate_call(emu, delegate, "URLSession:task:didCompleteWithError:", &[s, t, tk.error])?;
            send(emu, t, "release", &[])?;
            return Ok(());
        }
    }
    if let Some(x) = emu.net.tasks.get_mut(&t) {
        x.stage = stage;
    }
    crate::uikit::after_on_main(emu, CALLBACK_GAP_S, deliver, t)
}

/// Write a download's body to its temporary file: the file URL.
fn download_file(emu: &mut Emu, id: u64, data: u64) -> Result<u64> {
    let bytes = if data == 0 { Vec::new() } else { foundation::data_bytes(emu, data)? };
    let path = download_path(id);
    crate::fs::write_data_file(emu, &path, &bytes);
    foundation::new_file_url(emu, &path, false)
}

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

/// `+[NSURLProtocol setProperty:forKey:inRequest:]` (and remove/get).
fn protocol_property(emu: &mut Emu, op: u8) -> Result<()> {
    let (a, b, c) = (emu.arg(2), emu.arg(3), emu.arg(4));
    match op {
        0 => {
            let key = foundation::nsstring(emu, b)?;
            if a != 0 {
                send(emu, a, "retain", &[])?;
            }
            emu.net.properties.insert((c, key), a);
            Ok(())
        }
        1 => {
            let key = foundation::nsstring(emu, a)?;
            let v = emu.net.properties.get(&(b, key)).copied().unwrap_or(0);
            ret(emu, v)
        }
        _ => {
            let key = foundation::nsstring(emu, a)?;
            emu.net.properties.remove(&(b, key));
            Ok(())
        }
    }
}

const RQ: &str = "NSURLRequest";
const MRQ: &str = "NSMutableURLRequest";
const RS: &str = "NSHTTPURLResponse";
const S: &str = "NSURLSession";
const T: &str = "NSURLSessionTask";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSURLProtocol", true, "setProperty:forKey:inRequest:", |e| protocol_property(e, 0)),
    ("NSURLProtocol", true, "propertyForKey:inRequest:", |e| protocol_property(e, 1)),
    ("NSURLProtocol", true, "removePropertyForKey:inRequest:", |e| protocol_property(e, 2)),
    (RQ, true, "requestWithURL:", request_with_url),
    (RQ, true, "requestWithURL:cachePolicy:timeoutInterval:", request_with_url),
    (RQ, false, "initWithURL:", request_init),
    (RQ, false, "initWithURL:cachePolicy:timeoutInterval:", request_init),
    (RQ, false, "URL", |e| {
        let u = req(e, e.arg(0)).url;
        ret(e, u)
    }),
    (RQ, false, "HTTPMethod", request_method),
    (RQ, false, "allHTTPHeaderFields", request_headers),
    (RQ, false, "valueForHTTPHeaderField:", request_header),
    (RQ, false, "HTTPBody", |e| {
        let b = req(e, e.arg(0)).body;
        ret(e, b)
    }),
    (RQ, false, "HTTPBodyStream", |e| ret(e, 0)),
    (RQ, false, "timeoutInterval", |e| {
        let t = req(e, e.arg(0)).timeout;
        crate::uikit::ret_f64(e, t);
        Ok(())
    }),
    (RQ, false, "HTTPShouldHandleCookies", |e| ret(e, 1)),
    (RQ, false, "copy", |e| request_copy(e, false)),
    (RQ, false, "copyWithZone:", |e| request_copy(e, false)),
    (RQ, false, "mutableCopy", |e| request_copy(e, true)),
    (RQ, false, "mutableCopyWithZone:", |e| request_copy(e, true)),
    (MRQ, false, "setURL:", |e| {
        let (r, u) = (e.arg(0), e.arg(2));
        e.net.requests.entry(r).or_default().url = u;
        Ok(())
    }),
    (MRQ, false, "setHTTPMethod:", request_set_method),
    (MRQ, false, "setAllHTTPHeaderFields:", request_set_headers),
    (MRQ, false, "setValue:forHTTPHeaderField:", |e| request_set_header(e, false)),
    (MRQ, false, "addValue:forHTTPHeaderField:", |e| request_set_header(e, true)),
    (MRQ, false, "setHTTPBody:", |e| {
        let (r, b) = (e.arg(0), e.arg(2));
        let b = if b != 0 { send(e, b, "copy", &[])? } else { 0 };
        e.net.requests.entry(r).or_default().body = b;
        Ok(())
    }),
    (MRQ, false, "setTimeoutInterval:", |e| {
        let (r, t) = (e.arg(0), f64::from_bits(e.cpu.q[0] as u64));
        e.net.requests.entry(r).or_default().timeout = t;
        Ok(())
    }),
    (RS, false, "initWithURL:statusCode:HTTPVersion:headerFields:", response_init),
    ("NSURLResponse", false, "URL", |e| {
        let u = resp(e, e.arg(0)).url;
        ret(e, u)
    }),
    (RS, false, "statusCode", |e| {
        let s = resp(e, e.arg(0)).status;
        ret(e, s as u64)
    }),
    (RS, false, "allHeaderFields", |e| {
        let h = resp(e, e.arg(0)).headers;
        let d = headers_dict(e, &h)?;
        ret(e, d)
    }),
    (RS, false, "valueForHTTPHeaderField:", |e| {
        let name = foundation::nsstring(e, e.arg(2))?;
        let v = response_header(e, e.arg(0), &name);
        opt_string(e, v)
    }),
    ("NSURLResponse", false, "MIMEType", |e| {
        let (m, _) = content_type(e, e.arg(0));
        opt_string(e, m)
    }),
    ("NSURLResponse", false, "textEncodingName", |e| {
        let (_, c) = content_type(e, e.arg(0));
        opt_string(e, c)
    }),
    ("NSURLResponse", false, "expectedContentLength", |e| {
        let n = response_header(e, e.arg(0), "Content-Length").and_then(|v| v.parse::<i64>().ok()).unwrap_or(-1);
        ret(e, n as u64)
    }),
    ("NSURLResponse", false, "suggestedFilename", |e| opt_string(e, None)),
    (RS, true, "localizedStringForStatusCode:", |e| {
        let s = string(e, &format!("status {}", e.arg(2)))?;
        ret(e, s)
    }),
    // The headers for sending cookies: none, an empty dictionary; else
    // `Cookie: name=value; name=value`.
    ("NSHTTPCookie", true, "requestHeaderFieldsWithCookies:", |e| {
        let list = e.arg(2);
        let cookies = if list == 0 { Vec::new() } else { foundation::array_values(e, list)? };
        let mut parts = Vec::new();
        for c in cookies {
            let (n, v) = (send(e, c, "name", &[])?, send(e, c, "value", &[])?);
            let n = if n == 0 { String::new() } else { foundation::nsstring(e, n)? };
            let v = if v == 0 { String::new() } else { foundation::nsstring(e, v)? };
            parts.push(format!("{n}={v}"));
        }
        let d = if parts.is_empty() {
            foundation::new_dict_of(e, &[])?
        } else {
            let k = string(e, "Cookie")?;
            let v = string(e, &parts.join("; "))?;
            foundation::new_dict_of(e, &[(k, v)])?
        };
        let d = send(e, d, "autorelease", &[])?;
        ret(e, d)
    }),
    // A data task turned into a stream (see become_stream): its streams, on request.
    ("NSURLSessionStreamTask", false, "captureStreams", |e| {
        let st = e.arg(0);
        e.net.captured.insert(st);
        Ok(())
    }),
    (S, true, "sharedSession", shared_session),
    (S, true, "sessionWithConfiguration:", |e| {
        let s = new_session(e, e.arg(2), 0, 0)?;
        ret(e, s)
    }),
    (S, true, "sessionWithConfiguration:delegate:delegateQueue:", |e| {
        let s = new_session(e, e.arg(2), e.arg(3), e.arg(4))?;
        ret(e, s)
    }),
    (S, false, "dataTaskWithRequest:", |e| data_task(e, false)),
    (S, false, "dataTaskWithRequest:completionHandler:", |e| data_task(e, true)),
    (S, false, "dataTaskWithURL:", |e| data_task_with_url(e, false)),
    (S, false, "dataTaskWithURL:completionHandler:", |e| data_task_with_url(e, true)),
    (S, false, "downloadTaskWithRequest:", |e| download_task(e, false, false)),
    (S, false, "downloadTaskWithRequest:completionHandler:", |e| download_task(e, false, true)),
    (S, false, "downloadTaskWithURL:", |e| download_task(e, true, false)),
    (S, false, "downloadTaskWithURL:completionHandler:", |e| download_task(e, true, true)),
    (S, false, "uploadTaskWithRequest:fromFile:", |e| upload_from_file(e, false)),
    (S, false, "uploadTaskWithRequest:fromFile:completionHandler:", |e| upload_from_file(e, true)),
    (S, false, "uploadTaskWithRequest:fromData:", |e| {
        let (r, d) = (e.arg(2), e.arg(3));
        e.net.requests.entry(r).or_default().body = d;
        data_task(e, false)
    }),
    (S, false, "uploadTaskWithRequest:fromData:completionHandler:", |e| {
        let (r, d, b) = (e.arg(2), e.arg(3), e.arg(4));
        e.net.requests.entry(r).or_default().body = d;
        e.cpu.x[3] = b;
        data_task(e, true)
    }),
    (S, false, "delegate", |e| {
        let d = e.net.sessions.get(&e.arg(0)).map_or(0, |s| s.0);
        ret(e, d)
    }),
    (S, false, "configuration", |e| {
        let c = e.net.sessions.get(&e.arg(0)).map_or(0, |s| s.1);
        ret(e, c)
    }),
    (S, false, "finishTasksAndInvalidate", nothing),
    (S, false, "invalidateAndCancel", nothing),
    (T, false, "resume", resume),
    (T, false, "cancel", cancel),
    (T, false, "suspend", nothing),
    (T, false, "taskIdentifier", |e| {
        let id = task(e, e.arg(0)).id;
        ret(e, id)
    }),
    (T, false, "state", |e| {
        let s = task(e, e.arg(0)).state;
        ret(e, s)
    }),
    (T, false, "originalRequest", |e| {
        let r = task(e, e.arg(0)).request;
        ret(e, r)
    }),
    (T, false, "currentRequest", |e| {
        let r = task(e, e.arg(0)).request;
        ret(e, r)
    }),
    (T, false, "response", |e| {
        let r = task(e, e.arg(0)).response;
        ret(e, r)
    }),
    (T, false, "error", |e| {
        let r = task(e, e.arg(0)).error;
        ret(e, r)
    }),
    (T, false, "countOfBytesReceived", |e| {
        let n = task(e, e.arg(0)).received;
        ret(e, n)
    }),
    (T, false, "countOfBytesExpectedToReceive", |e| {
        let n = task(e, e.arg(0)).received;
        ret(e, n)
    }),
    (T, false, "countOfBytesSent", |e| ret(e, 0)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_and_headers() {
        let f = Fixtures::default().json("https://api.example.com/*", "{}");
        assert!(f.lookup("https://api.example.com/repos/x").is_some());
        assert!(f.lookup("https://other.example.com/").is_none());
        let mut h = vec![("Accept".to_string(), "a".to_string())];
        set_header(&mut h, "accept", Some("b".into()), true);
        assert_eq!(h, vec![("Accept".to_string(), "a,b".to_string())]);
        set_header(&mut h, "ACCEPT", None, false);
        assert!(h.is_empty());
    }
}

#[cfg(test)]
mod header_tests {
    use super::canonical_headers;

    #[test]
    fn names_are_canonical_and_repeats_join() {
        let h = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let got = canonical_headers(h(&[("content-type", "application/json"), ("x-goog-request-id", "1"), ("set-cookie", "a=1"), ("Set-Cookie", "b=2"), ("ETAG", "x")]));
        assert_eq!(got, h(&[("Content-Type", "application/json"), ("X-Goog-Request-Id", "1"), ("Set-Cookie", "a=1, b=2"), ("Etag", "x")]));
    }
}
