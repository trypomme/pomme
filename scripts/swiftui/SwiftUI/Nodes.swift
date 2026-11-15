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
  /// Its focus, if a FocusState follows it (`.focused`): whether it has it,
  /// and taking or giving it up.
  var focus: (get: () -> Bool, set: (Bool) -> Void)?
  /// What it's known by (`.tag(_:)`): a picker's option, a tab.
  var tag: AnyHashable?
  /// What a tab bar shows for it, if it's a tab (`.tabItem`, a `Tab`'s title and image).
  var tabItem: TabItem?

  init(path: ViewPath) {
    self.path = path
  }

  /// What tapping the list row it's the content of does: a button's action
  /// (on iOS the whole row is the button).
  var rowAction: (@MainActor () -> Void)? { nil }

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

  /// Where its first text starts, from its leading edge, once placed (a list
  /// row's separator starts there: SwiftUI's `listRowSeparatorLeading`); nil
  /// without text.
  var firstTextLeading: CGFloat? { nil }

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
  /// The view the tree is shown in, and the safe area its content keeps clear of.
  private(set) weak var root: UIView?
  private(set) var safeArea: UIEdgeInsets = .zero
  /// Whether the content keeps clear of the safe area.
  private(set) var respectsSafeArea = true

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
    root = view
    respectsSafeArea = respectingSafeArea
    safeArea = respectingSafeArea ? view.safeAreaInsets : .zero
    let bounds = view.bounds
    let safe = bounds.inset(by: safeArea)
    for node in nodes {
      let size = node.sizeThatFits(ProposedViewSize(safe.size))
      let frame = CGRect(x: safe.midX - size.width / 2, y: safe.midY - size.height / 2, width: size.width, height: size.height)
      node.place(in: frame, of: view, self)
    }
    for (path, v) in views where !used.contains(path) {
      v.removeFromSuperview()
      views[path] = nil
    }
  }
}

extension Renderer {
  /// `frame` (in `parent`) grown under the safe area at the edges where it meets
  /// it, as a list's or scroll view's background does; and how far it grew
  /// at each edge (its content's insets).
  func extendingUnderSafeArea(_ frame: CGRect, in parent: UIView) -> (frame: CGRect, insets: UIEdgeInsets) {
    guard let root else { return (frame, .zero) }
    let f = parent.convert(frame, to: root)
    let safe = root.bounds.inset(by: safeArea)
    let touching: (CGFloat, CGFloat) -> Bool = { abs($0 - $1) < 0.5 }
    var insets = UIEdgeInsets.zero
    if touching(f.minY, safe.minY) { insets.top = safeArea.top }
    if touching(f.maxY, safe.maxY) { insets.bottom = safeArea.bottom }
    if touching(f.minX, safe.minX) { insets.left = safeArea.left }
    if touching(f.maxX, safe.maxX) { insets.right = safeArea.right }
    let grown = CGRect(x: frame.minX - insets.left, y: frame.minY - insets.top,
                       width: frame.width + insets.left + insets.right, height: frame.height + insets.top + insets.bottom)
    return (grown, insets)
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
    // A stack's spacers grow along its axis.
    if let stack = layout as? any PommeStackLayout {
      for case let spacer as SpacerNode in children {
        spacer.axis = stack.pommeEngine.axis
      }
      for case let divider as DividerNode in children {
        divider.axis = stack.pommeEngine.axis
      }
    }
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

  override var firstTextLeading: CGFloat? {
    children.lazy.compactMap { child in child.firstTextLeading.map { (child.placement?.minX ?? 0) + $0 } }.first
  }
}

// ---- Text ----

final class TextNode: Node {
  let string: String

  /// How text looks: its font, its color, and whether it's shown in capitals.
  struct Style {
    var font: UIFont
    var color: UIColor
    var uppercase = false

    static let body = Style(font: .preferredFont(forTextStyle: .body), color: .label)
    /// A list section's header and footer.
    /// (iOS 26: the headline style, in the secondary color, as written.)
    static let sectionHeader = Style(font: .preferredFont(forTextStyle: .headline), color: .secondaryLabel)
    static let sectionFooter = Style(font: .preferredFont(forTextStyle: .footnote), color: .secondaryLabel)
  }

  /// How text looks where it is (a list's section header, say).
  var style = Style.body

  /// The font and weight the view gives it (its environment's, or its own
  /// modifiers'), and its own color: over the style.
  var font: Font?
  var weight: Font.Weight?
  var color: UIColor?

  init(path: ViewPath, string: String) {
    self.string = string
    super.init(path: path)
  }

  override var firstTextLeading: CGFloat? { 0 }

  /// How it's drawn: the style, with what the view gives it.
  var resolved: Style {
    var s = style
    if let font {
      s.font = font.pommeUIFont
    }
    if let weight {
      s.font = s.font.pommeWeighted(weight)
    }
    if let color {
      s.color = color
    }
    return s
  }

  private var shown: String { style.uppercase ? string.uppercased() : string }

  /// Measures text without a view of its own.
  private static let measurer: UILabel = {
    let l = UILabel()
    l.numberOfLines = 0
    return l
  }()

  /// SF Pro's line metrics, which iOS lays text out with (pomme draws its
  /// stand-in, Inter): a line is 1.196 × the size tall, and each further one
  /// 1.294 × the size further down (20.33 and 22 at 17 pt, measured with the
  /// swiftuimetrics probe on the iOS 26 simulator).
  static func lineHeight(_ font: UIFont) -> CGFloat { font.pointSize * (20.333 / 17) }
  static func lineSpacing(_ font: UIFont) -> CGFloat { font.pointSize * (22.0 / 17) }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    let style = resolved
    let label = Self.measurer
    label.font = style.font
    label.text = shown
    let width = proposal.width ?? .greatestFiniteMagnitude
    // Its own width: on one line if that fits, else what it's offered.
    let natural = ceil(label.sizeThatFits(CGSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)).width)
    let size = label.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
    // As many lines as the text wraps to, at SF Pro's spacing.
    let lines = natural <= width ? 1 : max(1, (size.height / style.font.lineHeight).rounded())
    let height = Self.lineHeight(style.font) + (lines - 1) * Self.lineSpacing(style.font)
    return CGSize(width: min(natural, width), height: height)
  }

  /// Its baselines: the first line's and the last's.
  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? {
    let style = resolved
    if key == VerticalAlignment.firstTextBaseline.key {
      return style.font.ascender
    }
    if key == VerticalAlignment.lastTextBaseline.key {
      return size.height + style.font.descender
    }
    return nil
  }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    let label: UILabel = renderer.view(for: path) {
      let l = UILabel()
      l.numberOfLines = 0
      return l
    }
    let style = resolved
    label.font = style.font
    label.textColor = style.color
    if label.text != shown {
      label.text = shown
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

  override var rowAction: (@MainActor () -> Void)? { child.rowAction }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize { child.sizeThatFits(proposal) }

  override func explicitGuide(_ key: AlignmentKey, size: CGSize) -> CGFloat? { child.explicitGuide(key, size: size) }

  override var firstTextLeading: CGFloat? { child.firstTextLeading }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {
    child.place(in: frame, of: parent, renderer)
    // Its safe-area insets: the safe area's, at the edges where it meets it.
    let safe = renderer.extendingUnderSafeArea(frame, in: parent).insets
    let proxy = GeometryProxy(
      size: frame.size,
      safeArea: EdgeInsets(top: safe.top, leading: safe.left, bottom: safe.bottom, trailing: safe.right)
    ) { space in
      switch space {
      case .local: return CGRect(origin: .zero, size: frame.size)
      // (Named spaces: the window's, the nearest one pomme knows.)
      case .global, .named: return parent.convert(frame, to: nil)
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

// ---- Spacers ----

extension Spacer: PrimitiveView {
  func pommeResolve(at path: ViewPath, in graph: ViewGraph, _ preferences: Preferences) -> [Node] {
    [SpacerNode(path: path, minLength: minLength)]
  }
}

/// Space that grows along its stack's axis (both axes outside a stack), never
/// shorter than its minimum length (the system's spacing, by default).
final class SpacerNode: Node {
  let minLength: CGFloat
  /// The axis of the stack it's in (set by the stack's node).
  var axis: Axis?

  init(path: ViewPath, minLength: CGFloat?) {
    self.minLength = minLength ?? 8
    super.init(path: path)
  }

  override func sizeThatFits(_ proposal: ProposedViewSize) -> CGSize {
    switch axis {
    case .horizontal: return CGSize(width: max(minLength, proposal.width ?? 0), height: 0)
    case .vertical: return CGSize(width: 0, height: max(minLength, proposal.height ?? 0))
    case nil: return CGSize(width: max(minLength, proposal.width ?? 0), height: max(minLength, proposal.height ?? 0))
    }
  }

  /// No space between a spacer and its neighbours: it is the space.
  nonisolated override var pommeSpacing: ViewSpacing { .zero }

  override func place(in frame: CGRect, of parent: UIView, _ renderer: Renderer) {}
}
