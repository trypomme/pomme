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

/// Run a fixture, activating a tab; the trees the screen reader saw, and the tabs' badges.
fn run(fixture: &str, activate: &'static str) -> Option<(Vec<Vec<Node>>, Vec<Option<String>>)> {
    run_look(fixture, activate).map(|(t, look)| (t, look.buttons.iter().map(|b| b.badge.as_ref().map(|b| b.text.clone())).collect()))
}

/// Run a fixture, activating a tab; the trees the screen reader saw, and the bar as drawn.
fn run_look(fixture: &str, activate: &'static str) -> Option<(Vec<Vec<Node>>, pomme_hle::tabbar::Look)> {
    let mut emu = common::boot(fixture)?;
    let trees = Arc::new(Mutex::new(Vec::new()));
    *emu.live.host = Some(Box::new(ScreenReader { trees: trees.clone(), activate: Some(activate), seen: 0 }));
    emu.live.scale = 1.0;
    common::views(&mut emu, 4.0);
    let t = trees.lock().unwrap().clone();
    let bar = pomme_hle::tabbar::bars(&emu)[0];
    let look = pomme_hle::tabbar::look(&mut emu, bar).unwrap().unwrap();
    Some((t, look))
}

fn says(tree: &[Node], text: &str) -> bool {
    tree.iter().any(|n| n.label.as_deref().is_some_and(|l| l.contains(text)))
}

fn tab_labels(tree: &[Node]) -> Vec<String> {
    tree.iter().filter(|n| n.role == Role::Tab).filter_map(|n| n.label.clone()).collect()
}

#[test]
fn ios_18_tabs_by_value() {
    let Some((trees, badges)) = run("swiftuitabs", "Search") else { return };
    // (Search comes from a ForEach in a Group; Settings has `.badge(3)`.)
    assert_eq!(badges, [None, None, Some("3".to_string()), None]);
    let first = &trees[0];
    assert_eq!(tab_labels(first), ["Home", "Search", "Settings", "Find"]);
    // The first tab: its navigation stack, with its title in the bar.
    assert!(says(first, "Home screen, home selected"), "{first:#?}");
    assert!(first.iter().any(|n| n.role == Role::Heading && n.label.as_deref() == Some("Home")));
    let last = trees.last().unwrap();
    assert!(says(last, "Search screen, search selected"), "the binding followed the tab: {last:#?}");
    let search = last.iter().find(|n| n.role == Role::Tab && n.label.as_deref() == Some("Search")).unwrap();
    assert!(search.selected());
}

/// `Tab(role: .search)` is UIKit's search tab: under Liquid Glass on a
/// circle of its own (tests/tab_bar.rs), and choosing it selects its value.
#[test]
fn the_search_tab() {
    let Some((trees, look)) = run_look("swiftuitabs", "Find") else { return };
    let find = look.buttons.last().unwrap();
    assert_eq!(find.title, "Find");
    assert!(find.detached && find.selected, "{find:?}");
    assert!(look.search_platter.is_some());
    let last = trees.last().unwrap();
    assert!(says(last, "Find screen, find selected"), "{last:#?}");
}

#[test]
fn tab_items_and_tags() {
    let Some((trees, badges)) = run("swiftuitabitems", "Second") else { return };
    assert_eq!(badges, [None, Some("New".to_string())]);
    let first = &trees[0];
    assert_eq!(tab_labels(first), ["First", "Second"]);
    assert!(says(first, "First screen, tab 1"));
    let last = trees.last().unwrap();
    assert!(says(last, "Second screen, tab 2"), "{last:#?}");
}
