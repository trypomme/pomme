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
