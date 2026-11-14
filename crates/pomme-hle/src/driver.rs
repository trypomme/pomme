//! Driving an app from a test, step by step, as XCUITest or Maestro do on a
//! device: find an element, tap it, type, wait until something shows.
//!
//! [`drive`] runs the app on this thread and the test on another, in
//! lockstep: while the test looks or acts the app is paused, and while the
//! app runs the test waits. The test gets its turns whenever the app is idle
//! (every thread waiting, as between frames), on the main thread through a
//! source on the main run loop, as UIKit delivers events. So what the test
//! sees is a committed screen, what it does reaches the app as touches and
//! keys do, and a run is as deterministic as a scripted one: virtual time
//! moves only when the test lets the app run.
//!
//! ```ignore
//! driver::drive(&mut emu, |app| {
//!     app.tap(&id("name"))?;
//!     app.type_text("Ada")?;
//!     app.wait_for(&text("Hello, Ada!"))?;
//!     Ok(())
//! })?;
//! ```

use crate::accessibility::{self, Element};
use crate::touch::{BEGAN, ENDED, MOVED};
use crate::{Emu, Result, Stop};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// How long the test waits for something by default (virtual seconds).
pub const DEFAULT_TIMEOUT: f64 = 10.0;
/// How often a wait looks again (virtual seconds): every few frames.
pub const POLL_INTERVAL: f64 = 0.05;
/// How long a finger stays down for a tap.
const TAP_HOLD: f64 = 0.08;
/// Between keys, as a quick typist.
const KEY_INTERVAL: f64 = 0.03;
/// Between the touch moves of a swipe (60 Hz, as the digitizer reports).
const MOVE_INTERVAL: f64 = 1.0 / 60.0;

type Work = Box<dyn FnOnce(&mut Emu) -> Result<()> + Send>;

enum Command {
    /// Run this on the main thread, with the app paused.
    Exec(Work),
    /// Let the app run until this virtual time (ns), then take the next turn when it's idle.
    Resume(u64),
}

/// The emulator's side of a drive.
pub(crate) struct State {
    commands: Receiver<Command>,
    /// A turn started, at this virtual time.
    turns: Sender<u64>,
    /// Don't take a turn before this virtual time.
    resume_at: u64,
    /// The run-loop source is signalled: the turn starts when the main thread gets to it.
    signalled: bool,
    /// The test is taking its turn (waits inside it are the app's own).
    in_turn: bool,
}

/// Why a step failed.
#[derive(Debug, Clone)]
pub enum Error {
    /// The app stopped (it exited or crashed, or pomme couldn't go on): why.
    Stopped(String),
    /// What the test waited for didn't happen in time; what the screen showed then.
    Timeout { waiting_for: String, seconds: f64, screen: String },
    /// Something the test asked for that can't be done.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Stopped(why) => write!(f, "the app stopped: {why}"),
            Error::Timeout { waiting_for, seconds, screen } => {
                write!(f, "waited {seconds:.1} s for {waiting_for}; the screen has:\n{screen}")
            }
            Error::Failed(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {}

/// What to look for on screen.
#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    /// An accessibility identifier: React Native's `testID`, SwiftUI's
    /// `.accessibilityIdentifier`, Flutter's `Semantics(identifier:)`, UIKit's own.
    Id(String),
    /// An accessibility label or value that's exactly this (a text's text, a button's title).
    Text(String),
    /// A label or value containing this.
    TextContaining(String),
}

/// An element with this accessibility identifier.
pub fn id(s: &str) -> Query {
    Query::Id(s.into())
}

/// An element whose label or value is exactly this text.
pub fn text(s: &str) -> Query {
    Query::Text(s.into())
}

/// An element whose label or value contains this text.
pub fn text_containing(s: &str) -> Query {
    Query::TextContaining(s.into())
}

impl Query {
    pub fn matches(&self, e: &Element) -> bool {
        let says = |f: &dyn Fn(&str) -> bool| e.label.as_deref().is_some_and(f) || e.value.as_deref().is_some_and(f);
        match self {
            Query::Id(id) => e.identifier.as_deref() == Some(id.as_str()),
            Query::Text(t) => says(&|s| s == t),
            Query::TextContaining(t) => says(&|s| s.contains(t.as_str())),
        }
    }
}

impl std::fmt::Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Query::Id(id) => write!(f, "#{id}"),
            Query::Text(t) => write!(f, "{t:?}"),
            Query::TextContaining(t) => write!(f, "text containing {t:?}"),
        }
    }
}

/// The test's side: the app, paused for the test to look at and act on.
pub struct App {
    commands: Sender<Command>,
    turns: Receiver<u64>,
    /// Why the run ended, once it has.
    ended: Arc<Mutex<Option<String>>>,
    now_ns: u64,
    /// How long waits wait (virtual seconds).
    pub timeout: f64,
}

impl App {
    fn stopped(&self) -> Error {
        Error::Stopped(self.ended.lock().unwrap().clone().unwrap_or_else(|| "the run ended".into()))
    }

    fn await_turn(&mut self) -> std::result::Result<(), Error> {
        self.now_ns = self.turns.recv().map_err(|_| self.stopped())?;
        Ok(())
    }

    /// Run `f` with the app paused, on its main thread (between run-loop
    /// iterations, as an event is handled), and return what it returns.
    pub fn with<T: Send + 'static>(&mut self, f: impl FnOnce(&mut Emu) -> Result<T> + Send + 'static) -> std::result::Result<T, Error> {
        let (tx, rx) = channel();
        let work: Work = Box::new(move |emu| match f(emu) {
            Ok(v) => {
                let _ = tx.send(Ok(v));
                Ok(())
            }
            Err(e) => {
                let _ = tx.send(Err(Error::Stopped(e.to_string())));
                Err(e)
            }
        });
        self.commands.send(Command::Exec(work)).map_err(|_| self.stopped())?;
        rx.recv().map_err(|_| self.stopped())?
    }

    /// Virtual time now, in seconds since the app started.
    pub fn now(&self) -> f64 {
        self.now_ns as f64 / 1e9
    }

    /// Let the app run for `secs` of virtual time (then until it's next idle).
    pub fn run_for(&mut self, secs: f64) -> std::result::Result<(), Error> {
        let until = self.now_ns + (secs.max(0.0) * 1e9) as u64;
        self.commands.send(Command::Resume(until)).map_err(|_| self.stopped())?;
        self.await_turn()
    }

    /// What accessibility sees on screen now.
    pub fn elements(&mut self) -> std::result::Result<Vec<Element>, Error> {
        self.with(accessibility::snapshot)
    }

    /// The screen's elements as indented text (for messages and debugging).
    pub fn describe(&mut self) -> std::result::Result<String, Error> {
        Ok(accessibility::describe(&self.elements()?))
    }

    /// The UIKit view hierarchy as text (`pomme run --views`).
    pub fn views(&mut self) -> std::result::Result<String, Error> {
        self.with(|emu| Ok(emu.describe_views()))
    }

    /// A PNG of the screen at `scale` pixels per point.
    pub fn screenshot_png(&mut self, scale: f32) -> std::result::Result<Vec<u8>, Error> {
        self.with(move |emu| emu.screenshot_png(scale))
    }

    /// Every element on screen that matches, front-most first.
    pub fn find_all(&mut self, q: &Query) -> std::result::Result<Vec<Element>, Error> {
        let q = q.clone();
        self.with(move |emu| {
            let screen = crate::orientation::screen(emu);
            let mut found: Vec<Element> = accessibility::snapshot(emu)?.into_iter().filter(|e| q.matches(e) && e.on_screen(screen)).collect();
            found.reverse();
            Ok(found)
        })
    }

    /// The front-most element on screen that matches, if there is one now.
    pub fn find(&mut self, q: &Query) -> std::result::Result<Option<Element>, Error> {
        Ok(self.find_all(q)?.into_iter().next())
    }

    /// Let the app run until `look` finds something, looking every [`POLL_INTERVAL`].
    fn wait<T>(&mut self, what: &str, mut look: impl FnMut(&mut Self) -> std::result::Result<Option<T>, Error>) -> std::result::Result<T, Error> {
        let deadline = self.now() + self.timeout;
        loop {
            if let Some(v) = look(self)? {
                return Ok(v);
            }
            if self.now() >= deadline {
                let screen = self.describe()?;
                return Err(Error::Timeout { waiting_for: what.into(), seconds: self.timeout, screen });
            }
            self.run_for(POLL_INTERVAL)?;
        }
    }

    /// Wait until an element that matches is on screen.
    pub fn wait_for(&mut self, q: &Query) -> std::result::Result<Element, Error> {
        self.wait(&q.to_string(), |app| app.find(q))
    }

    /// Wait until nothing on screen matches.
    pub fn wait_until_gone(&mut self, q: &Query) -> std::result::Result<(), Error> {
        self.wait(&format!("{q} to go"), |app| Ok(app.find(q)?.is_none().then_some(())))
    }

    /// Wait until the screen stops changing: no animation running, and the
    /// elements the same from one look to the next.
    pub fn settle(&mut self) -> std::result::Result<(), Error> {
        let mut last = None;
        self.wait("the screen to settle", |app| {
            let (now, moving) = app.with(|emu| Ok((accessibility::snapshot(emu)?, crate::animation::animating(emu))))?;
            let same = last.as_ref() == Some(&now);
            last = Some(now);
            Ok((same && !moving).then_some(()))
        })
    }

    /// Wait for an element that matches and that a touch at its center would
    /// reach (nothing in front of it), then tap it there. Returns it.
    pub fn tap(&mut self, q: &Query) -> std::result::Result<Element, Error> {
        let e = self.wait(&format!("{q} to be tappable"), |app| {
            let found = app.find_all(q)?;
            app.with(move |emu| {
                for e in found {
                    if accessibility::hittable(emu, &e)? {
                        return Ok(Some(e));
                    }
                }
                Ok(None)
            })
        })?;
        let (x, y) = e.center();
        self.tap_at(x, y)?;
        Ok(e)
    }

    /// Tap the screen at a point (in points).
    pub fn tap_at(&mut self, x: f64, y: f64) -> std::result::Result<(), Error> {
        self.touch(BEGAN, x, y)?;
        self.run_for(TAP_HOLD)?;
        self.touch(ENDED, x, y)
    }

    /// A finger down, moving or up at a point now.
    pub fn touch(&mut self, phase: u64, x: f64, y: f64) -> std::result::Result<(), Error> {
        self.with(move |emu| crate::touch::send_touch(emu, phase, x, y))
    }

    /// Drag a finger from one point to another over `secs`, moving at 60 Hz.
    pub fn swipe(&mut self, from: (f64, f64), to: (f64, f64), secs: f64) -> std::result::Result<(), Error> {
        let steps = ((secs / MOVE_INTERVAL).round() as usize).max(1);
        self.touch(BEGAN, from.0, from.1)?;
        for i in 1..=steps {
            self.run_for(secs / steps as f64)?;
            let t = i as f64 / steps as f64;
            let (x, y) = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            self.touch(if i == steps { ENDED } else { MOVED }, x, y)?;
        }
        Ok(())
    }

    /// Type on the keyboard into whatever has focus (tap a text field
    /// first), a key at a time: `'\n'` is return, `'\u{8}'` delete.
    pub fn type_text(&mut self, s: &str) -> std::result::Result<(), Error> {
        // As XCUITest's typeText: only once something has keyboard focus (a
        // tapped field may take it a little later, as Flutter's do).
        self.wait("something with keyboard focus", |app| app.with(|emu| Ok((emu.input.first_responder != 0 && emu.input.keyboard_shown).then_some(()))))?;
        for ch in s.chars() {
            self.with(move |emu| crate::textinput::key(emu, ch))?;
            self.run_for(KEY_INTERVAL)?;
        }
        Ok(())
    }
}

/// Run the app with `test` driving it, until the test returns. The test's
/// first turn comes once the app is first idle; whatever it returns is the
/// result. If the app stops first, the test's next step fails with
/// [`Error::Stopped`]; a panic in the test is passed on.
pub fn drive<R: Send>(emu: &mut Emu, test: impl FnOnce(&mut App) -> std::result::Result<R, Error> + Send) -> std::result::Result<R, Error> {
    let (commands_tx, commands) = channel();
    let (turns, turns_rx) = channel();
    let ended = Arc::new(Mutex::new(None));
    emu.driver = Some(State { commands, turns, resume_at: 0, signalled: false, in_turn: false });
    std::thread::scope(|s| {
        let ended_t = ended.clone();
        let tester = s.spawn(move || {
            let mut app = App { commands: commands_tx, turns: turns_rx, ended: ended_t, now_ns: 0, timeout: DEFAULT_TIMEOUT };
            app.await_turn()?;
            test(&mut app)
        });
        let run = emu.run_main();
        // The test finishing ends the run as exit(0) does; anything else is why the app stopped.
        *ended.lock().unwrap() = Some(match &run {
            Ok(code) => format!("it exited ({code})"),
            Err(e) => e.to_string(),
        });
        if let Some(d) = emu.driver.as_mut() {
            d.in_turn = false;
        }
        emu.driver = None;
        match tester.join() {
            Ok(r) => r,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    })
}

/// Whether a test is driving this run.
pub(crate) fn active(emu: &Emu) -> bool {
    emu.driver.is_some()
}

/// Every thread waits (`next_ns`: the earliest deadline). Ok(true): something
/// changed, look again; Ok(false): carry on as without a driver.
pub(crate) fn idle(emu: &mut Emu, next_ns: Option<u64>) -> Result<bool> {
    let Some(d) = emu.driver.as_ref() else { return Ok(false) };
    if d.in_turn || d.signalled {
        // Waiting inside the test's turn, or for the main thread to start it: the app's own time.
        return Ok(false);
    }
    let now = crate::threads::now_ns(emu);
    let resume_at = d.resume_at;
    if now < resume_at {
        // Run on to the next deadline, or to when the test wants its turn.
        let target = next_ns.map_or(resume_at, |n| n.min(resume_at));
        crate::threads::skip_to(emu, target)?;
        return Ok(true);
    }
    if !crate::host::signal_input(emu)? {
        return Ok(false); // no main run loop yet
    }
    emu.driver.as_mut().unwrap().signalled = true;
    Ok(true)
}

/// The test's turn, on the main thread: report the time, then do what it
/// asks until it lets the app run again (or finishes: the run ends).
pub(crate) fn turn(emu: &mut Emu) -> Result<()> {
    let Some(d) = emu.driver.as_mut() else { return Ok(()) };
    if !std::mem::replace(&mut d.signalled, false) {
        return Ok(());
    }
    d.in_turn = true;
    let now = crate::threads::now_ns(emu);
    let _ = emu.driver.as_ref().unwrap().turns.send(now);
    loop {
        let next = emu.driver.as_ref().map(|d| d.commands.recv());
        match next {
            Some(Ok(Command::Exec(work))) => work(emu)?,
            Some(Ok(Command::Resume(until))) => {
                let d = emu.driver.as_mut().unwrap();
                d.resume_at = until;
                d.in_turn = false;
                return Ok(());
            }
            // The test is done.
            Some(Err(_)) | None => return Err(Stop::Exit(0)),
        }
    }
}
