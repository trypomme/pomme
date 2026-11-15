//! Tab bars, held to iOS 26's in both designs. The tabbar fixture (a tab
//! bar controller with three tabs, a badge on the third, the second selected
//! after a second) reports the bar and a tab's safe area; pomme's tab bar
//! puts each title where iOS does. fixtures/tabbar/ios26.txt is Liquid
//! Glass, ios26-compat.txt the design before (UIDesignRequiresCompatibility);
//! measured/ has everything the probe printed there, accessibility included.
//! Skipped without Xcode or the sysroot.

mod common;

use pomme_hle::accessibility::{Action, Node, Role};
use pomme_hle::host::{Event, Frame, Host, Idle};
use std::sync::{Arc, Mutex};

/// The recorded reports: (name, [x, y, width, height]).
fn recorded(file: &str) -> Vec<(String, [f64; 4])> {
    let path = format!("{}/tests/fixtures/tabbar/{file}", env!("CARGO_MANIFEST_DIR"));
    common::parse_metrics(&std::fs::read_to_string(path).unwrap()).into_iter().collect()
}

fn boot(compat: bool) -> Option<pomme_hle::Emu> {
    let mut emu = common::boot("tabbar")?;
    emu.system.requires_compatibility = compat;
    Some(emu)
}

#[test]
fn bar_and_titles_are_where_ios_puts_them() {
    for (compat, file) in [(false, "ios26.txt"), (true, "ios26-compat.txt")] {
        let Some(mut emu) = boot(compat) else { return };
        common::views(&mut emu, 3.0);
        let ours = common::parse_metrics(&emu.console);
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        let mut wrong = Vec::new();
        for (name, want) in recorded(file) {
            // The bar and the safe area as the probe reports them; titles from the bar.
            // (Titles: pomme draws SF text in Inter, a little narrower or
            // wider, so a title's centre and height are held to iOS's, its
            // width more loosely.)
            let (got, tolerance) = if let Some(title) = name.strip_prefix("second.text.") {
                match look.buttons.iter().find(|b| b.title == title) {
                    Some(b) => {
                        let [x, y, w, h] = b.title_frame;
                        let mid = x + w / 2.0;
                        let want_mid = want[0] + want[2] / 2.0;
                        if (mid - want_mid).abs() > 1.0 || (y - want[1]).abs() > 0.5 || (h - want[3]).abs() > 0.5 || (w - want[2]).abs() > 3.0 {
                            wrong.push(format!("{name}: {:?}, iOS {want:?}", b.title_frame));
                        }
                        continue;
                    }
                    None => continue, // the badge's text: checked below
                }
            } else if name.starts_with("second.tab") {
                continue; // the buttons' accessibility frames: tested with the screen reader
            } else if name.starts_with("second.") {
                match ours.get(&name) {
                    Some(&g) => (g, 0.5),
                    None => {
                        wrong.push(format!("{name}: not reported"));
                        continue;
                    }
                }
            } else {
                continue;
            };
            for (i, what) in ["x", "y", "width", "height"].iter().enumerate() {
                if (got[i] - want[i]).abs() > tolerance {
                    wrong.push(format!("{name}: {what} {:.2}, iOS {:.2}", got[i], want[i]));
                }
            }
        }
        // The badge: on the third tab, at its image's top right.
        let badge = look.buttons[2].badge.as_ref().expect("a badge");
        let want = recorded(file).into_iter().find(|(n, _)| n == "second.text.3").unwrap().1;
        let (bx, by, bw, bh) = (badge.frame[0], badge.frame[1], badge.frame[2], badge.frame[3]);
        if (bx + bw / 2.0 - (want[0] + want[2] / 2.0)).abs() > 1.5 || (by + bh / 2.0 - (want[1] + want[3] / 2.0)).abs() > 1.5 {
            wrong.push(format!("badge at {:?}, iOS's text at {want:?}", badge.frame));
        }
        assert!(wrong.is_empty(), "{}: differences from iOS 26:\n  {}", if compat { "before iOS 26" } else { "Liquid Glass" }, wrong.join("\n  "));
        // The second tab is selected (the probe selected it), and its screen shows.
        assert!(look.buttons[1].selected && !look.buttons[0].selected);
        assert_eq!(look.glass, !compat);
        assert!(emu.describe_views().contains("Search screen"));
    }
}

/// A host whose screen reader is listening: keeps the trees, takes its steps.
struct ScreenReader {
    trees: Arc<Mutex<Vec<Vec<Node>>>>,
    steps: Vec<Box<dyn FnMut(&[Node]) -> Option<Event> + Send>>,
    seen: usize,
}

impl Host for ScreenReader {
    fn idle(&mut self, now: u64, _next: Option<u64>) -> Idle {
        let trees = self.trees.lock().unwrap();
        if trees.len() > self.seen
            && let Some(step) = self.steps.first_mut()
            && let Some(e) = step(trees.last().unwrap())
        {
            self.seen = trees.len();
            drop(self.steps.remove(0));
            return Idle::Input { at_ns: now, events: vec![e] };
        }
        Idle::Advance
    }
    fn frame(&mut self, _: Frame<'_>) {}
    fn wants_accessibility(&mut self) -> bool {
        true
    }
    fn accessibility(&mut self, tree: &[Node]) {
        self.trees.lock().unwrap().push(tree.to_vec());
    }
}

fn tabs(tree: &[Node]) -> Vec<&Node> {
    tree.iter().filter(|n| n.role == Role::Tab).collect()
}

/// VoiceOver finds a tab bar of three tabs, the selected one selected, by
/// their titles (before iOS 26 the badge is the third's value, "3 items",
/// as measured); activating one switches to it.
#[test]
fn the_screen_reader_sees_and_switches_tabs() {
    for compat in [false, true] {
        let Some(mut emu) = boot(compat) else { return };
        let trees = Arc::new(Mutex::new(Vec::new()));
        // Once the probe has selected Search (at ~1 s), activate Settings.
        let step = Box::new(|tree: &[Node]| {
            let t = tabs(tree);
            let search = t.iter().find(|n| n.label.as_deref() == Some("Search"))?;
            if !search.selected() {
                return None;
            }
            let settings = t.iter().find(|n| n.label.as_deref() == Some("Settings"))?;
            Some(Event::Accessibility { id: settings.id, action: Action::Activate })
        });
        *emu.live.host = Some(Box::new(ScreenReader { trees: trees.clone(), steps: vec![step], seen: 0 }));
        emu.live.scale = 1.0;
        common::views(&mut emu, 5.0);
        let trees = trees.lock().unwrap();
        let first = &trees[0];
        let bar = first.iter().find(|n| n.role == Role::TabBar).expect("a tab bar");
        assert_eq!(bar.label.as_deref(), Some("Tab Bar"));
        let t = tabs(first);
        let labels: Vec<_> = t.iter().map(|n| n.label.as_deref().unwrap_or("")).collect();
        assert_eq!(labels, ["Home", "Search", "Settings"]);
        assert!(t.iter().all(|n| n.parent == Some(bar.id)));
        assert!(t[0].selected() && !t[1].selected());
        assert_eq!(t[2].value.as_deref(), if compat { Some("3 items") } else { None });
        // The buttons' frames: where iOS's are (y in the window: the bar's at 761).
        let want = if compat { [262.0, 762.0, 126.0, 48.0] } else { [234.0, 765.0, 94.0, 54.0] };
        assert_eq!(t[2].frame, want, "compat {compat}");
        let last = trees.last().unwrap();
        let settings = tabs(last).into_iter().find(|n| n.label.as_deref() == Some("Settings")).unwrap();
        assert!(settings.selected(), "activating Settings selected it");
        assert!(last.iter().any(|n| n.label.as_deref() == Some("Settings screen")));
    }
}

// ---- The probe's other cases (PROBE_VARIANT), each recorded on its own ----

/// Run the probe in a case, in a design; its console and the tab bar's look.
fn run_variant(variant: &str, compat: bool) -> Option<(pomme_hle::Emu, String)> {
    let mut emu = boot(compat)?;
    // (The probe turns its window dark on iOS; pomme's appearance is the
    // system's, which looks the same: it starts dark.)
    emu.appearance.dark = variant.contains("dark");
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", variant);
    common::views(&mut emu, 4.5);
    let console = emu.console.clone();
    Some((emu, console))
}

/// The recorded file for a case.
fn recording(variant: &str, compat: bool) -> String {
    format!("ios26{}-{variant}.txt", if compat { "-compat" } else { "" })
}

/// The probe's reports that aren't frames, as iOS printed them (fixtures/tabbar/measured).
fn measured_line(variant: &str, compat: bool, prefix: &str) -> String {
    let path = format!("{}/tests/fixtures/tabbar/measured/tabbar-{}-{variant}.log", env!("CARGO_MANIFEST_DIR"), if compat { "compat" } else { "glass" });
    std::fs::read_to_string(path).unwrap().lines().find(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix}")).to_string()
}

fn line<'a>(console: &'a str, prefix: &str) -> &'a str {
    console.lines().find(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no {prefix} in\n{console}"))
}

/// The bar and safe-area reports, as recorded.
fn reports_match(console: &str, file: &str) -> Vec<String> {
    let ours = common::parse_metrics(console);
    let mut wrong = Vec::new();
    for (name, want) in recorded(file) {
        if !(name.ends_with(".bar") || name.ends_with(".insets")) {
            continue;
        }
        match ours.get(&name) {
            Some(got) if got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 0.5) => {}
            got => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
        }
    }
    wrong
}

/// Seven tabs: four and More on the bar, as recorded (frames and all); the
/// seventh selected sits on the More navigation controller over its list, as
/// on iOS; the list has the other three, the badge with Settings.
#[test]
fn more_than_five_tabs_have_more() {
    for compat in [false, true] {
        let Some((mut emu, console)) = run_variant("more", compat) else { return };
        let wrong = reports_match(&console, &recording("more", compat));
        assert!(wrong.is_empty(), "compat {compat}: {}", wrong.join("\n"));
        assert_eq!(line(&console, "MORE "), measured_line("more", compat, "MORE "));
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        let titles: Vec<&str> = look.buttons.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["Home", "Search", "Library", "Favourites", "More"]);
        // The More tab is selected (its list showing, at the end).
        assert!(look.buttons[4].selected && look.buttons.iter().filter(|b| b.selected).count() == 1);
        let xs: Vec<f64> = look.buttons.iter().map(|b| b.frame[0]).collect();
        let want = if compat { [2.0, 80.0, 158.0, 236.0, 314.0] } else { [25.0, 89.2, 153.3, 217.5, 291.0] };
        assert!(xs.iter().zip(want).all(|(x, w)| (x - w).abs() <= 1.5), "compat {compat}: {xs:?}");
        let views = emu.describe_views();
        let list_view = pomme_hle::tabbar::more_lists(&emu).into_iter().next().expect("a More list");
        let list = pomme_hle::tabbar::more_list(&mut emu, list_view).unwrap().unwrap();
        let rows: Vec<(&str, Option<&str>)> = list.rows.iter().map(|r| (r.title.as_str(), r.badge.as_deref())).collect();
        assert_eq!(rows, [("Settings", Some("3")), ("Downloads", None), ("History", None)], "{views}");
        // Rows from under the navigation bar: 52 pt under Liquid Glass, 44 before (measured).
        let want_top = if compat { 91.0 } else { 101.0 };
        assert_eq!(list.rows[0].frame[1], want_top);
        assert_eq!(list.rows[1].frame[1] - list.rows[0].frame[1], if compat { 44.0 } else { 52.0 });
    }
}

/// A screen pushed with hidesBottomBarWhenPushed hides the bar (its safe
/// area only the home indicator's); popping brings it back.
#[test]
fn pushed_screens_can_hide_the_bar() {
    for compat in [false, true] {
        let Some((_, console)) = run_variant("push", compat) else { return };
        let wrong = reports_match(&console, &recording("push", compat));
        assert!(wrong.is_empty(), "compat {compat}: {}", wrong.join("\n"));
        assert_eq!(line(&console, "PUSHED "), measured_line("push", compat, "PUSHED "));
        assert_eq!(line(&console, "POPPED "), measured_line("push", compat, "POPPED "));
    }
}

/// On its side: the compact bar (64 pt under Liquid Glass, 52 before), image
/// beside title, the buttons where iOS puts them.
#[test]
fn on_its_side_the_bar_is_compact() {
    for compat in [false, true] {
        let Some((mut emu, console)) = run_variant("landscape", compat) else { return };
        let wrong = reports_match(&console, &recording("landscape", compat));
        assert!(wrong.is_empty(), "compat {compat}: {}", wrong.join("\n"));
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        // (Recorded: fixtures/tabbar/measured/tabbar-*-landscape.log. Widths
        // follow the titles, which pomme sets in Inter, so a little room.)
        let want: &[[f64; 2]] = if compat { &[[81.0, 220.0], [305.0, 222.0], [531.0, 230.0]] } else { &[[286.0, 83.7], [373.7, 86.0], [463.7, 94.7]] };
        for (b, w) in look.buttons.iter().zip(want) {
            assert!((b.frame[0] - w[0]).abs() <= 4.0 && (b.frame[2] - w[1]).abs() <= 4.0, "compat {compat}: {:?}, iOS {w:?}", b.frame);
            assert!(b.image_frame[0] + b.image_frame[2] < b.title_frame[0], "image beside title");
        }
    }
}

/// Dark: Liquid Glass's unselected tabs white, the tint dark mode's blue; before, grey.
#[test]
fn dark() {
    for compat in [false, true] {
        let Some((mut emu, console)) = run_variant("dark", compat) else { return };
        let wrong = reports_match(&console, &recording("dark", compat));
        assert!(wrong.is_empty(), "compat {compat}: {}", wrong.join("\n"));
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        assert!(look.dark);
        let rgb = |c: [f64; 4]| c.map(|v| (v * 255.0).round() as u8);
        assert_eq!(rgb(look.buttons[1].color)[..3], [0, 145, 255], "the selected tab, dark blue");
        assert_eq!(rgb(look.buttons[0].color)[..3], if compat { [146, 146, 146] } else { [255, 255, 255] });
    }
}

/// Colours from the bar's appearance (as React Native's native tabs set
/// them): icons and titles by state, badges; under Liquid Glass an
/// unselected title keeps its own colour (measured).
#[test]
fn appearance_colours() {
    for compat in [false, true] {
        let Some((mut emu, _)) = run_variant("appearance", compat) else { return };
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        let rgb = |c: [f64; 4]| [c[0], c[1], c[2]].map(|v| (v * 255.0).round() as u8);
        let (normal, selected) = (&look.buttons[0], &look.buttons[1]);
        assert!(selected.selected);
        assert_eq!(rgb(normal.color), [255, 56, 60], "compat {compat}: red icons");
        assert_eq!(rgb(selected.color), [203, 48, 224], "compat {compat}: a purple selected icon");
        assert_eq!(rgb(selected.title_color), [255, 141, 40], "compat {compat}: an orange selected title");
        let want_title = if compat { [52, 199, 89] } else { [25, 25, 25] };
        assert_eq!(rgb(normal.title_color), want_title, "compat {compat}: unselected titles");
        let badge = look.buttons[2].badge.as_ref().unwrap();
        assert_eq!(rgb(badge.color), [0, 136, 255], "compat {compat}: blue badges");
    }
}

/// iOS 18's tabs (`UITab`s), as React Native's native tabs give them from
/// iOS 26.1: the tabs' controllers, titles and badges, the delegate told of
/// each new tab (a program's choice too), no `viewControllers`; and a search
/// tab, under Liquid Glass on a circle of its own at the right, the capsule
/// beside it from the left; before, an ordinary last tab.
#[test]
fn uitabs_and_a_search_tab() {
    for compat in [false, true] {
        let Some((mut emu, console)) = run_variant("uitab", compat) else { return };
        for prefix in ["UITAB controllers", "UITAB items", "UITAB after", "DELEGATE didSelectTab home", "DELEGATE didSelectTab com.apple"] {
            assert_eq!(line(&console, prefix), measured_line("uitab", compat, prefix), "compat {compat}");
        }
        assert!(!console.contains("DELEGATE should"), "nobody tapped: {console}");
        let wrong = reports_match(&console, &recording("uitab", compat));
        assert!(wrong.is_empty(), "compat {compat}: {wrong:#?}");
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        assert_eq!(look.buttons.len(), 4);
        let search = &look.buttons[3];
        assert!(search.selected && search.title == "Search");
        if compat {
            assert!(look.search_platter.is_none() && !search.detached);
            continue;
        }
        assert_eq!(look.platter, Some([21.0, 0.0, 278.0, 62.0]));
        assert_eq!(look.search_platter, Some([307.0, 0.0, 62.0, 62.0]));
        assert!(search.detached);
        let [x, y, w, h] = search.image_frame;
        assert!(((x + w / 2.0) - 338.0).abs() < 0.5 && ((y + h / 2.0) - 31.0).abs() < 0.5, "the magnifying glass in the middle: {:?}", search.image_frame);
        // (Measured 94.7-pt buttons from 25, 112.7, 200.3; pomme's are 95.)
        for (b, want) in look.buttons.iter().zip([25.0, 112.7, 200.3]) {
            assert!((b.frame[0] - want).abs() <= 0.5, "{:?} at {want}", b.frame);
        }
    }
    // Two other tabs fit: the capsule its own size, from the left. Four don't: it
    // fills to the circle, its buttons narrower (a long title's wider).
    for (variant, platter, xs) in [("uitab,two", [21.0, 0.0, 188.0, 62.0], &[25.0, 111.0][..]), ("uitab,four", [21.0, 0.0, 278.0, 62.0], &[25.0, 84.7, 152.0, 211.7][..])] {
        let Some((mut emu, _)) = run_variant(variant, false) else { return };
        let bar = pomme_hle::tabbar::bars(&emu)[0];
        let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
        assert_eq!(look.platter, Some(platter), "{variant}");
        for (b, &want) in look.buttons.iter().zip(xs) {
            assert!((b.frame[0] - want).abs() <= 0.5, "{variant}: {:?} at {want}", b.frame);
        }
    }
}
