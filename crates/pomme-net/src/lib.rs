//! The live network for a run on a computer: real HTTP (with `ureq`) for
//! whatever the fixtures don't answer, and the host's TCP for the app's
//! sockets. The `pomme` command uses it, and so do tests that run an app
//! against a real server (a development build against Metro). It isn't in
//! pomme-hle, which also builds for the browser, where `ureq` doesn't.

use pomme_hle::net::{Fixtures, HostSockets, Net, Readiness, Request, Response, SocketId};

/// Real HTTP for whatever the fixtures don't answer, and the host's TCP for sockets.
pub struct LiveNet {
    fixtures: Fixtures,
    agent: ureq::Agent,
    sockets: HostSockets,
}

impl LiveNet {
    /// Live, with these canned answers first.
    pub fn new(fixtures: Fixtures) -> LiveNet {
        LiveNet { fixtures, agent: ureq::Agent::new_with_defaults(), sockets: HostSockets::default() }
    }
}

impl Net for LiveNet {
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

