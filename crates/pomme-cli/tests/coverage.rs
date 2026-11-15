//! The coverage report (pomme_hle::coverage) on an app that reaches one of
//! each kind of gap (tests/fixtures/coverage/main.m): a missing function, a
//! missing method, and a setter pomme stores but never reads. Built with the
//! iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::coverage::{Answer, Recorder, Status, report};

#[test]
fn records_every_gap_and_keeps_going() {
    let Some(mut emu) = common::boot("coverage") else { return };
    *emu.coverage = Some(Recorder::new(true));
    let v = common::views(&mut emu, 2.0);
    // Each miss answered 0 (nil) and the app carried on past all three.
    assert!(v.contains("\"after r=0 m=nil\""), "{v}");

    let r = report(&emu, Some("done".into()));
    let probe = r.symbols.iter().find(|s| s.symbol == "_pomme_coverage_probe").expect("the probe is bound");
    assert_eq!((probe.status, probe.calls), (Status::Missing, 2));
    // What a UIKit app can't do without.
    let msg_send = r.symbols.iter().find(|s| s.symbol == "_objc_msgSend").unwrap();
    assert_eq!(msg_send.status, Status::Implemented);
    assert!(msg_send.calls > 0);
    let view = r.symbols.iter().find(|s| s.symbol == "_OBJC_CLASS_$_UIView").unwrap();
    assert_eq!(view.status, Status::Data);

    let run = r.run.as_ref().unwrap();
    let f = run.missing_functions.iter().find(|f| f.symbol == "_pomme_coverage_probe").unwrap();
    assert_eq!(f.miss.count, 2);
    assert!(f.miss.first, "the probe is the run's first miss");
    assert!(f.miss.first_caller.contains("viewDidLoad"), "{}", f.miss.first_caller);
    let m = run.missing_methods.iter().find(|m| m.method.selector == "pommeCoverageProbe").unwrap();
    assert_eq!((m.method.class.as_str(), m.method.class_method, m.miss.count, m.miss.first), ("NSObject", false, 1, false));
    let p = run.permissive.iter().find(|p| p.method.selector == "setPommeCoverageProbe:").unwrap();
    assert_eq!((p.method.class.as_str(), p.answer), ("UIView", Answer::StoredUnused));
    // A setter whose value pomme reads (the field's text) isn't a gap.
    assert!(run.permissive.iter().all(|p| p.method.selector != "setText:" || p.answer != Answer::StoredUnused));

    let text = r.text(8);
    assert!(text.contains("_pomme_coverage_probe (<flat namespace>), first from"), "{text}");
    assert!(text.contains("-[NSObject pommeCoverageProbe]"), "{text}");
    assert!(text.contains("-[UIView setPommeCoverageProbe:]"), "{text}");
}

#[test]
fn stops_at_the_first_gap_without_keep_going() {
    let Some(mut emu) = common::boot("coverage") else { return };
    *emu.coverage = Some(Recorder::new(false));
    emu.time_limit_ns = Some(2_000_000_000);
    match emu.run_main() {
        Err(pomme_hle::Stop::MissingFunction { symbol, .. }) => assert_eq!(symbol, "_pomme_coverage_probe"),
        other => panic!("expected the probe to stop the run, got {other:?}"),
    }
    let r = report(&emu, Some("stopped".into()));
    assert_eq!(r.run.unwrap().missing_functions.len(), 1);
}
