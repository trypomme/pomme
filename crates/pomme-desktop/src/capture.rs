//! File ▸ Save Screen and File ▸ Record Screen, as the Simulator has them: a
//! PNG or a video of the screen at the phone's own resolution, saved to the
//! Desktop and named like the Simulator's ("pomme Screenshot - iPhone 16e -
//! 2026-11-10 at 14.03.22.png"). Encoding happens on threads of its own, so
//! the app keeps running while a screenshot is written or a recording encodes.

use pomme_hle::host::Frame;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::JoinHandle;

/// Where captures go: the Desktop, as the Simulator saves them (the current
/// folder when there's no Desktop).
pub(crate) fn folder() -> PathBuf {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    match home.map(|h| h.join("Desktop")) {
        Some(d) if d.is_dir() => d,
        _ => PathBuf::from("."),
    }
}

/// "pomme Screenshot - iPhone 16e - 2026-11-10 at 14.03.22.png", in `dir`, made unique.
pub(crate) fn name(
    dir: &Path,
    what: &str,
    device: &str,
    ext: &str,
    when: chrono::DateTime<chrono::Local>,
) -> PathBuf {
    let stem = format!(
        "pomme {what} - {device} - {}",
        when.format("%Y-%m-%d at %H.%M.%S")
    );
    let mut path = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem} {n}.{ext}"));
        n += 1;
    }
    path
}

/// Save the screen as a PNG on the Desktop (on its own thread).
pub(crate) fn save_screenshot(f: Frame<'_>, device: &'static str) {
    let (rgba, w, h) = (f.rgba.to_vec(), f.width, f.height);
    let path = name(&folder(), "Screenshot", device, "png", chrono::Local::now());
    std::thread::spawn(move || {
        let png = tiny_skia::IntSize::from_wh(w, h)
            .and_then(|s| tiny_skia::Pixmap::from_vec(rgba, s))
            .map(|p| p.save_png(&path));
        match png {
            Some(Ok(())) => eprintln!("Saved the screen to {}", path.display()),
            Some(Err(e)) => eprintln!("Couldn't save the screen to {}: {e}", path.display()),
            None => eprintln!("Couldn't save the screen: no picture"),
        }
    });
}

enum Msg {
    Frame {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
        at_ns: u64,
    },
    /// Stop: at this time, or (the window closed) just after the last frame.
    End(Option<u64>),
}

/// A recording in progress: frames go to an encoder thread, which writes the
/// video when it ends.
pub(crate) struct Recording {
    tx: SyncSender<Msg>,
    worker: Option<JoinHandle<()>>,
}

impl Recording {
    pub(crate) fn start(device: &'static str) -> Self {
        let path = name(
            &folder(),
            "Screen Recording",
            device,
            "mp4",
            chrono::Local::now(),
        );
        // A few frames' slack; past that the app waits for the encoder rather
        // than the frames piling up in memory (each is several megabytes).
        let (tx, rx) = sync_channel::<Msg>(8);
        let worker = std::thread::Builder::new()
            .name("pomme recording".into())
            .spawn(move || {
                let mut rec: Option<pomme_video::Recorder> = None;
                let mut last = 0;
                for msg in rx {
                    match msg {
                        Msg::Frame {
                            rgba,
                            width,
                            height,
                            at_ns,
                        } => {
                            last = at_ns;
                            if rec.is_none() {
                                match pomme_video::Recorder::create(&path, width, height) {
                                    Ok(r) => rec = Some(r),
                                    Err(e) => {
                                        return eprintln!(
                                            "Couldn't record to {}: {e}",
                                            path.display()
                                        );
                                    }
                                }
                            }
                            if let Some(Err(e)) =
                                rec.as_mut().map(|r| r.push(&rgba, width, height, at_ns))
                            {
                                return eprintln!("Couldn't record a frame: {e}");
                            }
                        }
                        Msg::End(at_ns) => {
                            // A closing window ends it a frame after the last.
                            let end = at_ns.unwrap_or(last + 1_000_000_000 / 30);
                            return finish(rec.take(), end, &path);
                        }
                    }
                }
            })
            .expect("the recording thread");
        eprintln!("Recording the screen: File ▸ Stop Recording (⌘R) to finish");
        Self {
            tx,
            worker: Some(worker),
        }
    }

    pub(crate) fn frame(&self, f: Frame<'_>) {
        let _ = self.tx.send(Msg::Frame {
            rgba: f.rgba.to_vec(),
            width: f.width,
            height: f.height,
            at_ns: f.at_ns,
        });
    }

    /// Stop at `at_ns`, once the encoder has written the video.
    pub(crate) fn stop(mut self, at_ns: u64) {
        self.end(Some(at_ns));
    }

    fn end(&mut self, at_ns: Option<u64>) {
        if let Some(w) = self.worker.take() {
            let _ = self.tx.send(Msg::End(at_ns));
            let _ = w.join();
        }
    }
}

impl Drop for Recording {
    /// The window closed while recording: finish the video before the run ends.
    fn drop(&mut self) {
        self.end(None);
    }
}

fn finish(rec: Option<pomme_video::Recorder>, at_ns: u64, path: &Path) {
    let Some(rec) = rec else {
        return eprintln!("Nothing was recorded");
    };
    match rec.finish(at_ns) {
        Ok(()) => eprintln!("Saved the recording to {}", path.display()),
        Err(e) => eprintln!("Couldn't finish the recording {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn names_like_the_simulator() {
        let dir = std::env::temp_dir().join(format!("pomme-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let when = chrono::Local
            .with_ymd_and_hms(2026, 11, 10, 14, 3, 22)
            .unwrap();
        let p = name(&dir, "Screenshot", "iPhone 16e", "png", when);
        assert_eq!(
            p.file_name().unwrap(),
            "pomme Screenshot - iPhone 16e - 2026-11-10 at 14.03.22.png"
        );
        std::fs::write(&p, b"").unwrap();
        // Two in the same second don't overwrite each other.
        let q = name(&dir, "Screenshot", "iPhone 16e", "png", when);
        assert_eq!(
            q.file_name().unwrap(),
            "pomme Screenshot - iPhone 16e - 2026-11-10 at 14.03.22 2.png"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
