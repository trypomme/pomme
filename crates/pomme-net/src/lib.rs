//! The live network for a run on a computer: real HTTP (with `ureq`) for
//! whatever the fixtures don't answer, and the host's TCP for the app's
//! sockets. The `pomme` command uses it, and so do tests that run an app
//! against a real server (a development build against Metro). It isn't in
//! pomme-hle, which also builds for the browser, where `ureq` doesn't.
//! Bonjour browsing (Expo's dev launcher finding Metro) is real: macOS's
//! responder on a Mac (bonjour_macos.rs), mDNS with `mdns-sd` elsewhere.

#[cfg(target_os = "macos")]
mod bonjour_macos;

use pomme_hle::net::{Fixtures, HostSockets, Net, Readiness, Request, Response, Service, SocketId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Real HTTP for whatever the fixtures don't answer, and the host's TCP for sockets.
pub struct LiveNet {
    fixtures: Fixtures,
    agent: ureq::Agent,
    sockets: HostSockets,
    /// mDNS (not on macOS), started on the first browse.
    #[cfg(not(target_os = "macos"))]
    mdns: Option<mdns_sd::ServiceDaemon>,
    /// What each kind has found so far (by name), kept up to date on
    /// threads of its own.
    found: HashMap<String, Arc<Mutex<HashMap<String, Service>>>>,
}

impl LiveNet {
    /// Live, with these canned answers first.
    pub fn new(fixtures: Fixtures) -> LiveNet {
        LiveNet { fixtures, agent: ureq::Agent::new_with_defaults(), sockets: HostSockets::default(), #[cfg(not(target_os = "macos"))] mdns: None, found: HashMap::new() }
    }
}

/// A resolved mDNS service as pomme's.
#[cfg(not(target_os = "macos"))]
fn service(r: &mdns_sd::ResolvedService) -> Service {
    let kind_domain = r.ty_domain.trim_end_matches('.');
    // "_expo._tcp.local" → ("_expo._tcp", "local.")
    let (kind, domain) = match kind_domain.rsplit_once('.') {
        Some((k, d)) => (k.to_string(), format!("{d}.")),
        None => (kind_domain.to_string(), "local.".to_string()),
    };
    Service {
        name: mdns_sd::instance_name_unescaped(&r.fullname),
        kind,
        domain,
        host: r.host.clone(),
        port: r.port,
        addresses: r.addresses.iter().map(|a| a.to_ip_addr()).collect(),
        txt: r.txt_properties.iter().map(|p| (p.key().to_string(), p.val().map(<[u8]>::to_vec))).collect(),
    }
}

impl LiveNet {
    #[cfg(target_os = "macos")]
    fn start_browse(&mut self, kind: &str, found: Arc<Mutex<HashMap<String, Service>>>) {
        bonjour_macos::start(kind, found);
    }

    #[cfg(not(target_os = "macos"))]
    fn start_browse(&mut self, kind: &str, found: Arc<Mutex<HashMap<String, Service>>>) {
        if self.mdns.is_none() {
            self.mdns = mdns_sd::ServiceDaemon::new().ok();
        }
        let Some(rx) = self.mdns.as_ref().and_then(|d| d.browse(&format!("{kind}.local.")).ok()) else { return };
        std::thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(r) => {
                        let s = service(&r);
                        found.lock().unwrap().insert(s.name.clone(), s);
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, full) => {
                        found.lock().unwrap().remove(&mdns_sd::instance_name_unescaped(&full));
                    }
                    _ => {}
                }
            }
        });
    }
}

impl Net for LiveNet {
    fn browse(&mut self, kind: &str) -> Vec<Service> {
        if !self.found.contains_key(kind) {
            let found: Arc<Mutex<HashMap<String, Service>>> = Default::default();
            self.found.insert(kind.to_string(), found.clone());
            self.start_browse(kind, found);
        }
        let mut v: Vec<Service> = self.found[kind].lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    fn fetch(&mut self, req: &Request) -> std::result::Result<Response, String> {
        if let Some(r) = self.fixtures.lookup(&req.url) {
            return Ok(r.clone());
        }
        let mut b = ureq::http::Request::builder().method(req.method.as_str()).uri(&req.url);
        for (k, v) in &req.headers {
            b = b.header(k, v);
        }
        let request = b.body(req.body.clone()).map_err(|e| e.to_string())?;
        let request = self.agent.configure_request(request).http_status_as_error(false).build();
        let mut response = self.agent.run(request).map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let headers = response.headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect();
        // No 10 MB cap (ureq's default): iOS has none, and a development
        // bundle from Metro is bigger than that.
        let body = response.body_mut().with_config().limit(1 << 30).read_to_vec().map_err(|e| e.to_string())?;
        Ok(Response { status, headers, body })
    }

    fn resolve(&mut self, host: &str) -> std::result::Result<Vec<std::net::IpAddr>, String> {
        self.sockets.resolve(host)
    }
    fn connect(&mut self, addr: std::net::SocketAddr) -> std::io::Result<SocketId> {
        self.sockets.connect(addr)
    }
    fn send(&mut self, s: SocketId, data: &[u8]) -> std::io::Result<usize> {
        self.sockets.send(s, data)
    }
    fn recv(&mut self, s: SocketId, max: usize) -> std::io::Result<Vec<u8>> {
        self.sockets.recv(s, max)
    }
    fn readiness(&mut self, s: SocketId) -> Readiness {
        self.sockets.readiness(s)
    }
    fn wait(&mut self, sockets: &[SocketId], timeout: std::time::Duration) {
        self.sockets.wait(sockets, timeout)
    }
    fn shutdown(&mut self, s: SocketId, how: std::net::Shutdown) {
        self.sockets.shutdown(s, how)
    }
    fn close(&mut self, s: SocketId) {
        self.sockets.close(s)
    }
    fn local_addr(&mut self, s: SocketId) -> Option<std::net::SocketAddr> {
        self.sockets.local_addr(s)
    }
    fn peer_addr(&mut self, s: SocketId) -> Option<std::net::SocketAddr> {
        self.sockets.peer_addr(s)
    }
}

