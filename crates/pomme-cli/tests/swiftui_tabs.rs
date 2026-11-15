//! SwiftUI's TabView in pomme's tab bar, both ways apps write it: iOS 18's
//! `Tab`s by value (tests/fixtures/swiftuitabs, its first tab a
//! NavigationStack) and views with `.tabItem` and `.tag`
//! (tests/fixtures/swiftuitabitems). The tabs' titles reach the bar,
//! choosing one (as VoiceOver does, a tap at it) shows it and updates the
//! selection binding. Skipped without Xcode or the sysroot.

mod common;

use pomme_hle::accessibility::{Action, Node, Role};
use pomme_hle::host::{Event, Frame, Host, Idle};
use std::sync::{Arc, Mutex};

/// A host whose screen reader activates the tab with this label once it's there.
struct ScreenReader {
    trees: Arc<Mutex<Vec<Vec<Node>>>>,
    activate: Option<&'static str>,
    seen: usize,
}

impl Host for ScreenReader {
    fn idle(&mut self, now: u64, _next: Option<u64>) -> Idle {
        let trees = self.trees.lock().unwrap();
        if trees.len() > self.seen
            && let Some(label) = self.activate
            && let Some(tab) = trees.last().unwrap().iter().find(|n| n.role == Role::Tab && n.label.as_deref() == Some(label))
        {
            self.seen = trees.len();
            self.activate = None;
            return Idle::Input { at_ns: now, events: vec![Event::Accessibility { id: tab.id, action: Action::Activate }] };
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

/// Run a fixture, activating a tab; the trees the screen reader saw.
fn run(fixture: &str, activate: &'static str) -> Option<Vec<Vec<Node>>> {
    let mut emu = common::boot(fixture)?;
    let trees = Arc::new(Mutex::new(Vec::new()));
    *emu.live.host = Some(Box::new(ScreenReader { trees: trees.clone(), activate: Some(activate), seen: 0 }));
    emu.live.scale = 1.0;
    common::views(&mut emu, 4.0);
    let t = trees.lock().unwrap().clone();
    Some(t)
}

fn says(tree: &[Node], text: &str) -> bool {
    tree.iter().any(|n| n.label.as_deref().is_some_and(|l| l.contains(text)))
}

fn tab_labels(tree: &[Node]) -> Vec<String> {
    tree.iter().filter(|n| n.role == Role::Tab).filter_map(|n| n.label.clone()).collect()
}

#[test]
fn ios_18_tabs_by_value() {
    let Some(trees) = run("swiftuitabs", "Search") else { return };
    let first = &trees[0];
    assert_eq!(tab_labels(first), ["Home", "Search", "Settings"]);
    // The first tab: its navigation stack, with its title in the bar.
    assert!(says(first, "Home screen, home selected"), "{first:#?}");
    assert!(first.iter().any(|n| n.role == Role::Heading && n.label.as_deref() == Some("Home")));
    let last = trees.last().unwrap();
    assert!(says(last, "Search screen, search selected"), "the binding followed the tab: {last:#?}");
    let search = last.iter().find(|n| n.role == Role::Tab && n.label.as_deref() == Some("Search")).unwrap();
    assert!(search.selected());
}

#[test]
fn tab_items_and_tags() {
    let Some(trees) = run("swiftuitabitems", "Second") else { return };
    let first = &trees[0];
    assert_eq!(tab_labels(first), ["First", "Second"]);
    assert!(says(first, "First screen, tab 1"));
    let last = trees.last().unwrap();
    assert!(says(last, "Second screen, tab 2"), "{last:#?}");
}
