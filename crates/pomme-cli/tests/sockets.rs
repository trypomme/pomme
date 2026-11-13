//! BSD sockets (tests/fixtures/sockets/main.m), as a gRPC-style client uses
//! them: resolve, connect, non-blocking and blocking sends and receives,
//! poll, select, socket pairs, DNS-SD lookups and CFStream socket streams.
//! With a network, "echo.pomme.test" is a real echo server on the host's
//! loopback; without one, names don't resolve and connections don't open. Built with the iOS SDK; skipped without Xcode or
//! the sysroot.

mod common;

use pomme_hle::net::{HostSockets, Net, Readiness, Request, Response, SocketId};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener};
use std::time::Duration;

/// The host's TCP, with every name resolving to the loopback and every
/// connection going to the echo server.
struct EchoNet {
    sockets: HostSockets,
    server: SocketAddr,
}

impl Net for EchoNet {
    fn fetch(&mut self, _: &Request) -> Result<Response, String> {
        Err("no HTTP here".into())
    }
    fn resolve(&mut self, _host: &str) -> Result<Vec<IpAddr>, String> {
        Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)])
    }
    fn connect(&mut self, _addr: SocketAddr) -> std::io::Result<SocketId> {
        self.sockets.connect(self.server)
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
    fn wait(&mut self, sockets: &[SocketId], timeout: Duration) {
        self.sockets.wait(sockets, timeout)
    }
    fn shutdown(&mut self, s: SocketId, how: Shutdown) {
        self.sockets.shutdown(s, how)
    }
    fn close(&mut self, s: SocketId) {
        self.sockets.close(s)
    }
    fn local_addr(&mut self, s: SocketId) -> Option<SocketAddr> {
        self.sockets.local_addr(s)
    }
    fn peer_addr(&mut self, s: SocketId) -> Option<SocketAddr> {
        self.sockets.peer_addr(s)
    }
}

/// An echo server on the loopback: everything a client sends comes back.
fn echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut c) = conn else { break };
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = c.read(&mut buf) {
                    if n == 0 || c.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

fn checks(emu: &mut pomme_hle::Emu) -> String {
    let v = common::views(emu, 1.0);
    v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}")).to_string()
}

#[test]
fn sockets_with_a_network() {
    let Some(mut emu) = common::boot("sockets") else { return };
    emu.net.client = Some(Box::new(EchoNet { sockets: HostSockets::default(), server: echo_server() }));
    let line = checks(&mut emu);
    let expected = "inet=1 numericName=1 loopback=1 echo=1 blockingRead=1 selectAndPeer=1 dnssd=1 cfstream=1 socketpair=1";
    assert!(line.ends_with(&format!("\"{expected}\"")), "{line}");
}

#[test]
fn sockets_offline() {
    let Some(mut emu) = common::boot("sockets") else { return };
    emu.net.client = None;
    let line = checks(&mut emu);
    let expected = "inet=1 numericName=1 loopback=1 offline=1 dnssdOffline=1 cfstreamOffline=1 socketpair=1";
    assert!(line.ends_with(&format!("\"{expected}\"")), "{line}");
}
