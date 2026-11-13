//! Web views' pages in the host's own web view: WebKit's WKWebView on macOS.
//!
//! On iOS a WKWebView's pages run in a process of WebKit's own; here they run
//! in `pomme-webview`, a helper process pomme starts, which hosts one real web
//! view per WKWebView the app makes (in a window that's never on screen). The
//! helper needs its process's main thread for the platform's web views, which
//! the emulator can't give up, and a page that crashes takes only the helper.
//!
//! The emulator talks to the helper in [`WebEngine`] terms: [`Command`]s go
//! down its stdin, [`Event`]s come up its stdout, each a message of JSON with
//! a binary payload beside it (the pixels of a [`Event::Frame`]). The helper
//! sends a page's pixels whenever they change (it snapshots the page, as
//! WKWebView's `takeSnapshot` does, about 30 times a second), so the page is
//! drawn into the phone's screen with everything else: views above it cover
//! it, and screenshots and recordings have it, video included.
//!
//! [`WebViewEngine`] is the emulator's side; [`helper_main`] is the helper.

use pomme_hle::webkit::{Command, Event, PageId, WebEngine};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[cfg(target_os = "macos")]
mod mac;

/// The helper's executable name.
pub const HELPER: &str = "pomme-webview";

// ---- the wire: JSON, then a binary payload ----

/// Write one message: a little-endian u32 length and the JSON, then the payload's length and bytes.
pub fn write_message<W: Write, T: serde::Serialize>(w: &mut W, msg: &T, payload: &[u8]) -> std::io::Result<()> {
    let json = serde_json::to_vec(msg).map_err(std::io::Error::other)?;
    w.write_all(&(json.len() as u32).to_le_bytes())?;
    w.write_all(&json)?;
    w.write_all(&(payload.len() as u32).to_le_bytes())?;
    w.write_all(payload)?;
    w.flush()
}

/// Read one message (None at the end of the stream).
pub fn read_message<R: Read, T: serde::de::DeserializeOwned>(r: &mut R) -> std::io::Result<Option<(T, Vec<u8>)>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        other => other?,
    }
    let mut json = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut json)?;
    r.read_exact(&mut len)?;
    let mut payload = vec![0u8; u32::from_le_bytes(len) as usize];
    r.read_exact(&mut payload)?;
    let msg = serde_json::from_slice(&json).map_err(std::io::Error::other)?;
    Ok(Some((msg, payload)))
}

/// An event on the wire: a frame's pixels travel as the payload.
fn split(ev: Event) -> (Event, Vec<u8>) {
    match ev {
        Event::Frame { width, height, rgba } => (Event::Frame { width, height, rgba: Vec::new() }, rgba),
        ev => (ev, Vec::new()),
    }
}

fn join(ev: Event, payload: Vec<u8>) -> Event {
    match ev {
        Event::Frame { width, height, .. } => Event::Frame { width, height, rgba: payload },
        ev => ev,
    }
}

// ---- the emulator's side ----

/// The engine: the helper process, and what it has said.
pub struct WebViewEngine {
    child: Child,
    to: BufWriter<ChildStdin>,
    from: mpsc::Receiver<(PageId, Event)>,
    ready: Vec<(PageId, Event)>,
}

impl WebViewEngine {
    /// Start the helper at `helper`.
    pub fn start(helper: &Path) -> std::io::Result<Self> {
        let mut child = std::process::Command::new(helper).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn()?;
        let to = BufWriter::new(child.stdin.take().expect("helper's stdin"));
        let mut out = BufReader::new(child.stdout.take().expect("helper's stdout"));
        let (tx, from) = mpsc::channel();
        std::thread::Builder::new().name("pomme-webview".into()).spawn(move || {
            while let Ok(Some(((page, ev), payload))) = read_message::<_, (PageId, Event)>(&mut out) {
                if tx.send((page, join(ev, payload))).is_err() {
                    break;
                }
            }
        })?;
        Ok(WebViewEngine { child, to, from, ready: Vec::new() })
    }

    /// The helper beside this executable (or, for a test binary in `deps/`, beside its directory),
    /// or where `POMME_WEBVIEW` says; None where there's no such file.
    pub fn find_helper() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("POMME_WEBVIEW") {
            return Some(PathBuf::from(p));
        }
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        [dir.join(HELPER), dir.parent()?.join(HELPER)].into_iter().find(|p| p.is_file())
    }
}

impl Drop for WebViewEngine {
    fn drop(&mut self) {
        // Closing its stdin ends the helper; don't leave it behind if it doesn't.
        let _ = self.to.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl WebEngine for WebViewEngine {
    fn send(&mut self, page: PageId, cmd: Command) {
        let _ = write_message(&mut self.to, &(page, cmd), &[]);
    }

    fn poll(&mut self) -> Vec<(PageId, Event)> {
        let mut out = std::mem::take(&mut self.ready);
        out.extend(self.from.try_iter());
        out
    }

    fn live(&self) -> bool {
        true
    }

    fn wait(&mut self, timeout: Duration) {
        if self.ready.is_empty()
            && let Ok(ev) = self.from.recv_timeout(timeout)
        {
            self.ready.push(ev);
        }
    }
}

// ---- the helper ----

/// The helper process: host the pages the emulator asks for until its stdin closes.
pub fn helper_main() -> std::process::ExitCode {
    #[cfg(target_os = "macos")]
    {
        mac::run()
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("pomme-webview: no host web view on this platform yet");
        std::process::ExitCode::FAILURE
    }
}

/// Where the helper writes its events (stdout, one writer for the process).
#[allow(dead_code)]
fn emit(page: PageId, ev: Event) {
    static OUT: std::sync::Mutex<Option<BufWriter<std::io::Stdout>>> = std::sync::Mutex::new(None);
    let mut out = OUT.lock().unwrap_or_else(|e| e.into_inner());
    let w = out.get_or_insert_with(|| BufWriter::new(std::io::stdout()));
    let (ev, payload) = split(ev);
    if write_message(w, &(page, ev), &payload).is_err() {
        // The emulator went away.
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pomme_hle::webkit::NavigationType;

    #[test]
    fn messages_round_trip_with_pixels_beside() {
        let mut buf = Vec::new();
        let (ev, payload) = split(Event::Frame { width: 1, height: 1, rgba: vec![1, 2, 3, 4] });
        write_message(&mut buf, &(7u64, ev), &payload).unwrap();
        let policy = Event::Policy { id: 3, url: "ytplayer://onReady".into(), method: "GET".into(), main_frame: true, kind: NavigationType::Other };
        write_message(&mut buf, &(7u64, policy.clone()), &[]).unwrap();
        let mut r = &buf[..];
        let ((p, ev), payload) = read_message::<_, (PageId, Event)>(&mut r).unwrap().unwrap();
        assert_eq!((p, join(ev, payload)), (7, Event::Frame { width: 1, height: 1, rgba: vec![1, 2, 3, 4] }));
        let ((_, ev), _) = read_message::<_, (PageId, Event)>(&mut r).unwrap().unwrap();
        assert_eq!(ev, policy);
        assert!(read_message::<_, (PageId, Event)>(&mut r).unwrap().is_none());
    }
}
