// Nodes: what views resolve to. They lay out the way SwiftUI lays out (a
// parent proposes a size, the child chooses its own, the parent places it),
// through the same Layout protocol apps use, and are shown with UIKit views.

import UIKit

/// A resolved view: laid out, then shown with UIKit. To layouts, it's a subview.
@MainActor
class Node: LayoutProxy {
  let path: ViewPath
  var accessibilityIdentifier: String?
  /// Its layout priority (`.layoutPriority`).
  var priority: Double = 0
  /// Values for its parent layout (`.layoutValue`), by key type.
  var layoutValues: [ObjectIdentifier: Any] = [:]
  /// Where its parent layout placed it, in the parent's coordinates.
  fileprivate(set) var placement: CGRect?

  init(path: ViewPath) {
    self.path = path
  }

  /// Whether it takes the whole screen, under the bars, as lists do (its
  /// content keeps clear of them itself).
  var fillsScreen: Bool { false }

  /// The size it takes when offered `proposal` (a nil dimension: its ideal size).
  func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    .zero
  }

  /// A guide it sets itself (a text baseline …), in a view of `size`.
  func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    nil
  }

  /// Show it in `frame` of `parent`.
  func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {}

  // ---- LayoutProxy ----

  nonisolated func pommeSizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    MainActor.assumeIsolated { sizeThatFits(proposal) }
  }

  nonisolated func pommeDimensions(in proposal: ProposedViewSize) -> ViewDimensions {
    MainActor.assumeIsolated {
      let size = sizeThatFits(proposal)
      return ViewDimensions(size: size) { [self] key in MainActor.assumeIsolated { explicitGuide(key, size: size) } }
    }
  }

  nonisolated func pommePlace(at position: CGPoint, anchor: UnitPoint, proposal: ProposedViewSize) {
    MainActor.assumeIsolated {
      let size = sizeThatFits(proposal)
      placement = CGRect(x: position.x - anchor.x * size.width, y: position.y - anchor.y * size.height, width: size.width, height: size.height)
    }
  }

  nonisolated var pommePriority: Double { MainActor.assumeIsolated { priority } }

  nonisolated var pommeSpacing: ViewSpacing { ViewSpacing() }

  nonisolated func pommeLayoutValue(_ key: ObjectIdentifier) -> Any? {
    MainActor.assumeIsolated { layoutValues[key] }
  }
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

  /// Show `nodes` in `view` as the root of a window (or a navigation stack's
  /// screen): offered the safe area, and centered in it.
  func show(_ nodes: [Node], in view: UIView, respectingSafeArea: Bool = true) {
    used = []
    let bounds = view.bounds
    let safe = respectingSafeArea ? bounds.inset(by: view.safeAreaInsets) : bounds
    for node in nodes {
      if node.fillsScreen {
        node.place(in: bounds, of: view, self)
      } else {
        let size = node.sizeThatFits(ProposedViewSize(safe.size))
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

/// A view that only holds others (a layout's container).
final class ContainerView: UIView {}

// ---- Layouts ----

/// A layout (a stack, `AnyLayout`, an app's own) and the views it lays out.
final class LayoutNode: Node {
  let layout: any Layout
  let children: [Node]

  init(path: ViewPath, layout: any Layout, children: [Node]) {
    self.layout = layout
    self.children = children
    super.init(path: path)
  }

  private var subviews: LayoutSubviews {
    LayoutSubviews(children.map { LayoutSubview($0) })
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    func run<L: Layout>(_ l: L) -> CGSize {
      let subviews = subviews
      var cache = l.makeCache(subviews: subviews)
      return l.sizeThatFits(proposal: proposal, subviews: subviews, cache: &cache)
    }
    return run(layout)
  }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    func run<L: Layout>(_ l: L) -> CGFloat? {
      let subviews = subviews
      var cache = l.makeCache(subviews: subviews)
      let bounds = CGRect(origin: .zero, size: size)
      // (The key is one axis's guide; a layout answers for the guides it knows.)
      if let h = l.explicitAlignment(of: HorizontalAlignment(pommeKey: key), in: bounds, proposal: ProposedViewSize(size), subviews: subviews, cache: &cache) {
        return h
      }
      return l.explicitAlignment(of: VerticalAlignment(pommeKey: key), in: bounds, proposal: ProposedViewSize(size), subviews: subviews, cache: &cache)
    }
    return run(layout)
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let container: ContainerView = renderer.view(for: path) { ContainerView() }
    container.frame = frame
    container.accessibilityIdentifier = accessibilityIdentifier
    parent.pommeAdopt(container)
    func run<L: Layout>(_ l: L) {
      let subviews = subviews
      var cache = l.makeCache(subviews: subviews)
      l.placeSubviews(in: CGRect(origin: .zero, size: frame.size), proposal: ProposedViewSize(frame.size), subviews: subviews, cache: &cache)
    }
    run(layout)
    for child in children {
      child.place(in: child.placement ?? .zero, of: container, renderer)
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

  static let font = UIFont.preferredFont(forTextStyle: .body)

  /// Measures text without a view of its own.
  private static let measurer: UILabel = {
    let l = UILabel()
    l.numberOfLines = 0
    l.font = font
    return l
  }()

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let label = Self.measurer
    label.text = string
    let width = proposal.width ?? .greatestFiniteMagnitude
    let size = label.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
    return CGSize(width: min(ceil(size.width), width), height: ceil(size.height))
  }

  /// Its baselines: the first line's and the last's.
  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    let font = Self.font
    if key == VerticalAlignment.firstTextBaseline.key {
      return font.ascender
    }
    if key == VerticalAlignment.lastTextBaseline.key {
      return size.height + font.descender
    }
    return nil
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let label: UILabel = renderer.view(for: path) {
      let l = UILabel()
      l.numberOfLines = 0
      l.font = Self.font
      l.textColor = .label
      return l
    }
    if label.text != string {
      label.text = string
    }
    label.accessibilityIdentifier = accessibilityIdentifier
    label.frame = frame
    parent.pommeAdopt(label)
  }
}

// ---- Geometry ----

/// A view whose geometry something follows (`onGeometryChange`): laid out as
/// its content; once placed, its geometry is given to `observe`.
final class GeometryNode: Node {
  let child: Node
  let observe: (GeometryProxy) -> Void

  init(path: ViewPath, child: Node, observe: @escaping (GeometryProxy) -> Void) {
    self.child = child
    self.observe = observe
    super.init(path: path)
  }

  override var fillsScreen: Bool { child.fillsScreen }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { child.sizeThatFits(proposal) }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? { child.explicitGuide(key, size: size) }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    child.place(in: frame, of: parent, renderer)
    let window = parent.window
    let safe = parent.safeAreaInsets
    let proxy = GeometryProxy(
      size: frame.size,
      safeArea: EdgeInsets(top: max(safe.top - frame.minY, 0), leading: max(safe.left - frame.minX, 0),
                           bottom: max(safe.bottom - (parent.bounds.height - frame.maxY), 0), trailing: max(safe.right - (parent.bounds.width - frame.maxX), 0))
    ) { space in
      switch space {
      case .local: return CGRect(origin: .zero, size: frame.size)
      // (Named spaces: the hosting view's, the nearest one pomme knows.)
      case .global, .named: return window.map { parent.convert(frame, to: $0) } ?? frame
      }
    }
    observe(proxy)
  }
}

extension _GeometryActionModifier: PrimitiveModifier {
  /// The action runs after the layout pass, when the value has changed (and the first time).
  func pommeResolve(content: () -> [Node], at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    let compute = pommeValue
    let action = action
    return content().map { node in
      GeometryNode(path: node.path.appending("geometry"), child: node) { proxy in
        let value = compute(proxy)
        guard graph.geometryChanged(at: path, to: value) else { return }
        DispatchQueue.main.async { action(value) }
      }
    }
  }
}
