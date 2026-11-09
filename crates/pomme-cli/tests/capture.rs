//! File ▸ Save Screen and File ▸ Record Screen through the live host: the
//! screen comes back at the phone's own resolution (whatever the window's),
//! and a recording's frames, with their times, make a video. On the
//! transitions fixture (tests/fixtures/transitions): red, then blue pushed at
//! 0.5 s.

mod common;

use pomme_hle::host::{Event, Frame, Host, Idle};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Seen {
    shot: Option<(u32, u32, [u8; 4])>,
    recorded: Vec<(u64, u32, u32, Vec<u8>)>,
    stopped: Option<u64>,
}

/// Sends `script` (time, event) as the app reaches each time; records what comes back.
struct Capturing {
    script: Vec<(u64, Event)>,
    seen: Arc<Mutex<Seen>>,
}

impl Host for Capturing {
    fn idle(&mut self, now: u64, next: Option<u64>) -> Idle {
        if let Some(&(at, _)) = self.script.first()
            && next.is_none_or(|n| at <= n)
        {
            let (at, e) = self.script.remove(0);
            return Idle::Input {
                at_ns: at.max(now),
                events: vec![e],
            };
        }
        Idle::Advance
    }
    fn frame(&mut self, _: Frame<'_>) {}
    fn screenshot(&mut self, f: Frame<'_>) {
        let mid = ((f.height / 2 * f.width + f.width / 2) * 4) as usize;
        self.seen.lock().unwrap().shot =
            Some((f.width, f.height, f.rgba[mid..mid + 4].try_into().unwrap()));
    }
    fn recorded(&mut self, f: Frame<'_>) {
        self.seen
            .lock()
            .unwrap()
            .recorded
            .push((f.at_ns, f.width, f.height, f.rgba.to_vec()));
    }
    fn recording_stopped(&mut self, at_ns: u64) {
        self.seen.lock().unwrap().stopped = Some(at_ns);
    }
}

#[test]
fn save_and_record_the_screen() {
    let Some(mut emu) = common::boot("transitions") else {
        return;
    };
    let seen = Arc::new(Mutex::new(Seen::default()));
    let script = vec![
        (300_000_000, Event::SaveScreen),
        (400_000_000, Event::Record(true)),
        (1_400_000_000, Event::Record(false)),
    ];
    emu.live.host = Some(Box::new(Capturing {
        script,
        seen: seen.clone(),
    }));
    // The window draws at 1 pixel per point; captures are at the phone's 3.
    emu.live.scale = 1.0;
    common::views(&mut emu, 1.6);
    let seen = seen.lock().unwrap();

    // The screen before the push: red, at the iPhone 16e's 1170 × 2532.
    let (w, h, mid) = seen.shot.expect("a screenshot");
    assert_eq!((w, h), (1170, 2532));
    assert_eq!(mid, [255, 0, 0, 255]);

    // The recording: the screen as it started, then the push's frames, all at full size, in order.
    let rec = &seen.recorded;
    assert!(rec.len() > 8, "{} frames", rec.len());
    assert!(rec.iter().all(|f| (f.1, f.2) == (1170, 2532)));
    assert!(
        rec.windows(2).all(|p| p[0].0 < p[1].0),
        "frames in time order"
    );
    assert!(
        (rec[0].0 as i64 - 400_000_000).abs() < 10_000_000,
        "starts when asked: {}",
        rec[0].0
    );
    let stopped = seen.stopped.expect("the recording stopped");
    assert!(stopped >= 1_400_000_000 && stopped > rec.last().unwrap().0);

    // They make a video: H.264 at full size, a sample per frame, as long as the recording.
    let path = std::env::temp_dir().join(format!("pomme-capture-test-{}.mp4", std::process::id()));
    let mut video = pomme_video::Recorder::create(&path, 1170, 2532).unwrap();
    for (at, w, h, rgba) in rec {
        video.push(rgba, *w, *h, *at).unwrap();
    }
    video.finish(stopped).unwrap();
    let f = std::fs::File::open(&path).unwrap();
    let size = f.metadata().unwrap().len();
    let mp4 = mp4::Mp4Reader::read_header(std::io::BufReader::new(f), size).unwrap();
    let track = mp4.tracks().get(&1).unwrap();
    assert_eq!((track.width(), track.height()), (1170, 2532));
    assert_eq!(track.sample_count() as usize, rec.len());
    let secs = track.duration().as_secs_f64();
    let expect = (stopped - rec[0].0) as f64 / 1e9;
    assert!((secs - expect).abs() < 0.01, "{secs} s, expected {expect}");
    std::fs::remove_file(&path).ok();
}
