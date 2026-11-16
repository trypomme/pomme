//! Relaunching the app within a run, as quitting and reopening it on iOS
//! does (`XCUIApplication.terminate()` then `launch()`): it starts again from
//! the beginning, launched as the first time was (the same arguments and
//! URL), and keeps what it saved: its data container, its defaults (written
//! to `Library/Preferences` first, as iOS keeps them) and its keychain items.
//! Everything else starts fresh: memory, threads, the screen, virtual time.
//!
//! The app's run is on pomme's own stack, so a relaunch unwinds it: the
//! request saves what's kept and stops with [`Stop::Relaunch`];
//! [`Emu::run_main_relaunching`] catches that, puts back the phone as it was
//! before the first launch (a snapshot) with the kept data, and runs again.

use crate::{Emu, Result, Stop};

/// What a relaunch keeps.
#[derive(Clone, Default)]
pub(crate) struct Kept {
    container: Option<crate::fs::Container>,
    keychain: Vec<crate::keychain::Saved>,
}

/// Relaunch the app (on its main thread): save what's kept and unwind.
pub(crate) fn request(emu: &mut Emu) -> Result<()> {
    crate::userdefaults::save_files(emu)?;
    let keychain = crate::keychain::save(emu)?;
    let container = crate::fs::take_container(emu);
    emu.kept = Some(Kept { container, keychain });
    Err(Stop::Relaunch)
}

/// After the unwind: the phone as before the first launch, with the kept data.
fn start_again(emu: &mut Emu, boot: &crate::snapshot::Snapshot) -> Result<()> {
    let kept = emu.kept.take().unwrap_or_default();
    // (Nothing of the old run is on pomme's stack any more.)
    emu.frames.clear();
    // (What the app printed stays printed, as in Xcode's console.)
    let console = std::mem::take(&mut emu.console);
    emu.restore(boot).map_err(|r| Stop::Unsupported(format!("can't relaunch: {r}")))?;
    emu.console = console;
    crate::fs::put_container(emu, kept.container);
    crate::keychain::keep(emu, kept.keychain);
    crate::driver::relaunched(emu);
    Ok(())
}

impl Emu {
    /// Run the app as [`Emu::run_main`] does, starting it again whenever it's
    /// relaunched (a test's [`crate::driver::App::relaunch`], the window's
    /// Relaunch), keeping what it saved.
    pub fn run_main_relaunching(&mut self) -> Result<i32> {
        let boot = self.snapshot().map_err(|r| Stop::Unsupported(format!("can't keep the phone for a relaunch: {r}")))?;
        loop {
            match self.run_main() {
                Err(Stop::Relaunch) => start_again(self, &boot)?,
                other => return other,
            }
        }
    }
}
