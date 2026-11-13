// Nodes: what views resolve to, laid out the way SwiftUI lays out (a parent
// proposes a size, the child chooses its own, the parent places it) and shown
// with UIKit views.

import UIKit

/// A resolved view: laid out, then shown with UIKit.
@MainActor
class Node {
  let path: ViewPath
  var accessibilityIdentifier: String?

  init(path: ViewPath) {
    self.path = path
  }

  /// Whether it takes the whole screen, under the bars, as lists do (its
  /// content keeps clear of them itself).
  var fillsScreen: Bool { false }

  /// The size it takes when offered `proposal`.
  func sizeThatFits(_ proposal: CGSize, _ renderer: Renderer) -> CGSize {
    .zero
  }

  /// Show it in `frame` of `parent`.
  func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {}
}

/// The UIKit views that show nodes, kept for the nodes' paths from one update
/// to the next; those of nodes that have gone are removed.
@MainActor
final class Renderer {
  private var views: [ViewPath: UIView] = [:]
  private var used: Set<ViewPath> = []

  func view<V: UIView>(for path: ViewPath, make: () -> V) -> V {
    used.insert(path)
    if let view = views[path] as? V {
      return view
    }
    views[path]?.removeFromSuperview()
    let view = make()
    views[path] = view
    return view
  }

  /// Show `nodes` in `view` as a window's (or a navigation stack's) content.
  func show(_ nodes: [Node], in view: UIView) {
    used = []
    let bounds = view.bounds
    let safe = bounds.inset(by: view.safeAreaInsets)
    for node in nodes {
      if node.fillsScreen {
        node.place(in: bounds, of: view, self)
      } else {
        // Centered in the safe area, at the size it chooses.
        let size = node.sizeThatFits(safe.size, self)
        let frame = CGRect(x: safe.midX - size.width / 2, y: safe.midY - size.height / 2, width: size.width, height: size.height)
        node.place(in: frame, of: view, self)
      }
    }
    for (path, v) in views where !used.contains(path) {
      v.removeFromSuperview()
      views[path] = nil
    }
  }
}

extension UIView {
  /// Add `child` if it isn't a subview already.
  func pommeAdopt(_ child: UIView) {
    if child.superview !== self {
      addSubview(child)
    }
  }
}

// ---- Text ----

final class TextNode: Node {
  let string: String

  init(path: ViewPath, string: String) {
    self.string = string
    super.init(path: path)
  }

  private func label(_ renderer: Renderer) -> UILabel {
    let label: UILabel = renderer.view(for: path) {
      let l = UILabel()
      l.numberOfLines = 0
      l.font = .preferredFont(forTextStyle: .body)
      l.textColor = .label
      return l
    }
    if label.text != string {
      label.text = string
    }
    label.accessibilityIdentifier = accessibilityIdentifier
    return label
  }

  override func sizeThatFits(_ proposal: CGSize, _ renderer: Renderer) -> CGSize {
    let size = label(renderer).sizeThatFits(CGSize(width: proposal.width, height: .greatestFiniteMagnitude))
    return CGSize(width: min(ceil(size.width), proposal.width), height: ceil(size.height))
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let label = label(renderer)
    label.frame = frame
    parent.pommeAdopt(label)
  }
}
