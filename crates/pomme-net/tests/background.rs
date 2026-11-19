//! Requests out in the background (`LiveNet::start` and `poll`): a fixture
//! answers at once, a real request answers later on its own thread, and a
//! forgotten request's answer goes nowhere.
use pomme_hle::net::{Fixtures, Net, Request};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn get(url: &str) -> Request {
    Request { method: "GET".into(), url: url.into(), headers: Vec::new(), body: Vec::new() }
}

/// A server on this computer that answers each request with "hello" after `delay`.
fn slow_server(delay: Duration) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                std::thread::sleep(delay);
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
            });
        }
    });
    format!("http://{addr}/")
}

#[test]
fn a_fixture_answers_at_once() {
    let mut net = pomme_net::LiveNet::new(Fixtures::default().json("https://api.example.com/*", "{}"));
    assert_eq!(net.start(&get("https://api.example.com/x")), None);
    assert_eq!(net.fetch(&get("https://api.example.com/x")).unwrap().body, b"{}");
}

#[test]
fn a_real_request_answers_later() {
    let url = slow_server(Duration::from_millis(300));
    let mut net = pomme_net::LiveNet::new(Fixtures::default());
    let begun = Instant::now();
    let id = net.start(&get(&url)).expect("goes out in the background");
    assert!(begun.elapsed() < Duration::from_millis(200), "start waited for the answer");
    assert!(net.poll(id).is_none(), "answered before the server did");
    let answer = loop {
        if let Some(a) = net.poll(id) {
            break a;
        }
        assert!(begun.elapsed() < Duration::from_secs(10), "no answer");
        std::thread::sleep(Duration::from_millis(10));
    };
    let r = answer.unwrap();
    assert_eq!((r.status, &r.body[..]), (200, &b"hello"[..]));
    assert!(net.poll(id).is_none(), "an answer comes once");
}

#[test]
fn a_forgotten_request_has_no_answer() {
    let url = slow_server(Duration::from_millis(50));
    let mut net = pomme_net::LiveNet::new(Fixtures::default());
    let id = net.start(&get(&url)).unwrap();
    net.forget(id);
    std::thread::sleep(Duration::from_millis(300));
    assert!(net.poll(id).is_none());
}
