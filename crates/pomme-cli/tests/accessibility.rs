//! A host's screen reader (VoiceOver in the browser or on the desktop): the
//! live host gets the screen's accessibility tree as it changes and acts on
//! its elements, on the accessibility fixture (tests/fixtures/accessibility).

mod common;

use pomme_hle::accessibility::{Action, Node, Role};
use pomme_hle::host::{Event, Frame, Host, Idle};
use std::sync::{Arc, Mutex};

/// A step: once the latest tree shows something, what the screen reader does next.
type Step = Box<dyn FnMut(&[Node]) -> Option<Event> + Send>;

/// A host whose screen reader is listening: keeps every tree it's given and
/// takes its steps in turn as the trees allow.
struct ScreenReader {
    trees: Arc<Mutex<Vec<Vec<Node>>>>,
    steps: Vec<Step>,
    /// Trees seen when the last step was taken (the next waits for a newer one).
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

fn find<'a>(tree: &'a [Node], label: &str) -> Option<&'a Node> {
    tree.iter().find(|n| n.label.as_deref() == Some(label))
}

/// Act on the element labelled `label` once it's on screen.
fn act(label: &'static str, action: Action) -> Step {
    Box::new(move |tree| find(tree, label).map(|n| Event::Accessibility { id: n.id, action }))
}

/// The tree as text: each node's role, label and value, indented under its parent.
fn outline(tree: &[Node]) -> String {
    let mut s = String::new();
    for n in tree {
        let mut depth = 0;
        let mut p = n.parent;
        while let Some(id) = p {
            depth += 1;
            p = tree.iter().find(|m| m.id == id).and_then(|m| m.parent);
        }
        s += &format!("{}{:?}", "  ".repeat(depth), n.role);
        if let Some(l) = &n.label {
            s += &format!(" {l:?}");
        }
        if let Some(v) = &n.value {
            s += &format!(" = {v:?}");
        }
        s += "\n";
    }
    s
}

#[test]
fn the_host_reads_and_acts_on_the_screen() {
    let Some(mut emu) = common::boot("accessibility") else {
        return;
    };
    let trees = Arc::new(Mutex::new(Vec::new()));
    let steps: Vec<Step> = vec![
        act("Tap me", Action::Activate),
        act("Volume", Action::Increment),
        act("Activated 0 times", Action::Activate),
        act("Show alert", Action::Activate),
    ];
    emu.live.host = Some(Box::new(ScreenReader { trees: trees.clone(), steps, seen: 0 }));
    emu.live.scale = 1.0;
    common::views(&mut emu, 3.0);
    let trees = trees.lock().unwrap();
    for t in trees.iter() {
        eprintln!("{}", outline(t));
    }
    let first = &trees[0];
    assert_eq!(
        outline(first),
        "Window\n  Heading \"Accessibility\"\n  Button \"Tap me\"\n  StaticText \"Tapped 0 times\"\n  Group \"Settings\"\n    Switch \"Wi-Fi\" = \"1\"\n    Adjustable \"Volume\" = \"50%\"\n  TextField \"Your name\"\n  Button \"Activated 0 times\"\n  Button \"Show alert\"\n"
    );
    let tap = find(first, "Tap me").unwrap();
    assert_eq!(tap.identifier.as_deref(), Some("tap"));
    assert_eq!(tap.hint.as_deref(), Some("Counts taps"));
    assert_eq!(tap.frame, [20.0, 110.0, 160.0, 44.0]);
    assert_eq!(tap.activation_point, [100.0, 132.0]);
    assert!(tap.is_element() && tap.enabled());
    assert!(find(first, "Wi-Fi").unwrap().on());
    assert!(find(first, "Decoration").is_none(), "hidden from accessibility");
    let window = &first[0];
    assert_eq!((window.role, window.parent), (Role::Window, None));

    // Each action shows in a later tree: a tap at the button, the slider a
    // tenth up, the element's own activation, then the modal view alone.
    let last = trees.last().unwrap();
    assert!(trees.iter().any(|t| find(t, "Tapped 1 times").is_some()), "the tap reached the button");
    assert!(trees.iter().any(|t| find(t, "Volume").and_then(|n| n.value.as_deref()) == Some("60%")), "the slider went up");
    assert!(trees.iter().any(|t| find(t, "Activated 1 times").is_some()), "accessibilityActivate handled it");
    assert_eq!(outline(last), "Window\n  StaticText \"Are you sure?\"\n  Button \"OK\"\n");
}
