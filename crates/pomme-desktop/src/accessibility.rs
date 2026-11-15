//! The app's accessibility tree on the window, for the system's screen
//! reader (VoiceOver, Narrator, NVDA, Orca) through AccessKit: macOS's
//! NSAccessibility, Windows' UI Automation, Linux's AT-SPI.
//!
//! The emulator hands the host the screen as VoiceOver presents it
//! (`pomme_hle::accessibility::tree`) whenever it changes, but only once a
//! screen reader is listening: AccessKit asks for the first tree when one
//! connects. Each element is placed where it's drawn in the window, and what
//! the screen reader does to it (its default action, increment, decrement,
//! focus) goes back to the app as VoiceOver's actions.

use accesskit::{Action as AkAction, ActionRequest, Node as AkNode, NodeId, Rect, Role as AkRole, Toggled, TreeId, TreeInfo, TreeUpdate};
use pomme_hle::accessibility::{Action, Node, Role};

/// The window itself (the app's nodes are its objects, never this).
pub const ROOT: NodeId = NodeId(u64::MAX);

/// Where the app's screen is in the window: its top-left corner in window
/// pixels, and window pixels per point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub k: f64,
}

/// The whole tree for AccessKit: the window, and the app's nodes in it.
/// `focus`: the element the screen reader last moved to, if it's still there.
pub fn update(title: &str, nodes: &[Node], place: Place, focus: Option<u64>) -> TreeUpdate {
    let mut out = Vec::with_capacity(nodes.len() + 1);
    let mut root = AkNode::new(AkRole::Window);
    root.set_label(title);
    root.set_children(nodes.iter().filter(|n| n.parent.is_none()).map(|n| NodeId(n.id)).collect::<Vec<_>>());
    out.push((ROOT, root));
    for n in nodes {
        let mut node = node(n, place);
        node.set_children(nodes.iter().filter(|c| c.parent == Some(n.id)).map(|c| NodeId(c.id)).collect::<Vec<_>>());
        out.push((NodeId(n.id), node));
    }
    let focus = focus.filter(|&f| nodes.iter().any(|n| n.id == f)).map_or(ROOT, NodeId);
    TreeUpdate { nodes: out, tree: Some(TreeInfo::new(ROOT)), tree_id: TreeId::ROOT, focus }
}

fn node(n: &Node, place: Place) -> AkNode {
    let role = match n.role {
        Role::Window => AkRole::GenericContainer,
        Role::Group => AkRole::Group,
        Role::List => AkRole::List,
        Role::TabBar => AkRole::TabList,
        Role::Tab => AkRole::Tab,
        Role::Button => AkRole::Button,
        Role::Link => AkRole::Link,
        Role::Switch => AkRole::Switch,
        Role::Adjustable => AkRole::Slider,
        Role::TextField => AkRole::TextInput,
        Role::SearchField => AkRole::SearchInput,
        Role::Heading => AkRole::Heading,
        Role::Image => AkRole::Image,
        // Text, and elements with no more specific trait: read as text.
        Role::StaticText | Role::Element => AkRole::Label,
    };
    let mut node = AkNode::new(role);
    let [x, y, w, h] = n.frame;
    let x0 = place.x + x * place.k;
    let y0 = place.y + y * place.k;
    node.set_bounds(Rect { x0, y0, x1: x0 + w.max(0.0) * place.k, y1: y0 + h.max(0.0) * place.k });
    match n.role {
        Role::StaticText | Role::Element => {
            // A label's text is its value; VoiceOver reads the label, then the value.
            let text: Vec<&str> = [n.label.as_deref(), n.value.as_deref()].into_iter().flatten().collect();
            node.set_value(text.join(", "));
        }
        _ => {
            if let Some(l) = &n.label {
                node.set_label(l.as_str());
            }
            if let Some(v) = &n.value
                && n.role != Role::Switch
            {
                node.set_value(v.as_str());
            }
        }
    }
    if let Some(h) = &n.hint {
        node.set_description(h.as_str());
    }
    if n.role == Role::Switch {
        node.set_toggled(Toggled::from(n.on()));
    }
    if n.role == Role::Adjustable
        && let Some(pct) = n.value.as_deref().and_then(|v| v.strip_suffix('%')).and_then(|v| v.trim().parse::<f64>().ok())
    {
        node.set_numeric_value(pct);
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(100.0);
    }
    if n.is_element() {
        if !n.enabled() {
            node.set_disabled();
        }
        if n.selected() {
            node.set_selected(true);
        }
        node.add_action(AkAction::Focus);
        node.add_action(AkAction::Click);
        if n.role == Role::Adjustable {
            node.add_action(AkAction::Increment);
            node.add_action(AkAction::Decrement);
        }
    }
    node
}

/// What a screen reader asked for, as VoiceOver's action on an element (none for the window).
pub fn action(req: &ActionRequest) -> Option<(u64, Action)> {
    if req.target_node == ROOT {
        return None;
    }
    let action = match req.action {
        AkAction::Click => Action::Activate,
        AkAction::Increment => Action::Increment,
        AkAction::Decrement => Action::Decrement,
        AkAction::Focus => Action::Focus,
        _ => return None,
    };
    Some((req.target_node.0, action))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: u64, parent: Option<u64>, role: Role, label: &str, frame: [f64; 4]) -> Node {
        Node { id, parent, role, label: Some(label.into()), value: None, hint: None, identifier: None, traits: 0, frame, activation_point: [0.0; 2] }
    }

    #[test]
    fn nodes_sit_where_they_are_drawn_under_the_window() {
        let nodes = [n(1, None, Role::Window, "", [0.0, 0.0, 390.0, 844.0]), n(2, Some(1), Role::Button, "OK", [20.0, 100.0, 50.0, 44.0])];
        let place = Place { x: 30.0, y: 40.0, k: 2.0 };
        let u = update("pomme", &nodes, place, Some(2));
        assert_eq!(u.focus, NodeId(2));
        assert_eq!(u.nodes[0].1.children(), &[NodeId(1)]);
        assert_eq!(u.nodes[1].1.children(), &[NodeId(2)]);
        let ok = &u.nodes[2].1;
        assert_eq!(ok.role(), AkRole::Button);
        assert_eq!(ok.label(), Some("OK"));
        assert_eq!(ok.bounds(), Some(Rect { x0: 70.0, y0: 240.0, x1: 170.0, y1: 328.0 }));
        assert!(ok.supports_action(AkAction::Click));
        // A focus that's gone falls back to the window.
        assert_eq!(update("pomme", &nodes[..1], place, Some(2)).focus, ROOT);
    }

    #[test]
    fn actions_map_to_voiceover_s() {
        let req = |action, node| ActionRequest { action, target_tree: TreeId::ROOT, target_node: NodeId(node), data: None };
        assert_eq!(action(&req(AkAction::Click, 7)), Some((7, Action::Activate)));
        assert_eq!(action(&req(AkAction::Increment, 7)), Some((7, Action::Increment)));
        assert_eq!(action(&req(AkAction::Click, ROOT.0)), None);
    }
}
