//! UIKit: the application object and `UIApplicationMain`.

use crate::objc::{self, send};
use crate::{Emu, HostFn, Result, Stop, foundation, swift};

#[derive(Debug, Default)]
pub struct State {
    pub app: u64,
    pub delegate: u64,
    /// Set once `application:didFinishLaunchingWithOptions:` returns.
    pub launched: bool,
}

/// The body of `UIApplicationMain`: create the app and its delegate, deliver
/// the launch callbacks, then (eventually) run the event loop.
fn application_main(emu: &mut Emu, principal: Option<String>, delegate: Option<String>) -> Result<()> {
    let principal = principal.unwrap_or_else(|| "UIApplication".into());
    let app_cls = class_named(emu, &principal)?;
    let app = send(emu, app_cls, "alloc", &[])?;
    let app = send(emu, app, "init", &[])?;
    emu.ui.app = app;

    if let Some(name) = delegate {
        let cls = class_named(emu, &name)?;
        let d = send(emu, cls, "alloc", &[])?;
        let d = send(emu, d, "init", &[])?;
        send(emu, app, "setDelegate:", &[d])?;
    }
    let d = emu.ui.delegate;
    for sel in ["application:willFinishLaunchingWithOptions:", "application:didFinishLaunchingWithOptions:"] {
        let s = objc::intern(emu, sel)?;
        if objc::responds(emu, d, s)? {
            send(emu, d, sel, &[app, 0])?;
        }
    }
    emu.ui.launched = true;
    Err(Stop::Unsupported(
        "UIApplicationMain: launch finished (didFinishLaunchingWithOptions returned); scene connection and the event loop aren't implemented yet".into(),
    ))
}

fn class_named(emu: &Emu, name: &str) -> Result<u64> {
    emu.objc.by_name.get(name).copied().ok_or_else(|| Stop::Unsupported(format!("no class named {name}")))
}

/// Swift overlay: `UIApplicationMain(_ argc: Int32, _ argv: …, _ principalClassName: String?, _ delegateClassName: String?) -> Int32`
fn swift_application_main(emu: &mut Emu) -> Result<()> {
    let principal = swift::read_optional_string(emu, emu.cpu.x[2], emu.cpu.x[3])?;
    let delegate = swift::read_optional_string(emu, emu.cpu.x[4], emu.cpu.x[5])?;
    application_main(emu, principal, delegate)
}

/// `int UIApplicationMain(int argc, char **argv, NSString *principal, NSString *delegate)`
fn c_application_main(emu: &mut Emu) -> Result<()> {
    let (p, d) = (emu.arg(2), emu.arg(3));
    let principal = if p == 0 { None } else { Some(foundation::nsstring(emu, p)?) };
    let delegate = if d == 0 { None } else { Some(foundation::nsstring(emu, d)?) };
    application_main(emu, principal, delegate)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_$s5UIKit17UIApplicationMainys5Int32VAD_SpySpys4Int8VGGSgSSSgAJtF", swift_application_main),
    ("_UIApplicationMain", c_application_main),
];

// ---- UIApplication ----

fn shared_application(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.app);
    Ok(())
}

fn delegate(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.ui.delegate);
    Ok(())
}

fn set_delegate(emu: &mut Emu) -> Result<()> {
    emu.ui.delegate = emu.arg(2);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIApplication", true, "sharedApplication", shared_application),
    ("UIApplication", false, "delegate", delegate),
    ("UIApplication", false, "setDelegate:", set_delegate),
];
