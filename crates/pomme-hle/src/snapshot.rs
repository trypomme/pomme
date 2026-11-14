//! Snapshots: the whole phone (memory, CPU, threads, and every system
//! library's state) saved at a moment and put back later, so a test can boot
//! an app once and start each case from the same point, exactly.
//!
//! What's outside the phone isn't in a snapshot (see [`crate::External`]):
//! the window or browser showing it, the network, a web engine, a profiler,
//! a driving test. Restoring keeps the ones in use. So a snapshot can't be
//! taken while the app holds something that lives out there: an open network
//! connection, or pages in a web engine.
//!
//! While the app runs, part of its state is on pomme's own stack: host
//! functions that called into the app and wait for it to return (the main
//! run loop inside `UIApplicationMain`, a thread waiting in place). Restoring
//! is only possible under those same frames, so a snapshot records them and
//! [`Emu::restore`] refuses anywhere else. A driving test's turns are such a
//! place: a snapshot taken in one can be restored in any later turn of the
//! same run (see [`crate::driver::App::checkpoint`]).

use crate::Emu;

/// The phone at a moment.
pub struct Snapshot {
    emu: Box<Emu>,
}

impl Snapshot {
    /// The virtual time it was taken at (ns).
    pub fn at_ns(&self) -> u64 {
        crate::threads::now_ns(&self.emu)
    }
}

/// Why a snapshot can't be taken or restored here.
#[derive(Debug, Clone)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Refused {}

impl Emu {
    /// Save the phone as it is now.
    pub fn snapshot(&self) -> Result<Snapshot, Refused> {
        let connections = crate::sockets::host_connections(self);
        if connections > 0 {
            return Err(Refused(format!("the app has {connections} network connection(s) open, which the host holds: a snapshot can't hold them")));
        }
        let pages = crate::webkit::engine_pages(self);
        if pages > 0 {
            return Err(Refused(format!("the app has {pages} web page(s) in the host's web engine: a snapshot can't hold them")));
        }
        Ok(Snapshot { emu: Box::new(self.clone()) })
    }

    /// Put the phone back as it was when `snapshot` was taken. Only under the
    /// same host frames (see the module's documentation): anywhere else it's
    /// refused and nothing changes. What's outside the phone stays as it is,
    /// and so do the run's limits (`max_steps`, `time_limit_ns`, `trace`).
    pub fn restore(&mut self, snapshot: &Snapshot) -> Result<(), Refused> {
        if self.frames != snapshot.emu.frames {
            return Err(Refused(format!(
                "the app isn't where the snapshot was taken: pomme's stack has frames {:?}, the snapshot {:?} (restore in a later turn of the run it was taken in)",
                self.frames, snapshot.emu.frames
            )));
        }
        let host = self.live.host.take();
        let client = self.net.client.take();
        let engine = self.webkit.engine.take();
        let driver = self.driver.take();
        let profiler = self.profiler.take();
        let (max_steps, time_limit_ns, trace, serial) = (self.max_steps, self.time_limit_ns, self.trace, self.frame_serial);
        *self = (*snapshot.emu).clone();
        *self.live.host = host;
        *self.net.client = client;
        *self.webkit.engine = engine;
        *self.driver = driver;
        *self.profiler = profiler;
        (self.max_steps, self.time_limit_ns, self.trace) = (max_steps, time_limit_ns, trace);
        // Frames made from here on are new ones, whatever the snapshot's count.
        self.frame_serial = self.frame_serial.max(serial);
        // A window showing the phone shows it as it now is.
        crate::host::changed(self);
        Ok(())
    }
}
