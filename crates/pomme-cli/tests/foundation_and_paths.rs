//! Foundation's string search and UIKit's Bézier paths, as apps use them.
//! Skipped without Xcode or the sysroot.

mod common;

fn console(name: &str) -> Option<String> {
    let mut emu = common::boot(name)?;
    emu.time_limit_ns = Some(2_000_000_000);
    let _ = emu.run_main();
    Some(emu.console.clone())
}

fn lines<'a>(console: &'a str, prefix: &str) -> Vec<&'a str> {
    console.lines().filter(|l| l.starts_with(prefix)).collect()
}

/// `range(of:options:)` honours its options: regular expressions (Expo
/// parses colors with one), case folding, backwards and anchored search;
/// replacing, with empty matches and templates.
#[test]
fn string_search_options() {
    let Some(out) = console("stringsearch") else { return };
    assert_eq!(
        lines(&out, "FOUND "),
        [
            "FOUND hex 0:ffa500FF",
            "FOUND digits 3:22",
            "FOUND nohex none",
            "FOUND caseless 2:ll",
            "FOUND backwards 3:.",
            "FOUND anchored none",
            "FOUND literal 1:.",
            "FOUND replaced x#y#",
            "FOUND empty -a-b-",
            "FOUND template Smith, John",
            "FOUND nonoverlapping ba",
            "FOUND contains true false",
        ],
        "{out}"
    );
}

/// UIBezierPath strokes with its line width and round caps, fills an oval,
/// and outlines a rounded rectangle; its bounds and current point.
#[test]
fn bezier_paths_draw() {
    let Some(out) = console("bezierpath") else { return };
    let px = |name: &str| -> u8 {
        let line = out.lines().find(|l| l.starts_with(&format!("PIXEL {name} "))).unwrap_or_else(|| panic!("no {name}\n{out}"));
        line.rsplit(' ').next().unwrap().parse().unwrap()
    };
    for inked in ["cap", "line", "oval", "box-edge"] {
        assert!(px(inked) > 200, "{inked}: {}\n{out}", px(inked));
    }
    for clear in ["beyond-cap", "cap-corner", "above-line", "oval-corner", "box-inside", "box-corner"] {
        assert!(px(clear) < 60, "{clear}: {}\n{out}", px(clear));
    }
    assert!(out.contains("BOUNDS oval 30.0 40.0 40.0 20.0\n"), "{out}");
    assert!(out.contains("BOUNDS line 20.0 20.0 60.0 0.0 at 80.0 20.0\n"), "{out}");
}
